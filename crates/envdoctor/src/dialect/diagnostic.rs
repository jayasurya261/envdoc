use crate::model::name::EnvName;
use crate::model::span::Span;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Severity levels for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Note,
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Note => write!(f, "note"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Diagnostic codes emitted by the dialect parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DiagnosticCode {
    /// Duplicate environment variable defined in the same file.
    #[serde(rename = "ENV004")]
    ENV004,
    /// Malformed line or syntax error.
    #[serde(rename = "ENV005")]
    ENV005,
    /// Unsupported shell construct (variable expansion `$FOO` or command substitution `$(cmd)`).
    #[serde(rename = "ENV006")]
    ENV006,
}

impl DiagnosticCode {
    /// Standard code string (e.g. `"ENV004"`).
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ENV004 => "ENV004",
            Self::ENV005 => "ENV005",
            Self::ENV006 => "ENV006",
        }
    }

    /// Kebab-case identifier for this code.
    pub const fn identifier(&self) -> &'static str {
        match self {
            Self::ENV004 => "duplicate-key",
            Self::ENV005 => "malformed-line",
            Self::ENV006 => "unsupported-syntax",
        }
    }

    /// Default severity for this diagnostic code.
    pub const fn default_severity(&self) -> Severity {
        match self {
            Self::ENV004 => Severity::Warning,
            Self::ENV005 => Severity::Error,
            Self::ENV006 => Severity::Warning,
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

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
