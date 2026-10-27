//! EnvDoctor CLI entry point.

#![forbid(unsafe_code)]

use clap::Parser;
use envdoctor::cli::{Cli, ColorChoice, DuplicateKeyArg, OutputFormat};
use envdoctor::config::Config;
use envdoctor::dialect::{DialectOptions, DuplicateKeyStrategy};
use envdoctor::engine::{run_engine, EngineConfig};
use envdoctor::framework::FrameworkPreset;
use envdoctor::render::{render_github, render_human, render_json, render_junit, render_sarif};
use envdoctor::AuditOptions;
use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::PathBuf;

fn main() {
    #[cfg(unix)]
    {
        // Disable core dumps on Unix systems to prevent secret retention in core dump files
        let _ = rlimit::Resource::CORE.set(0, 0);
    }

    let cli = Cli::parse();

    // 1. Load configuration from file (envdoctor.toml) if present
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let file_config = if let Some(ref cfg_path) = cli.config {
        match Config::load_from_path(cfg_path) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("envdoctor: {}", e);
                std::process::exit(3);
            }
        }
    } else {
        Config::find_and_load(&current_dir)
    };

    let use_color = match (cli.no_color, cli.color) {
        (true, _) => false,
        (false, ColorChoice::Never) => false,
        (false, ColorChoice::Always) => true,
        (false, ColorChoice::Auto) => {
            std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
        }
    };

    // 2. Resolve scan paths
    let scan_paths = if !cli.paths.is_empty() {
        cli.paths
    } else if let Some(ref fc) = file_config {
        if !fc.workspaces.paths.is_empty() {
            fc.workspaces.paths.clone()
        } else {
            vec![PathBuf::from(".")]
        }
    } else {
        vec![PathBuf::from(".")]
    };

    // 3. Resolve contract & env paths
    let contract_path = if cli.contract != std::path::Path::new(".env.example") {
        cli.contract
    } else if let Some(ref fc) = file_config {
        fc.audit.contract.clone().unwrap_or(cli.contract)
    } else {
        cli.contract
    };

    let env_paths = if !cli.env.is_empty() {
        cli.env
    } else if let Some(ref fc) = file_config {
        fc.audit.env.clone().unwrap_or_default()
    } else {
        Vec::new()
    };

    // 4. Resolve Framework Preset
    let preset = if let Some(ref p_str) = cli.preset {
        FrameworkPreset::parse(p_str).unwrap_or(FrameworkPreset::Auto)
    } else if let Some(ref fc) = file_config {
        if let Some(ref p_str) = fc.framework.preset {
            FrameworkPreset::parse(p_str).unwrap_or(FrameworkPreset::Auto)
        } else {
            FrameworkPreset::detect(&current_dir)
        }
    } else {
        FrameworkPreset::detect(&current_dir)
    };

    // 5. Construct AuditOptions
    let mut audit_options = AuditOptions::default();
    if cli.no_default_ignores
        || file_config
            .as_ref()
            .map(|fc| fc.audit.no_default_ignores)
            .unwrap_or(false)
    {
        audit_options.ignore_platform_vars = false;
        audit_options.ignore_prefixes.clear();
    }

    if cli.case_insensitive
        || file_config
            .as_ref()
            .and_then(|fc| fc.audit.case_insensitive)
            .unwrap_or(false)
    {
        audit_options.case_insensitive = true;
    }

    // Add explicit ignore keys
    let mut ignore_keys = HashSet::new();
    for k in cli.ignore_keys {
        ignore_keys.insert(k);
    }
    if let Some(ref fc) = file_config {
        for k in &fc.audit.ignore_keys {
            ignore_keys.insert(k.clone());
        }
    }
    // Add framework platform ignores
    for pv in preset.platform_ignores() {
        ignore_keys.insert((*pv).to_string());
    }
    audit_options.ignore_keys = ignore_keys;

    // Add framework client prefixes
    for prefix in preset.client_prefixes() {
        audit_options.ignore_prefixes.push((*prefix).to_string());
    }

    // 6. Construct DialectOptions
    let duplicate_strategy = match cli.duplicate_keys {
        DuplicateKeyArg::Last => DuplicateKeyStrategy::LastWins,
        DuplicateKeyArg::First => DuplicateKeyStrategy::FirstWins,
        DuplicateKeyArg::Error => DuplicateKeyStrategy::Error,
    };
    let dialect_options = DialectOptions { duplicate_strategy };

    // 7. Resolve enabled languages (opt-in for new languages to avoid breaking existing CI)
    let languages = if cli.languages != vec!["default".to_string()] {
        cli.languages
    } else if let Some(ref fc) = file_config {
        fc.audit.languages.clone().unwrap_or(cli.languages)
    } else {
        cli.languages
    };

    let config = EngineConfig {
        contract_path,
        env_path: None,
        env_paths,
        scan_paths,
        include_ignored: cli.include_ignored,
        max_file_size: cli.max_file_size,
        max_files: cli.max_files,
        max_depth: cli.max_depth,
        max_line_length: 64 * 1024,
        max_total_bytes: 100 * 1024 * 1024,
        max_diagnostics: 2_000,
        audit_options,
        dialect_options,
        fix: cli.fix,
        baseline: cli.baseline,
        generate_baseline: cli.generate_baseline,
        use_cache: !cli.no_cache,
        cache_path: cli.cache_file,
        languages,
    };

    if cli.watch {
        run_watch_mode(&config, cli.format, cli.fail_on, use_color);
        return;
    }

    let report = run_engine(&config);

    // Stream separation: Pure report to stdout, operational logs to stderr
    match cli.format {
        OutputFormat::Json => match render_json(&report) {
            Ok(json) => println!("{}", json),
            Err(err) => {
                eprintln!("envdoctor: error serializing JSON report: {}", err);
                std::process::exit(3);
            }
        },
        OutputFormat::Human => {
            if let Err(err) = render_human(&report, &mut std::io::stdout(), use_color) {
                eprintln!("envdoctor: error writing output: {}", err);
                std::process::exit(3);
            }
        }
        OutputFormat::Sarif => {
            println!("{}", render_sarif(&report));
        }
        OutputFormat::Junit => {
            print!("{}", render_junit(&report));
        }
        OutputFormat::Github => {
            print!("{}", render_github(&report));
        }
    }

    let exit_code = report.exit_code(cli.fail_on.into());
    if exit_code == 3 {
        eprintln!("envdoctor: operational error encountered; scan results may be incomplete.");
    }

    std::process::exit(exit_code);
}

