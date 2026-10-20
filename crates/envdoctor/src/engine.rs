//! Orchestration engine coordinating discovery, bounded file reading, dialect parsing,
//! AST scanning, rule evaluation, and deterministic report generation.

#![allow(clippy::disallowed_methods)]

use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::report::Report;
use crate::diagnostics::severity::Severity;
use crate::dialect;
use crate::discover::{self, DiscoveryOptions};
use crate::model::path::RelPath;
use crate::model::span::{Location, Span};
use crate::rules::{self, AuditOptions, Contract};
use crate::scan;
use std::fs;
use std::path::{Path, PathBuf};

/// Engine configuration parameters.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Path to the contract file (e.g. `.env.example`).
    pub contract_path: PathBuf,
    /// Optional path to real `.env` file to audit.
    pub env_path: Option<PathBuf>,
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
    /// Audit options (e.g. ignored keys).
    pub audit_options: AuditOptions,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            contract_path: PathBuf::from(".env.example"),
            env_path: None,
            scan_paths: vec![PathBuf::from(".")],
            include_ignored: false,
            max_file_size: 2 * 1024 * 1024,
            max_files: 10_000,
            max_depth: 30,
            audit_options: AuditOptions::default(),
        }
    }
}

enum ReadResult {
    Ok(String),
    LimitExceeded(u64),
    NotAFile,
    IoError(String),
    InvalidUtf8,
}

fn read_file_bounded(path: &Path, max_bytes: u64) -> ReadResult {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return ReadResult::IoError(e.to_string()),
    };

    if metadata.is_dir() {
        return ReadResult::NotAFile;
    }

    let len = metadata.len();
    if len > max_bytes {
        return ReadResult::LimitExceeded(len);
    }

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => return ReadResult::IoError(e.to_string()),
    };

    match String::from_utf8(bytes) {
        Ok(s) => ReadResult::Ok(s),
        Err(_) => ReadResult::InvalidUtf8,
    }
}

/// Executes a full audit pass given an engine configuration.
pub fn run_engine(config: &EngineConfig) -> Report {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut scanned_files = 0;
    let mut skipped_files = 0;
    let mut complete = true;

    // 1. Process Contract File
    let mut contract: Option<Contract> = None;
    let contract_rel = RelPath::new(&config.contract_path);

    if !config.contract_path.exists() {
        // Contract not found -> evaluate_contract will emit ENV020
        contract = None;
    } else {
        match read_file_bounded(&config.contract_path, config.max_file_size) {
            ReadResult::Ok(content) => {
                let parse_output = dialect::parse(&content, contract_rel.clone());
                diagnostics.extend(
                    parse_output
                        .diagnostics
                        .into_iter()
                        .map(|d| d.into_diagnostic(contract_rel.clone())),
                );
                contract = Some(Contract::new(contract_rel.clone(), parse_output.definitions));
            }
            ReadResult::LimitExceeded(len) => {
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
            ReadResult::NotAFile => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::IO003,
                    Severity::Error,
                    format!(
                        "Contract path '{}' exists but is a directory, not a file",
                        contract_rel
                    ),
                    None,
                    vec![Location::new(contract_rel.clone(), Span::point(1, 1, 0))],
                    None,
                ));
                complete = false;
            }
            ReadResult::IoError(err) => {
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
            ReadResult::InvalidUtf8 => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::IO002,
                    Severity::Error,
                    format!("Contract file '{}' contains invalid UTF-8 bytes", contract_rel),
                    None,
                    vec![Location::new(contract_rel.clone(), Span::point(1, 1, 0))],
                    None,
                ));
                complete = false;
            }
        }
    }

    // 2. Process Optional Environment File
    let mut env_definitions = None;
    if let Some(ref env_path) = config.env_path {
        let env_rel = RelPath::new(env_path);
        if !env_path.exists() {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::IO001,
                Severity::Error,
                format!("Specified environment file '{}' was not found", env_rel),
                None,
                vec![Location::new(env_rel.clone(), Span::point(1, 1, 0))],
                None,
            ));
            complete = false;
        } else {
            match read_file_bounded(env_path, config.max_file_size) {
                ReadResult::Ok(content) => {
                    let parse_output = dialect::parse(&content, env_rel.clone());
                    diagnostics.extend(
                        parse_output
                            .diagnostics
                            .into_iter()
                            .map(|d| d.into_diagnostic(env_rel.clone())),
                    );
                    env_definitions = Some(parse_output.definitions);
                }
                ReadResult::LimitExceeded(len) => {
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
                ReadResult::NotAFile => {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::IO003,
                        Severity::Error,
                        format!(
                            "Environment path '{}' exists but is a directory, not a file",
                            env_rel
                        ),
                        None,
                        vec![Location::new(env_rel.clone(), Span::point(1, 1, 0))],
                        None,
                    ));
                    complete = false;
                }
                ReadResult::IoError(err) => {
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
                ReadResult::InvalidUtf8 => {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::IO002,
                        Severity::Error,
                        format!("Environment file '{}' contains invalid UTF-8 bytes", env_rel),
                        None,
                        vec![Location::new(env_rel.clone(), Span::point(1, 1, 0))],
                        None,
                    ));
                    complete = false;
                }
            }
        }
    }

    // 3. File Discovery
    let discovery_options = DiscoveryOptions {
        paths: config.scan_paths.clone(),
        include_ignored: config.include_ignored,
        follow_symlinks: false,
        max_files: config.max_files,
        max_depth: config.max_depth,
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

    // 4. Source Scanning
    let mut all_references = Vec::new();

    for file_path in &discovered.files {
        let rel_path = RelPath::new(file_path);

        // Do not scan the contract file or active .env file as JS/TS source code
        if rel_path == contract_rel {
            continue;
        }
        if let Some(ref env_path) = config.env_path {
            if rel_path == RelPath::new(env_path) {
                continue;
            }
        }

        match read_file_bounded(file_path, config.max_file_size) {
            ReadResult::Ok(content) => {
                let scan_res = scan::scan_source(&content, rel_path);
                if scan_res
                    .diagnostics
                    .iter()
                    .any(|d| d.code == DiagnosticCode::ENV011)
                {
                    complete = false;
                }
                diagnostics.extend(scan_res.diagnostics);
                all_references.extend(scan_res.references);
                scanned_files += 1;
            }
            ReadResult::LimitExceeded(len) => {
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
            ReadResult::NotAFile => {
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
            ReadResult::IoError(err) => {
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
            ReadResult::InvalidUtf8 => {
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
        }
    }

    // 5. Evaluate Contract Rules
    let rule_diagnostics = rules::evaluate_contract(
        contract.as_ref(),
        env_definitions.as_deref(),
        &all_references,
        &config.audit_options,
    );
    diagnostics.extend(rule_diagnostics);

    // 6. Build Final Report (deterministic ordering and summary calculation)
    Report::new(diagnostics, scanned_files, skipped_files, complete)
}
