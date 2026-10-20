use serde::{Deserialize, Serialize};
use std::fmt;

/// The syntactic access pattern discovered in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    /// Direct member access, e.g. `process.env.FOO` or `import.meta.env.FOO`.
    DirectMember,
    /// Bracket access with string literal, e.g. `process.env['FOO']` or `process.env["FOO"]`.
    BracketLiteral,
    /// Destructured property, e.g. `const { FOO } = process.env`.
    Destructured,
    /// Unresolved dynamic bracket access, e.g. `process.env[someVar]`.
    DynamicUnresolved,
    /// Whole environment object aliasing or spread, e.g. `const myEnv = process.env`.
    WholeObjectEscape,
}

impl EvidenceKind {
    /// Returns the kebab-case identifier for this evidence kind.
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::DirectMember => "direct-member",
            Self::BracketLiteral => "bracket-literal",
            Self::Destructured => "destructured",
            Self::DynamicUnresolved => "dynamic-unresolved",
            Self::WholeObjectEscape => "whole-object-escape",
        }
    }
}

impl fmt::Display for EvidenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evidence_kind_as_str() {
        assert_eq!(EvidenceKind::DirectMember.as_str(), "direct-member");
        assert_eq!(EvidenceKind::BracketLiteral.as_str(), "bracket-literal");
        assert_eq!(EvidenceKind::Destructured.as_str(), "destructured");
        assert_eq!(EvidenceKind::DynamicUnresolved.as_str(), "dynamic-unresolved");
        assert_eq!(EvidenceKind::WholeObjectEscape.as_str(), "whole-object-escape");
    }

    #[test]
    fn test_evidence_kind_serde() {
        let serialized = serde_json::to_string(&EvidenceKind::DirectMember).unwrap();
        assert_eq!(serialized, "\"direct-member\"");
        let deserialized: EvidenceKind = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, EvidenceKind::DirectMember);
    }
}

