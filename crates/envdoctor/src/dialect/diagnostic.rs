pub use crate::diagnostics::{DiagnosticCode, Severity};
use crate::model::name::EnvName;
use crate::model::span::Span;
use serde::{Deserialize, Serialize};

/// A diagnostic emitted during dialect parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialectDiagnostic {
    /// Diagnostic code (e.g. ENV004, ENV005, ENV006).
    pub code: DiagnosticCode,
    /// Severity level.
    pub severity: Severity,
    /// Human-readable explanation.
    pub message: String,
    /// Source span of the offending syntax.
    pub span: Span,
    /// Associated variable name, if identified.
    pub key: Option<EnvName>,
}

impl DialectDiagnostic {
    /// Constructs a new dialect diagnostic.
    pub fn new(
        code: DiagnosticCode,
        message: impl Into<String>,
        span: Span,
        key: Option<EnvName>,
    ) -> Self {
        Self {
            code,
            severity: code.default_severity(),
            message: message.into(),
            span,
            key,
        }
    }
}
