//! Command-line argument parsing and configuration options.

use crate::diagnostics::severity::Severity;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Output formats supported by the EnvDoctor CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Human-readable terminal output with ANSI colors and advice hints.
    #[default]
    Human,
    /// Machine-readable JSON conforming to schemas/report.v1.schema.json.
    Json,
}

/// Minimum severity threshold that causes CLI to exit with code 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum FailOn {
    /// Fail only on error-severity findings.
    Error,
    /// Fail on warning or error-severity findings.
    #[default]
    Warning,
    /// Fail on note, warning, or error-severity findings.
    Note,
}

impl From<FailOn> for Severity {
    fn from(f: FailOn) -> Self {
        match f {
            FailOn::Error => Severity::Error,
            FailOn::Warning => Severity::Warning,
            FailOn::Note => Severity::Note,
        }
    }
}

/// Terminal color modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum ColorChoice {
    /// Automatically enable colors when stdout is a terminal and NO_COLOR is unset.
    #[default]
    Auto,
    /// Always enable ANSI colors.
    Always,
    /// Never emit ANSI escape codes.
    Never,
}

/// Command-line arguments for EnvDoctor.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "envdoctor",
    about = "Safe-by-construction, AST-based environment contract checker",
    version,
    long_about = None
)]
pub struct Cli {
    /// Path to environment contract file (e.g. .env.example)
    #[arg(short, long, default_value = ".env.example")]
    pub contract: PathBuf,

    /// Optional path to active .env file to audit against the contract
    #[arg(short, long)]
    pub env: Option<PathBuf>,

    /// Target source files or directories to scan (defaults to current directory)
    #[arg(short, long = "path", num_args = 0..)]
    pub paths: Vec<PathBuf>,

    /// Report output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    /// Failure threshold: exits with code 1 if findings meet or exceed this level
    #[arg(long, value_enum, default_value_t = FailOn::Warning)]
    pub fail_on: FailOn,

    /// Disable colored terminal output (alias for --color never)
    #[arg(long)]
    pub no_color: bool,

    /// Color mode for terminal output
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Include files ignored by .gitignore or .ignore
    #[arg(long)]
    pub include_ignored: bool,

    /// Maximum file size to scan in bytes (default: 2 MB)
    #[arg(long, default_value_t = 2 * 1024 * 1024)]
    pub max_file_size: u64,

    /// Maximum number of files to scan (default: 10,000)
    #[arg(long, default_value_t = 10_000)]
    pub max_files: usize,

    /// Maximum directory recursion depth (default: 30)
    #[arg(long, default_value_t = 30)]
    pub max_depth: usize,
}
