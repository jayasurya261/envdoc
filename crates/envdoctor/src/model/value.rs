use serde::{Deserialize, Serialize};
use std::fmt;

/// Classification kind of an environment variable value.
///
/// Invariant: To uphold zero-leakage security, no actual string values
/// are stored in AST or Diagnostic structures. Only this metadata kind is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValueKind {
    /// Variable is defined with an empty string value (e.g., `KEY=` or `KEY=""`).
    Empty,
    /// Variable is defined with a single-line non-empty string value.
    NonEmpty,
    /// Variable is defined with a multi-line value.
    MultiLine,
}

impl ValueKind {
    /// Returns `true` if the value classification is `Empty`.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Returns `true` if the value classification is `MultiLine`.
    #[inline]
    pub const fn is_multiline(&self) -> bool {
        matches!(self, Self::MultiLine)
    }

    /// Returns the kebab-case string representation.
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::NonEmpty => "non-empty",
            Self::MultiLine => "multi-line",
        }
    }
}

impl fmt::Display for ValueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_kind_predicates() {
        assert!(ValueKind::Empty.is_empty());
        assert!(!ValueKind::NonEmpty.is_empty());
        assert!(ValueKind::MultiLine.is_multiline());
    }

    #[test]
    fn test_value_kind_serde() {
        assert_eq!(
            serde_json::to_string(&ValueKind::Empty).unwrap(),
            "\"empty\""
        );
        assert_eq!(
            serde_json::to_string(&ValueKind::NonEmpty).unwrap(),
            "\"non-empty\""
        );
        assert_eq!(
            serde_json::to_string(&ValueKind::MultiLine).unwrap(),
            "\"multi-line\""
        );
    }
}
