//! EnvDoctor CLI entry point.

use clap::Parser;
use envdoctor::cli::{Cli, ColorChoice, OutputFormat};
use envdoctor::engine::{run_engine, EngineConfig};
use envdoctor::render::{render_human, render_json};
use envdoctor::AuditOptions;
use std::io::IsTerminal;
use std::path::PathBuf;

fn main() {
    let cli = Cli::parse();

    let use_color = match (cli.no_color, cli.color) {
        (true, _) => false,
        (false, ColorChoice::Never) => false,
        (false, ColorChoice::Always) => true,
        (false, ColorChoice::Auto) => {
            std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
        }
    };

    let scan_paths = if cli.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        cli.paths
    };

    let config = EngineConfig {
        contract_path: cli.contract,
        env_path: cli.env,
        scan_paths,
        include_ignored: cli.include_ignored,
        max_file_size: cli.max_file_size,
        max_files: cli.max_files,
        max_depth: cli.max_depth,
        audit_options: AuditOptions::default(),
    };

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
    }

    let exit_code = report.exit_code(cli.fail_on.into());
    if exit_code == 3 {
        eprintln!("envdoctor: operational error encountered; scan results may be incomplete.");
    }

    std::process::exit(exit_code);
}
