//! Pure in-memory .env dialect lexer and parser.
//!
//! Follows standard dotenv syntax, extracts definitions, classifies
//! values with zero secret leakage, and emits structured diagnostics.

pub mod diagnostic;
pub mod parser;

pub use diagnostic::{DiagnosticCode, DialectDiagnostic, Severity};
pub use parser::{parse, parse_with_options, DialectOptions, DuplicateKeyStrategy, ParseOutput};
