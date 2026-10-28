//! Orchestration engine coordinating discovery, bounded file reading, dialect parsing,
//! AST scanning, rule evaluation, and deterministic report generation.

#![allow(clippy::disallowed_methods)]

use crate::cache::{compute_config_hash, compute_content_hash, ScanCache, DEFAULT_CACHE_FILE};
use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::report::Report;
use crate::diagnostics::severity::Severity;
use crate::dialect::{self, ParseOutput};
use crate::discover::{self, DiscoveryOptions};
use crate::model::path::RelPath;
use crate::model::span::{Location, Span};
use crate::rules::{self, AuditOptions, Contract};
use crate::scan;
use rayon::prelude::*;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Engine configuration parameters.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Path to the contract file (e.g. `.env.example`).
    pub contract_path: PathBuf,
    /// Optional path to real `.env` file to audit.
    pub env_path: Option<PathBuf>,
    /// Optional ordered paths to layered `.env` files (e.g. `.env.local`, `.env`).
    pub env_paths: Vec<PathBuf>,
    /// Target directories or files to scan.
    pub scan_paths: Vec<PathBuf>,
    /// Whether to include ignored/hidden files.
    pub include_ignored: bool,
    /// Maximum file size to scan in bytes (default: 2 MB).
    pub max_file_size: u64,
    /// Maximum total files to scan (default: 10,000).
    pub max_files: usize,
    /// Maximum directory recursion depth (default: 30).
    pub max_depth: usize,
    /// Maximum allowed length for a single line in bytes (default: 65,536).
    pub max_line_length: usize,
    /// Maximum total bytes across all scanned source files (default: 100 MB).
    pub max_total_bytes: u64,
    /// Maximum total diagnostics before capping emission (default: 2,000).
    pub max_diagnostics: usize,
    /// Audit options (e.g. ignored keys).
    pub audit_options: AuditOptions,
    /// Dialect parsing options.
    pub dialect_options: dialect::DialectOptions,
    /// Automatically append missing contract variables to contract file.
    pub fix: bool,
    /// Baseline file to filter known legacy diagnostics.
    pub baseline: Option<PathBuf>,
    /// Save current diagnostics as a new baseline file.
    pub generate_baseline: Option<PathBuf>,
    /// Whether to enable result caching across runs (default: true).
    pub use_cache: bool,
    /// Path to cache file (default: `.envdoctor-cache`).
    pub cache_path: Option<PathBuf>,
    /// Configured languages to scan ('default' for js/ts/rust, 'all', or specific: 'js,rust,python,go,infra').
    pub languages: Vec<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            contract_path: PathBuf::from(".env.example"),
            env_path: None,
            env_paths: Vec::new(),
            scan_paths: vec![PathBuf::from(".")],
            include_ignored: false,
            max_file_size: 2 * 1024 * 1024,
            max_files: 10_000,
            max_depth: 30,
            max_line_length: 64 * 1024,
            max_total_bytes: 100 * 1024 * 1024,
            max_diagnostics: 2_000,
            audit_options: AuditOptions::default(),
            dialect_options: dialect::DialectOptions::default(),
            fix: false,
            baseline: None,
            generate_baseline: None,
            use_cache: true,
            cache_path: None,
            languages: vec!["all".to_string()],
        }
    }
}

enum EnvReadResult {
    Ok(ParseOutput),
    NotFound,
    LimitExceeded(u64),
    NotAFile,
    IoError(String),
    InvalidUtf8,
    LineTooLong,
}

