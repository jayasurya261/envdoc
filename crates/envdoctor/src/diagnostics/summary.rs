use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::severity::Severity;
use serde::{Deserialize, Serialize};

/// High-level scan execution summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// Number of files scanned.
    pub scanned_files: usize,
    /// Number of files skipped (e.g. ignored, oversized, unreadable).
    pub skipped_files: usize,
    /// Total number of error-severity diagnostics.
    pub total_errors: usize,
    /// Total number of warning-severity diagnostics.
    pub total_warnings: usize,
    /// Total number of note-severity diagnostics.
    pub total_notes: usize,
    /// Whether the scan ran to completion without early termination or operational limits.
    pub complete: bool,
}

impl Summary {
    /// Calculates summary totals from a set of diagnostics.
    pub fn from_diagnostics(
        diagnostics: &[Diagnostic],
        scanned_files: usize,
        skipped_files: usize,
        complete: bool,
    ) -> Self {
        let mut total_errors = 0;
        let mut total_warnings = 0;
        let mut total_notes = 0;

        for diag in diagnostics {
            match diag.severity {
                Severity::Error => total_errors += 1,
                Severity::Warning => total_warnings += 1,
                Severity::Note => total_notes += 1,
            }
        }

        Self {
            scanned_files,
            skipped_files,
            total_errors,
            total_warnings,
            total_notes,
            complete,
        }
    }
}

