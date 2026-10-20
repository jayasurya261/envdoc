use crate::diagnostics::severity::Severity;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable, non-reusable diagnostic codes specified by EnvDoctor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DiagnosticCode {
    /// Environment variable accessed in source code but absent from `.env.example`.
    #[serde(rename = "ENV001")]
    ENV001,
    /// Defined in `.env.example` but not referenced anywhere in scanned source.
    #[serde(rename = "ENV002")]
    ENV002,
    /// Defined in `.env.example` but absent in provided `.env` (downgraded for `# @optional`).
    #[serde(rename = "ENV003")]
    ENV003,
    /// Same variable defined multiple times in a single `.env` file.
    #[serde(rename = "ENV004")]
    ENV004,
    /// Syntax error (unrecognized line neither comment nor assignment).
    #[serde(rename = "ENV005")]
    ENV005,
    /// Valid shell construct unsupported by dialect (interpolation `$FOO`, command substitution `$(cmd)`).
    #[serde(rename = "ENV006")]
    ENV006,
    /// Supplied `.env` has an empty value for a variable not marked optional.
    #[serde(rename = "ENV007")]
    ENV007,
    /// Dynamic access pattern whose key cannot be statically resolved.
    #[serde(rename = "ENV010")]
    ENV010,
    /// Source file failed AST parsing; findings for this file may be partial.
    #[serde(rename = "ENV011")]
    ENV011,
    /// Entire environment object is aliased or spread (`const e = process.env`).
    #[serde(rename = "ENV012")]
    ENV012,
    /// Specified contract file (e.g. `.env.example`) was not found (Usage Error).
    #[serde(rename = "ENV020")]
    ENV020,
    /// Specified contract file exists but contains zero declared variables.
    #[serde(rename = "ENV021")]
    ENV021,
    /// Operating system permission or file reading error.
    #[serde(rename = "IO001")]
    IO001,
    /// File is not valid UTF-8; lossy fallback is strictly forbidden.
    #[serde(rename = "IO002")]
    IO002,
    /// Target path exists but is a directory or special device.
    #[serde(rename = "IO003")]
    IO003,
    /// Scan terminated early because size, file count, or depth limit was exceeded.
    #[serde(rename = "IO010")]
    IO010,
}

impl DiagnosticCode {
    /// The canonical 6-character code string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ENV001 => "ENV001",
            Self::ENV002 => "ENV002",
            Self::ENV003 => "ENV003",
            Self::ENV004 => "ENV004",
            Self::ENV005 => "ENV005",
            Self::ENV006 => "ENV006",
            Self::ENV007 => "ENV007",
            Self::ENV010 => "ENV010",
            Self::ENV011 => "ENV011",
            Self::ENV012 => "ENV012",
            Self::ENV020 => "ENV020",
            Self::ENV021 => "ENV021",
            Self::IO001 => "IO001",
            Self::IO002 => "IO002",
            Self::IO003 => "IO003",
            Self::IO010 => "IO010",
        }
    }

    /// The human-readable kebab-case identifier.
    pub const fn identifier(&self) -> &'static str {
        match self {
            Self::ENV001 => "undocumented-reference",
            Self::ENV002 => "unused-definition",
            Self::ENV003 => "missing-in-env",
            Self::ENV004 => "duplicate-key",
            Self::ENV005 => "malformed-line",
            Self::ENV006 => "unsupported-syntax",
            Self::ENV007 => "empty-required-value",
            Self::ENV010 => "unresolved-dynamic-access",
            Self::ENV011 => "source-parse-problem",
            Self::ENV012 => "environment-escapes",
            Self::ENV020 => "contract-missing",
            Self::ENV021 => "contract-empty",
            Self::IO001 => "unreadable-file",
            Self::IO002 => "invalid-utf8",
            Self::IO003 => "not-a-file",
            Self::IO010 => "limit-exceeded",
        }
    }

    /// The default severity as per specification.
    pub const fn default_severity(&self) -> Severity {
        match self {
            Self::ENV001 => Severity::Warning,
            Self::ENV002 => Severity::Note,
            Self::ENV003 => Severity::Error,
            Self::ENV004 => Severity::Warning,
            Self::ENV005 => Severity::Error,
            Self::ENV006 => Severity::Warning,
            Self::ENV007 => Severity::Warning,
            Self::ENV010 => Severity::Note,
            Self::ENV011 => Severity::Warning,
            Self::ENV012 => Severity::Note,
            Self::ENV020 => Severity::Error,
            Self::ENV021 => Severity::Warning,
            Self::IO001 => Severity::Error,
            Self::IO002 => Severity::Error,
            Self::IO003 => Severity::Error,
            Self::IO010 => Severity::Error,
        }
    }

    /// Returns `true` if this code belongs to the operational I/O group.
    #[inline]
    pub const fn is_io(&self) -> bool {
        matches!(self, Self::IO001 | Self::IO002 | Self::IO003 | Self::IO010)
    }

    /// Returns `true` if this is a usage / configuration error code.
    #[inline]
    pub const fn is_usage(&self) -> bool {
        matches!(self, Self::ENV020)
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

