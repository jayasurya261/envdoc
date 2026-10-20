use serde::{Deserialize, Serialize};
use std::fmt;

/// Diagnostic severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational note; does not fail CI by default.
    Note = 0,
    /// Warning; indicates potential drift or unsupported syntax.
    Warning = 1,
    /// Error; contract violation or fatal syntax issue.
    Error = 2,
}

impl Severity {
    /// Returns `true` if this severity is at or above the given threshold.
    #[inline]
    pub fn is_at_least(&self, threshold: Severity) -> bool {
        *self as u8 >= threshold as u8
    }

    /// String identifier for this severity.
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Note);
        assert!(Severity::Warning.is_at_least(Severity::Warning));
        assert!(Severity::Error.is_at_least(Severity::Warning));
        assert!(!Severity::Note.is_at_least(Severity::Warning));
    }
}
