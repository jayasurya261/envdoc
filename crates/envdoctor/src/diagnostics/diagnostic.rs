use crate::dialect::diagnostic::DialectDiagnostic;
use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::severity::Severity;
use crate::model::evidence::EvidenceKind;
use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::span::Location;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// A structured finding produced during environment contract checking.
///
/// Invariants:
/// - Contains NO secret string values (zero leakage guarantee).
/// - Sortable by `(path, line, column, code, name)` for deterministic reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Diagnostic code (e.g. `ENV001`, `ENV003`).
    pub code: DiagnosticCode,
    /// Effective severity of the finding.
    pub severity: Severity,
    /// Human-readable description.
    pub message: String,
    /// The associated environment variable name, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<EnvName>,
    /// Source file locations relevant to this finding.
    pub locations: Vec<Location>,
    /// Syntactic evidence kind for source references.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceKind>,
}

impl Diagnostic {
    /// Constructs a new `Diagnostic`.
    pub fn new(
        code: DiagnosticCode,
        severity: Severity,
        message: impl Into<String>,
        key: Option<EnvName>,
        locations: Vec<Location>,
        evidence: Option<EvidenceKind>,
    ) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            key,
            locations,
            evidence,
        }
    }

    /// Converts a parser-level `DialectDiagnostic` into a unified `Diagnostic`.
    pub fn from_dialect(diag: DialectDiagnostic, path: &RelPath) -> Self {
        Self {
            code: diag.code,
            severity: diag.severity,
            message: diag.message,
            key: diag.key,
            locations: vec![Location::new(path.clone(), diag.span)],
            evidence: None,
        }
    }

    /// Returns the primary location (the first location in the list, if present).
    #[inline]
    pub fn primary_location(&self) -> Option<&Location> {
        self.locations.first()
    }

    /// Generates the deterministic sort key tuple: `(path, line, column, code, name)`.
    pub fn sort_key(&self) -> (&str, u32, u32, &'static str, &str) {
        let (path, line, col) = if let Some(loc) = self.primary_location() {
            (loc.path.as_str(), loc.span.line, loc.span.column)
        } else {
            ("", 0, 0)
        };
        let code_str = self.code.as_str();
        let key_str = self.key.as_ref().map(|k| k.as_str()).unwrap_or("");
        (path, line, col, code_str, key_str)
    }

    /// Sorts a slice of diagnostics deterministically by `(path, line, column, code, name)`.
    pub fn sort_diagnostics(diagnostics: &mut [Self]) {
        diagnostics.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    }
}

impl PartialOrd for Diagnostic {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Diagnostic {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::span::Span;

    #[test]
    fn test_diagnostic_sorting_order() {
        let d1 = Diagnostic::new(
            DiagnosticCode::ENV001,
            Severity::Warning,
            "Undocumented reference",
            Some(EnvName::new("PORT")),
            vec![Location::new("src/server.ts", Span::new(10, 5, 100, 104))],
            Some(EvidenceKind::DirectMember),
        );

        let d2 = Diagnostic::new(
            DiagnosticCode::ENV001,
            Severity::Warning,
            "Undocumented reference",
            Some(EnvName::new("DATABASE_URL")),
            vec![Location::new("src/server.ts", Span::new(5, 1, 40, 52))],
            Some(EvidenceKind::Destructured),
        );

        let d3 = Diagnostic::new(
            DiagnosticCode::ENV002,
            Severity::Note,
            "Unused definition",
            Some(EnvName::new("API_KEY")),
            vec![Location::new(".env.example", Span::new(1, 1, 0, 7))],
            None,
        );

        let mut list = vec![d1.clone(), d2.clone(), d3.clone()];
        Diagnostic::sort_diagnostics(&mut list);

        // Sorting priority: .env.example (line 1) -> src/server.ts (line 5) -> src/server.ts (line 10)
        assert_eq!(list[0].locations[0].path.as_str(), ".env.example");
        assert_eq!(list[1].locations[0].span.line, 5);
        assert_eq!(list[2].locations[0].span.line, 10);
    }
}
