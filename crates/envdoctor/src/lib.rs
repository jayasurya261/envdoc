//! # EnvDoctor
//!
//! A read-only, AST-based, safe-by-construction, CI-grade environment contract checker
//! keeping `.env.example` in sync with Rust & TypeScript source code.

#![deny(clippy::disallowed_methods)]

pub const NAME: &str = "envdoctor";

pub mod cli;
pub mod diagnostics;
pub mod dialect;
pub mod discover;
pub mod engine;
pub mod model;
pub mod redaction;
pub mod render;
pub mod rules;
pub mod scan;

pub use cli::{Cli, ColorChoice, FailOn, OutputFormat};
pub use diagnostics::{Diagnostic, DiagnosticCode, Report, Severity, Summary};
pub use dialect::{parse, DialectDiagnostic, ParseOutput};
pub use discover::{discover_files, DiscoveredFiles, DiscoveryOptions};
pub use engine::{run_engine, EngineConfig};
pub use model::{
    Definition, EnvName, EvidenceKind, Location, Reference, RelPath, Span, ValueKind,
};
pub use redaction::RawEnvValue;
pub use render::{render_human, render_json};
pub use rules::{evaluate_contract, AuditOptions, Contract};
pub use scan::{scan_source, ScanResult};