fn read_env_file_bounded(
    path: &Path,
    max_bytes: u64,
    max_line_len: usize,
    rel_path: RelPath,
    options: &dialect::DialectOptions,
    contract_defs: Option<&[crate::model::definition::Definition]>,
) -> EnvReadResult {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return EnvReadResult::NotFound;
            }
            return EnvReadResult::IoError(e.to_string());
        }
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(e) => return EnvReadResult::IoError(e.to_string()),
    };

    let file_type = metadata.file_type();
    if !file_type.is_file() {
        return EnvReadResult::NotAFile;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if file_type.is_fifo()
            || file_type.is_socket()
            || file_type.is_char_device()
            || file_type.is_block_device()
        {
            return EnvReadResult::NotAFile;
        }
    }

    let len = metadata.len();
    if len > max_bytes {
        return EnvReadResult::LimitExceeded(len);
    }

    // Read directly into Zeroizing buffer to avoid leaving secrets in un-wiped heap memory.
    // Allocate extra capacity (+128) so read_to_end EOF probe does not reallocate buffer on heap.
    let alloc_size = (len.min(max_bytes) as usize) + 128;
    let mut zeroizing_bytes = zeroize::Zeroizing::new(Vec::with_capacity(alloc_size));
    let mut reader = (&mut file).take(max_bytes + 1);
    if let Err(e) = reader.read_to_end(&mut zeroizing_bytes) {
        return EnvReadResult::IoError(e.to_string());
    }

    if zeroizing_bytes.len() as u64 > max_bytes {
        return EnvReadResult::LimitExceeded(zeroizing_bytes.len() as u64);
    }

    let utf8_slice = match std::str::from_utf8(&zeroizing_bytes) {
        Ok(s) => s,
        Err(_) => return EnvReadResult::InvalidUtf8,
    };

    for line in utf8_slice.lines() {
        if line.len() > max_line_len {
            return EnvReadResult::LineTooLong;
        }
    }

    let mut parse_output = dialect::parse_with_options(utf8_slice, rel_path.clone(), options);

    if let Some(defs) = contract_defs {
        validate_env_values_against_contract(utf8_slice, defs, &mut parse_output.diagnostics);
    }

    EnvReadResult::Ok(parse_output)
}

fn validate_env_values_against_contract(
    content: &str,
    contract_defs: &[crate::model::definition::Definition],
    diagnostics: &mut Vec<dialect::DialectDiagnostic>,
) {
    let def_map: std::collections::HashMap<&str, &crate::model::definition::Definition> =
        contract_defs.iter().map(|d| (d.name.as_str(), d)).collect();

    for (line_idx, line) in content.lines().enumerate() {
        let line_num = (line_idx + 1) as u32;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((raw_key, raw_val)) = trimmed.split_once('=') {
            let key = raw_key.trim();
            if let Some(contract_def) = def_map.get(key) {
                let mut val = raw_val.trim();
                if ((val.starts_with('"') && val.ends_with('"'))
                    || (val.starts_with('\'') && val.ends_with('\'')))
                    && val.len() >= 2
                {
                    val = &val[1..val.len() - 1];
                }

                if !val.is_empty() {
                    if let Err(reason) = contract_def.annotations.validate_value(val) {
                        diagnostics.push(
                            dialect::DialectDiagnostic::new(
                                DiagnosticCode::ENV030,
                                format!(
                                    "Value for '{}' does not match contract type: {}",
                                    key, reason
                                ),
                                Span::point(line_num, 1, 0),
                                Some(contract_def.name.clone()),
                            )
                            .with_severity(Severity::Error),
                        );
                    }
                }
            }
        }
    }
}

enum SourceReadResult {
    Ok(String, u64, String),
    LimitExceeded(u64),
    NotAFile,
    IoError(String),
    InvalidUtf8,
    LineTooLong,
}

fn read_source_file_bounded(path: &Path, max_bytes: u64, max_line_len: usize) -> SourceReadResult {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return SourceReadResult::IoError(e.to_string()),
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(e) => return SourceReadResult::IoError(e.to_string()),
    };

    let file_type = metadata.file_type();
    if !file_type.is_file() {
        return SourceReadResult::NotAFile;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if file_type.is_fifo()
            || file_type.is_socket()
            || file_type.is_char_device()
            || file_type.is_block_device()
        {
            return SourceReadResult::NotAFile;
        }
    }

    let len = metadata.len();
    if len > max_bytes {
        return SourceReadResult::LimitExceeded(len);
    }

    let mut bytes = Vec::with_capacity(len.min(max_bytes) as usize);
    let mut reader = (&mut file).take(max_bytes + 1);
    if let Err(e) = reader.read_to_end(&mut bytes) {
        return SourceReadResult::IoError(e.to_string());
    }

    let actual_len = bytes.len() as u64;
    if actual_len > max_bytes {
        return SourceReadResult::LimitExceeded(actual_len);
    }

    let content_hash = compute_content_hash(&bytes);

    let content = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return SourceReadResult::InvalidUtf8,
    };

    for line in content.lines() {
        if line.len() > max_line_len {
            return SourceReadResult::LineTooLong;
        }
    }

    SourceReadResult::Ok(content, actual_len, content_hash)
}

