//! # EnvDoctor
//!
//! A read-only, AST-based, safe-by-construction, CI-grade environment contract checker
//! keeping `.env.example` in sync with Rust & TypeScript source code.

#![deny(clippy::disallowed_methods)]

pub const NAME: &str = "envdoctor";

pub mod model;
pub mod redaction;

pub use model::{
    Definition, EnvName, EvidenceKind, Location, Reference, RelPath, Span, ValueKind,
};
pub use redaction::RawEnvValue;

