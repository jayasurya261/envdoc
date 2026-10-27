//! # EnvDoctor
//!
//! A read-only, AST-based, safe-by-construction, CI-grade environment contract checker
//! keeping `.env.example` in sync with Rust & TypeScript source code.

#![forbid(unsafe_code)]
#![deny(clippy::disallowed_methods)]

pub const NAME: &str = "envdoctor";

pub mod cache;
pub mod cli;
pub mod config;
pub mod diagnostics;
pub mod dialect;
pub mod discover;
pub mod engine;
pub mod framework;
pub mod model;
pub mod redaction;
pub mod render;
pub mod rules;
pub mod scan;
pub mod suppression;

pub use cache::ScanCache;
pub use cli::{Cli, ColorChoice, FailOn, OutputFormat};
pub use config::Config;
pub use diagnostics::{Diagnostic, DiagnosticCode, Report, Severity, Summary};
pub use dialect::{parse, DialectDiagnostic, ParseOutput};
pub use discover::{discover_files, DiscoveredFiles, DiscoveryOptions};
pub use engine::{run_engine, EngineConfig};
pub use framework::FrameworkPreset;
pub use model::{
    ContractAnnotations, ContractType, Definition, EnvName, EvidenceKind, Location, Reference,
    RelPath, Span, ValueKind,
};
pub use redaction::RawEnvValue;
pub use render::{render_github, render_human, render_json, render_junit, render_sarif};
pub use rules::{evaluate_contract, AuditOptions, Contract};
pub use scan::{scan_source, ScanResult, Scanner};
pub use suppression::{BaselineEntry, BaselineFile};