/// Executes a full audit pass given an engine configuration.
pub fn run_engine(config: &EngineConfig) -> Report {
    // Configure bounded 4MB worker thread stacks on Rayon's global threadpool
    let _ = rayon::ThreadPoolBuilder::new()
        .stack_size(4 * 1024 * 1024)
        .build_global();

    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut scanned_files = 0;
    let mut skipped_files = 0;
    let mut complete = true;
    let mut total_bytes_scanned: u64 = 0;

    // 1. Process Contract File
    let mut contract: Option<Contract> = None;
    let contract_rel = RelPath::new(&config.contract_path);

    match read_env_file_bounded(
        &config.contract_path,
        config.max_file_size,
        config.max_line_length,
        contract_rel.clone(),
        &config.dialect_options,
        None,
    ) {
        EnvReadResult::Ok(parse_output) => {
            diagnostics.extend(
                parse_output
                    .diagnostics
                    .into_iter()
                    .map(|d| d.into_diagnostic(contract_rel.clone())),
            );
            contract = Some(Contract::new(
                contract_rel.clone(),
                parse_output.definitions,
            ));
        }
        EnvReadResult::NotFound => {
            // Contract not found -> evaluate_contract will emit ENV020
            contract = None;
        }
        EnvReadResult::LimitExceeded(len) => {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::IO010,
                Severity::Error,
                format!(
                    "Contract file '{}' exceeds maximum allowed size ({} bytes > {} bytes)",
                    contract_rel, len, config.max_file_size
                ),
                None,
                vec![Location::new(contract_rel.clone(), Span::point(1, 1, 0))],
                None,
            ));
            complete = false;
        }
        EnvReadResult::NotAFile => {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::IO003,
                Severity::Error,
                format!(
                    "Contract path '{}' exists but is not a regular file",
                    contract_rel
                ),
                None,
                vec![Location::new(contract_rel.clone(), Span::point(1, 1, 0))],
                None,
            ));
            complete = false;
        }
        EnvReadResult::IoError(err) => {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::IO001,
                Severity::Error,
                format!("Failed to read contract file '{}': {}", contract_rel, err),
                None,
                vec![Location::new(contract_rel.clone(), Span::point(1, 1, 0))],
                None,
            ));
            complete = false;
        }
        EnvReadResult::InvalidUtf8 => {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::IO002,
                Severity::Error,
                format!(
                    "Contract file '{}' contains invalid UTF-8 bytes",
                    contract_rel
                ),
                None,
                vec![Location::new(contract_rel.clone(), Span::point(1, 1, 0))],
                None,
            ));
            complete = false;
        }
        EnvReadResult::LineTooLong => {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::IO010,
                Severity::Error,
                format!(
                    "Contract file '{}' contains line exceeding maximum line length ({} bytes)",
                    contract_rel, config.max_line_length
                ),
                None,
                vec![Location::new(contract_rel.clone(), Span::point(1, 1, 0))],
                None,
            ));
            complete = false;
        }
    }

    // 2. Process Environment Files (support multiple layered files with documented precedence)
    let mut effective_env_paths = config.env_paths.clone();
    if effective_env_paths.is_empty() {
        if let Some(ref env_path) = config.env_path {
            effective_env_paths.push(env_path.clone());
        }
    }

    let mut combined_env_defs: Vec<crate::model::definition::Definition> = Vec::new();
    let mut seen_env_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    for env_path in &effective_env_paths {
        let env_rel = RelPath::new(env_path);
        let contract_defs = contract.as_ref().map(|c| c.definitions.as_slice());
        match read_env_file_bounded(
            env_path,
            config.max_file_size,
            config.max_line_length,
            env_rel.clone(),
            &config.dialect_options,
            contract_defs,
        ) {
            EnvReadResult::Ok(parse_output) => {
                diagnostics.extend(
                    parse_output
                        .diagnostics
                        .into_iter()
                        .map(|d| d.into_diagnostic(env_rel.clone())),
                );
                for def in parse_output.definitions {
                    let key = def.name.as_str().to_string();
                    if !seen_env_keys.contains(&key) {
                        seen_env_keys.insert(key);
                        combined_env_defs.push(def);
                    }
                }
            }
            EnvReadResult::NotFound => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::IO001,
                    Severity::Error,
                    format!("Specified environment file '{}' was not found", env_rel),
                    None,
                    vec![Location::new(env_rel.clone(), Span::point(1, 1, 0))],
                    None,
                ));
                complete = false;
            }
            EnvReadResult::LimitExceeded(len) => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::IO010,
                    Severity::Error,
                    format!(
                        "Environment file '{}' exceeds maximum allowed size ({} bytes > {} bytes)",
                        env_rel, len, config.max_file_size
                    ),
                    None,
                    vec![Location::new(env_rel.clone(), Span::point(1, 1, 0))],
                    None,
                ));
                complete = false;
            }
            EnvReadResult::NotAFile => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::IO003,
                    Severity::Error,
                    format!(
                        "Environment path '{}' exists but is not a regular file",
                        env_rel
                    ),
                    None,
                    vec![Location::new(env_rel.clone(), Span::point(1, 1, 0))],
                    None,
                ));
                complete = false;
            }
            EnvReadResult::IoError(err) => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::IO001,
                    Severity::Error,
                    format!("Failed to read environment file '{}': {}", env_rel, err),
                    None,
                    vec![Location::new(env_rel.clone(), Span::point(1, 1, 0))],
                    None,
                ));
                complete = false;
            }
            EnvReadResult::InvalidUtf8 => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::IO002,
                    Severity::Error,
                    format!(
                        "Environment file '{}' contains invalid UTF-8 bytes",
                        env_rel
                    ),
                    None,
                    vec![Location::new(env_rel.clone(), Span::point(1, 1, 0))],
                    None,
                ));
                complete = false;
            }
            EnvReadResult::LineTooLong => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::IO010,
                    Severity::Error,
                    format!(
                        "Environment file '{}' contains line exceeding maximum line length ({} bytes)",
                        env_rel, config.max_line_length
                    ),
                    None,
                    vec![Location::new(env_rel.clone(), Span::point(1, 1, 0))],
                    None,
                ));
                complete = false;
            }
        }
    }

    let env_definitions = if !effective_env_paths.is_empty() {
        Some(combined_env_defs)
    } else {
        None
    };

    // 3. File Discovery
    let discovery_options = DiscoveryOptions {
        paths: config.scan_paths.clone(),
        include_ignored: config.include_ignored,
        follow_symlinks: false,
        max_files: config.max_files,
        max_depth: config.max_depth,
        languages: config.languages.clone(),
    };
    let discovered = discover::discover_files(&discovery_options);

    if discovered.file_count_limit_exceeded {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::IO010,
            Severity::Error,
            format!(
                "Scan aborted: file count limit ({}) exceeded during discovery",
                config.max_files
            ),
            None,
            vec![],
            None,
        ));
        complete = false;
    }

    if discovered.depth_limit_exceeded {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::IO010,
            Severity::Error,
            format!(
                "Directory recursion depth limit ({}) exceeded during discovery",
                config.max_depth
            ),
            None,
            vec![],
            None,
        ));
        complete = false;
    }

    // 4. Source Scanning (Parallelized via Rayon with bounded chunking and caching)
    let cache_path = config
        .cache_path
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CACHE_FILE));

    let config_hash = compute_config_hash(
        config.max_file_size,
        config.max_line_length,
        config.max_total_bytes,
        config.max_diagnostics,
        config.dialect_options.duplicate_strategy as u8,
        &config.languages,
    );
    let languages_str = config.languages.join(",");

    let mut cache = if config.use_cache {
        ScanCache::load_matching(&cache_path, &config_hash, &languages_str).unwrap_or_else(|| {
            let mut c = ScanCache::new();
            c.config_hash = config_hash.clone();
            c.languages = languages_str.clone();
            c
        })
    } else {
        ScanCache::new()
    };

    let mut all_references = Vec::new();

    // Filter out contract file and active .env files
    let candidate_files: Vec<PathBuf> = discovered
        .files
        .into_iter()
        .filter(|file_path| {
            let rel_path = RelPath::new(file_path);
            if rel_path == contract_rel {
                return false;
            }
            if let Some(ref env_path) = config.env_path {
                if rel_path == RelPath::new(env_path) {
                    return false;
                }
            }
            for ep in &effective_env_paths {
                if rel_path == RelPath::new(ep) {
                    return false;
                }
            }
            true
        })
        .collect();

    enum Outcome {
        Cached {
            bytes_read: u64,
            references: Vec<crate::model::reference::Reference>,
            diagnostics: Vec<Diagnostic>,
        },
        Fresh {
            rel_path: RelPath,
            bytes_read: u64,
            content_hash: String,
            references: Vec<crate::model::reference::Reference>,
            diagnostics: Vec<Diagnostic>,
        },
        LimitExceeded {
            rel_path: RelPath,
            len: u64,
        },
        NotAFile {
            rel_path: RelPath,
        },
        IoError {
            rel_path: RelPath,
            err: String,
        },
        InvalidUtf8 {
            rel_path: RelPath,
        },
        LineTooLong {
            rel_path: RelPath,
        },
    }

    const BATCH_SIZE: usize = 64;
    let mut scan_halted = false;

    for batch in candidate_files.chunks(BATCH_SIZE) {
        if diagnostics.len() >= config.max_diagnostics {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::IO010,
                Severity::Warning,
                format!(
                    "Diagnostics cap ({}) reached; remaining findings truncated",
                    config.max_diagnostics
                ),
                None,
                vec![],
                None,
            ));
            complete = false;
            break;
        }

        let max_file_size = config.max_file_size;
        let max_line_length = config.max_line_length;
        let use_cache = config.use_cache;

        let batch_outcomes: Vec<Outcome> = batch
            .par_iter()
            .map(|file_path| {
                let rel_path = RelPath::new(file_path);
                match read_source_file_bounded(file_path, max_file_size, max_line_length) {
                    SourceReadResult::Ok(content, bytes_read, content_hash) => {
                        if use_cache {
                            if let Some((cached_refs, cached_diags)) =
                                cache.get(rel_path.as_str(), &content_hash)
                            {
                                return Outcome::Cached {
                                    bytes_read,
                                    references: cached_refs.to_vec(),
                                    diagnostics: cached_diags.to_vec(),
                                };
                            }
                        }
                        let scan_res = scan::scan_source(&content, rel_path.clone());
                        Outcome::Fresh {
                            rel_path,
                            bytes_read,
                            content_hash,
                            references: scan_res.references,
                            diagnostics: scan_res.diagnostics,
                        }
                    }
                    SourceReadResult::LimitExceeded(len) => {
                        Outcome::LimitExceeded { rel_path, len }
                    }
                    SourceReadResult::NotAFile => Outcome::NotAFile { rel_path },
                    SourceReadResult::IoError(err) => Outcome::IoError { rel_path, err },
                    SourceReadResult::InvalidUtf8 => Outcome::InvalidUtf8 { rel_path },
                    SourceReadResult::LineTooLong => Outcome::LineTooLong { rel_path },
                }
            })
            .collect();

        for outcome in batch_outcomes {
            match outcome {
                Outcome::Cached {
                    bytes_read,
                    references,
                    diagnostics: diags,
                } => {
                    total_bytes_scanned += bytes_read;
                    all_references.extend(references);
                    diagnostics.extend(diags);
                    scanned_files += 1;
                }
                Outcome::Fresh {
                    rel_path,
                    bytes_read,
                    content_hash,
                    references,
                    diagnostics: diags,
                } => {
                    total_bytes_scanned += bytes_read;
                    if total_bytes_scanned > config.max_total_bytes {
                        diagnostics.push(Diagnostic::new(
                            DiagnosticCode::IO010,
                            Severity::Error,
                            format!(
                                "Scan halted: total bytes scanned limit ({} bytes) exceeded",
                                config.max_total_bytes
                            ),
                            None,
                            vec![Location::new(rel_path.clone(), Span::point(1, 1, 0))],
                            None,
                        ));
                        complete = false;
                        scan_halted = true;
                        break;
                    }

                    if diags.iter().any(|d| d.code == DiagnosticCode::ENV011) {
                        complete = false;
                    }

                    if config.use_cache {
                        cache.insert(
                            rel_path.to_string(),
                            content_hash,
                            bytes_read,
                            references.clone(),
                            diags.clone(),
                        );
                    }

                    all_references.extend(references);
                    diagnostics.extend(diags);
                    scanned_files += 1;
                }
                Outcome::LimitExceeded { rel_path, len } => {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::IO010,
                        Severity::Error,
                        format!(
                            "Source file '{}' size ({} bytes) exceeds maximum limit ({} bytes)",
                            rel_path, len, config.max_file_size
                        ),
                        None,
                        vec![Location::new(rel_path, Span::point(1, 1, 0))],
                        None,
                    ));
                    skipped_files += 1;
                    complete = false;
                }
                Outcome::NotAFile { rel_path } => {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::IO003,
                        Severity::Error,
                        format!("Path '{}' is not a regular file", rel_path),
                        None,
                        vec![Location::new(rel_path, Span::point(1, 1, 0))],
                        None,
                    ));
                    skipped_files += 1;
                    complete = false;
                }
                Outcome::IoError { rel_path, err } => {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::IO001,
                        Severity::Error,
                        format!("Failed to read source file '{}': {}", rel_path, err),
                        None,
                        vec![Location::new(rel_path, Span::point(1, 1, 0))],
                        None,
                    ));
                    skipped_files += 1;
                    complete = false;
                }
                Outcome::InvalidUtf8 { rel_path } => {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::IO002,
                        Severity::Error,
                        format!("Source file '{}' contains invalid UTF-8 bytes", rel_path),
                        None,
                        vec![Location::new(rel_path, Span::point(1, 1, 0))],
                        None,
                    ));
                    skipped_files += 1;
                    complete = false;
                }
                Outcome::LineTooLong { rel_path } => {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::IO010,
                        Severity::Error,
                        format!(
                            "Source file '{}' contains line exceeding maximum line length ({} bytes)",
                            rel_path, config.max_line_length
                        ),
                        None,
                        vec![Location::new(rel_path, Span::point(1, 1, 0))],
                        None,
                    ));
                    skipped_files += 1;
                    complete = false;
                }
            }
        }

        if scan_halted {
            break;
        }
    }

    // Save cache after scan
    if config.use_cache {
        cache.config_hash = config_hash;
        cache.languages = languages_str;
        let active_paths: Vec<String> = candidate_files
            .iter()
            .map(|p| RelPath::new(p).to_string())
            .collect();
        cache.prune(&active_paths);
        let _ = cache.save(&cache_path);
    }

    // Deterministic sorting across all parallel threads
    all_references.sort();
    all_references.dedup();
    Diagnostic::sort_diagnostics(&mut diagnostics);

    // 5. Evaluate Contract Rules
    let rule_diagnostics = rules::evaluate_contract(
        contract.as_ref(),
        env_definitions.as_deref(),
        &all_references,
        &config.audit_options,
    );

    // 6. Handle --fix (generate missing contract entries with empty values; zero secret leakage!)
    if config.fix {
        let mut missing_keys: Vec<String> = rule_diagnostics
            .iter()
            .filter(|d| d.code == DiagnosticCode::ENV001)
            .filter_map(|d| d.key.as_ref().map(|k| k.as_str().to_string()))
            .collect();
        missing_keys.sort();
        missing_keys.dedup();
        if !missing_keys.is_empty() {
            use std::io::Write;
            if let Ok(mut f) = fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&config.contract_path)
            {
                for k in &missing_keys {
                    let _ = writeln!(f, "\n# Added by envdoctor --fix\n{}=", k);
                }
            }
        }
    }

    diagnostics.extend(rule_diagnostics);

    // 7. Inline suppressions (// envdoctor-ignore ENV001, # envdoctor-ignore ENV001)
    diagnostics.retain(|d| {
        if let Some(loc) = d.locations.first() {
            if let Ok(content) = fs::read_to_string(loc.path.as_str()) {
                let lines: Vec<&str> = content.lines().collect();
                let line_idx = loc.span.line.saturating_sub(1) as usize;
                if line_idx < lines.len()
                    && crate::suppression::is_line_suppressed(lines[line_idx], d.code.as_str())
                {
                    return false;
                }
                if line_idx > 0
                    && crate::suppression::is_line_suppressed(lines[line_idx - 1], d.code.as_str())
                {
                    return false;
                }
            }
        }
        true
    });

    // 8. Baseline file filtering and generation
    if let Some(ref baseline_path) = config.baseline {
        if let Ok(baseline_file) = crate::suppression::BaselineFile::load_from_path(baseline_path) {
            diagnostics = baseline_file.filter_diagnostics(diagnostics);
        }
    }

    if let Some(ref gen_path) = config.generate_baseline {
        let baseline_file = crate::suppression::BaselineFile::from_diagnostics(&diagnostics);
        let _ = baseline_file.save_to_path(gen_path);
    }

    // 9. Build Final Report (deterministic ordering and summary calculation)
    Report::new(diagnostics, scanned_files, skipped_files, complete)
}
