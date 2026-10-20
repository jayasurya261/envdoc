use crate::diagnostics::diagnostic::Diagnostic;
pub use crate::diagnostics::{DiagnosticCode, Severity};
use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::span::{Location, Span};
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

    /// Converts a dialect-specific diagnostic into a standard report `Diagnostic`.
    pub fn into_diagnostic(self, path: RelPath) -> Diagnostic {
        Diagnostic::new(
            self.code,
            self.severity,
            self.message,
            self.key,
            vec![Location::new(path, self.span)],
            None,
        )
    }
}
