use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::severity::Severity;
use crate::diagnostics::summary::Summary;
use serde::{Deserialize, Serialize};

/// Versioned root report model matching `schemas/report.v1.schema.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    /// Schema format version, strictly version 1 for v1.
    pub schema_version: u32,
    /// Product name identifier (e.g. `envdoctor`).
    pub tool_name: String,
    /// Version of the tool emitting the report.
    pub tool_version: String,
    /// High-level scan execution summary.
    pub summary: Summary,
    /// List of discovered findings, deterministically ordered.
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    /// Current JSON schema version.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Constructs a new `Report` from a list of diagnostics and file counts.
    /// Diagnostics are deterministically sorted automatically.
    pub fn new(
        mut diagnostics: Vec<Diagnostic>,
        scanned_files: usize,
        skipped_files: usize,
        complete: bool,
    ) -> Self {
        Diagnostic::sort_diagnostics(&mut diagnostics);
        let summary =
            Summary::from_diagnostics(&diagnostics, scanned_files, skipped_files, complete);

        Self {
            schema_version: Self::SCHEMA_VERSION,
            tool_name: crate::NAME.to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            summary,
            diagnostics,
        }
    }

    /// Determines the process exit code according to the EnvDoctor specification:
    /// - `0`: Success (no diagnostic at or above `fail_on` threshold).
    /// - `1`: Contract violation detected (one or more diagnostics at or above threshold).
    /// - `2`: Usage / CLI configuration error (e.g. missing contract file `ENV020`).
    /// - `3`: Operational error leading to incomplete results (`IO001`–`IO003`, `IO010`, or `complete == false`).
    pub fn exit_code(&self, fail_on: Severity) -> i32 {
        // Operational failure has highest priority
        if !self.summary.complete
            || self
                .diagnostics
                .iter()
                .any(|d| d.code.is_io() || d.code == DiagnosticCode::IO010)
        {
            return 3;
        }

        // Usage / CLI configuration errors (ENV020)
        if self.diagnostics.iter().any(|d| d.code.is_usage()) {
            return 2;
        }

        // Check if any diagnostic meets or exceeds the fail_on threshold
        let has_failures = self.diagnostics.iter().any(|d| d.severity.is_at_least(fail_on));
        if has_failures {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::span::{Location, Span};

    #[test]
    fn test_exit_codes() {
        let clean_report = Report::new(vec![], 5, 0, true);
        assert_eq!(clean_report.exit_code(Severity::Warning), 0);

        let warn_diag = Diagnostic::new(
            DiagnosticCode::ENV001,
            Severity::Warning,
            "Undocumented",
            None,
            vec![],
            None,
        );
        let warn_report = Report::new(vec![warn_diag], 5, 0, true);
        assert_eq!(warn_report.exit_code(Severity::Warning), 1);
        assert_eq!(warn_report.exit_code(Severity::Error), 0);

        let usage_diag = Diagnostic::new(
            DiagnosticCode::ENV020,
            Severity::Error,
            "Contract missing",
            None,
            vec![],
            None,
        );
        let usage_report = Report::new(vec![usage_diag], 0, 0, true);
        assert_eq!(usage_report.exit_code(Severity::Warning), 2);

        let io_diag = Diagnostic::new(
            DiagnosticCode::IO001,
            Severity::Error,
            "Unreadable file",
            None,
            vec![Location::new("unreadable.ts", Span::point(1, 1, 0))],
            None,
        );
        let io_report = Report::new(vec![io_diag], 4, 1, false);
        assert_eq!(io_report.exit_code(Severity::Warning), 3);
    }
}