/// Continuous watch mode looping and re-running incremental audits upon file modifications.
fn run_watch_mode(
    config: &EngineConfig,
    format: OutputFormat,
    _fail_on: envdoctor::cli::FailOn,
    use_color: bool,
) {
    use std::io::Write;
    use std::thread::sleep;
    use std::time::Duration;

    println!("envdoctor: watch mode started. Monitoring workspace files for changes...");

    let mut last_run = std::time::Instant::now();
    let mut initial = true;

    loop {
        if initial || last_run.elapsed() >= Duration::from_millis(500) {
            if !initial {
                // ANSI escape to clear terminal and move cursor to top-left
                print!("\x1B[2J\x1B[1;1H");
                let _ = std::io::stdout().flush();
            }
            initial = false;

            let report = run_engine(config);
            match format {
                OutputFormat::Json => {
                    if let Ok(json) = render_json(&report) {
                        println!("{}", json);
                    }
                }
                OutputFormat::Human => {
                    let _ = render_human(&report, &mut std::io::stdout(), use_color);
                }
                OutputFormat::Sarif => println!("{}", render_sarif(&report)),
                OutputFormat::Junit => print!("{}", render_junit(&report)),
                OutputFormat::Github => print!("{}", render_github(&report)),
            }

            println!(
                "\n[watch] Completed incremental audit at {:?}. Waiting for changes...",
                std::time::SystemTime::now()
            );
            last_run = std::time::Instant::now();
        }
        sleep(Duration::from_millis(1000));
    }
}
