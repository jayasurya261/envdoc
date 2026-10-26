use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::span::{Location, Span};
use crate::model::value::ValueKind;
use serde::{Deserialize, Serialize};

/// Represents an environment variable definition extracted from a contract
/// file (`.env.example`) or environment file (`.env`).
///
/// Invariant: To uphold zero-leakage security, no actual string values
/// are stored in this struct. Only `ValueKind` is retained.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Definition {
    /// The name of the environment variable.
    pub name: EnvName,
    /// The file location where this variable is declared.
    pub location: Location,
    /// The classified kind of the value (Empty, NonEmpty, MultiLine).
    pub value_kind: ValueKind,
    /// Whether the definition was marked as optional (e.g. via `# @optional`).
    pub is_optional: bool,
}

impl Definition {
    /// Constructs a new `Definition`.
    pub fn new(
        name: impl Into<EnvName>,
        path: impl Into<RelPath>,
        span: Span,
        value_kind: ValueKind,
        is_optional: bool,
    ) -> Self {
        Self {
            name: name.into(),
            location: Location::new(path.into(), span),
            value_kind,
            is_optional,
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
    fn test_definition_construction() {
        let def = Definition::new(
            "PORT",
            ".env.example",
            Span::new(1, 1, 0, 4),
            ValueKind::NonEmpty,
            false,
        );

        assert_eq!(def.name.as_str(), "PORT");
        assert_eq!(def.path().as_str(), ".env.example");
        assert_eq!(def.span().line, 1);
        assert_eq!(def.value_kind, ValueKind::NonEmpty);
        assert!(!def.is_optional);
    }
}
