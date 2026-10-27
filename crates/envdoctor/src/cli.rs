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
    /// SARIF 2.1.0 JSON format for GitHub Code Scanning tab.
    Sarif,
    /// JUnit XML format for CI test runners.
    Junit,
    /// GitHub Actions workflow annotations format (::warning/::error).
    Github,
}

/// Strategy for handling duplicate keys in environment files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum DuplicateKeyArg {
    #[default]
    Last,
    First,
    Error,
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

    /// Optional paths to active .env files to audit against the contract (ordered by precedence)
    #[arg(short, long, num_args = 0..)]
    pub env: Vec<PathBuf>,

    /// Target source files or directories to scan (defaults to current directory)
    #[arg(short, long = "path", num_args = 0..)]
    pub paths: Vec<PathBuf>,

    /// Report output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    /// Failure threshold: exits with code 1 if findings meet or exceed this level
    #[arg(long, value_enum, default_value_t = FailOn::Warning)]
    pub fail_on: FailOn,

    /// Strategy for handling duplicate keys in .env files (last, first, error)
    #[arg(long, value_enum, default_value_t = DuplicateKeyArg::Last)]
    pub duplicate_keys: DuplicateKeyArg,

    /// Environment variable keys to ignore during audit
    #[arg(long = "ignore-key", num_args = 0..)]
    pub ignore_keys: Vec<String>,

    /// Disable built-in default ignore lists for platform variables (PATH, NODE_ENV, etc.)
    #[arg(long)]
    pub no_default_ignores: bool,

    /// Force case-insensitive matching for environment variable names
    #[arg(long)]
    pub case_insensitive: bool,

    /// Automatically generate missing contract entries with empty values (zero secret leakage)
    #[arg(long)]
    pub fix: bool,

    /// Path to baseline file (.envdoctor-baseline.json) to filter out known legacy findings
    #[arg(long)]
    pub baseline: Option<PathBuf>,

    /// Generate a baseline file from current findings and save to path
    #[arg(long)]
    pub generate_baseline: Option<PathBuf>,

    /// Explicit path to configuration file (envdoctor.toml)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Framework preset to apply (nextjs, vite, nuxt, astro, rails, django, auto, none)
    #[arg(long)]
    pub preset: Option<String>,

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

    /// Watch workspace files for changes and re-run audit incrementally
    #[arg(short = 'w', long)]
    pub watch: bool,

    /// Disable incremental result caching (.envdoctor-cache)
    #[arg(long)]
    pub no_cache: bool,

    /// Explicit path to cache file (defaults to .envdoctor-cache)
    #[arg(long)]
    pub cache_file: Option<PathBuf>,

    /// Comma-separated languages to scan ('default' for js/ts/rust, 'all', or specific: 'js,rust,python,go,infra')
    #[arg(
        short = 'l',
        long = "languages",
        value_delimiter = ',',
        default_values_t = vec!["default".to_string()]
    )]
    pub languages: Vec<String>,
}
