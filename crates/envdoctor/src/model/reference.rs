use crate::model::evidence::EvidenceKind;
use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::span::{Location, Span};
use serde::{Deserialize, Serialize};

/// Represents an environment variable reference discovered in source code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Reference {
    /// The name of the environment variable referenced.
    pub name: EnvName,
    /// The source file location where the reference occurs.
    pub location: Location,
    /// The syntactic pattern used to access the variable.
    pub evidence_kind: EvidenceKind,
}

impl Reference {
    /// Constructs a new `Reference`.
    pub fn new(
        name: impl Into<EnvName>,
        path: impl Into<RelPath>,
        span: Span,
        evidence_kind: EvidenceKind,
    ) -> Self {
        Self {
            name: name.into(),
            location: Location::new(path.into(), span),
            evidence_kind,
        }
    }

    /// Convenience accessor for the relative path.
    #[inline]
    pub fn path(&self) -> &RelPath {
        &self.location.path
    }

    /// Convenience accessor for the span.
    #[inline]
    pub const fn span(&self) -> Span {
        self.location.span
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reference_construction() {
        let reference = Reference::new(
            "API_KEY",
            "src/client.ts",
            Span::new(12, 10, 310, 317),
            EvidenceKind::DirectMember,
        );

        assert_eq!(reference.name.as_str(), "API_KEY");
        assert_eq!(reference.path().as_str(), "src/client.ts");
        assert_eq!(reference.span().line, 12);
        assert_eq!(reference.evidence_kind, EvidenceKind::DirectMember);
    }
}
