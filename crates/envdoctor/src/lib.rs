//! # EnvDoctor
//!
//! A read-only, AST-based, safe-by-construction, CI-grade environment contract checker
//! keeping `.env.example` in sync with Rust & TypeScript source code.

#![deny(clippy::disallowed_methods)]

pub const NAME: &str = "envdoctor";

pub mod diagnostics;
pub mod dialect;
pub mod model;
pub mod redaction;
pub mod rules;
pub mod scan;

pub use diagnostics::{Diagnostic, DiagnosticCode, Report, Severity, Summary};
pub use dialect::{parse, DialectDiagnostic, ParseOutput};
pub use model::{
    Definition, EnvName, EvidenceKind, Location, Reference, RelPath, Span, ValueKind,
};
pub use redaction::RawEnvValue;
pub use rules::{evaluate_contract, AuditOptions, Contract};
pub use scan::{scan_source, ScanResult};
