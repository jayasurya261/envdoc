#![allow(clippy::disallowed_methods)]

use crate::diagnostics::diagnostic::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Baseline entry representing a suppressed legacy finding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BaselineEntry {
    pub code: String,
    pub key: Option<String>,
    pub path: String,
}

/// Baseline file schema.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BaselineFile {
    pub schema_version: u32,
    pub suppressions: Vec<BaselineEntry>,
}

impl BaselineFile {
    /// Loads a baseline file from disk.
    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read baseline file '{}': {}", path.display(), e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Invalid JSON in baseline file '{}': {}", path.display(), e))
    }

    /// Saves this baseline file to disk.
    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize baseline: {}", e))?;
        std::fs::write(path, json)
            .map_err(|e| format!("Failed to write baseline file '{}': {}", path.display(), e))
    }

    /// Generates a baseline from a collection of diagnostics.
    pub fn from_diagnostics(diagnostics: &[Diagnostic]) -> Self {
        let mut suppressions = Vec::new();
        let mut seen = HashSet::new();

        for d in diagnostics {
            let primary_path = d
                .locations
                .first()
                .map(|loc| loc.path.as_str().to_string())
                .unwrap_or_default();

            let entry = BaselineEntry {
                code: d.code.as_str().to_string(),
                key: d.key.as_ref().map(|k| k.as_str().to_string()),
                path: primary_path,
            };

            if seen.insert(entry.clone()) {
                suppressions.push(entry);
            }
        }

        Self {
            schema_version: 1,
            suppressions,
        }
    }

    /// Filters diagnostics, removing those recorded in this baseline.
    pub fn filter_diagnostics(&self, diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
        let baseline_set: HashSet<&BaselineEntry> = self.suppressions.iter().collect();

        diagnostics
            .into_iter()
            .filter(|d| {
                let primary_path = d
                    .locations
                    .first()
                    .map(|loc| loc.path.as_str().to_string())
                    .unwrap_or_default();

                let entry = BaselineEntry {
                    code: d.code.as_str().to_string(),
                    key: d.key.as_ref().map(|k| k.as_str().to_string()),
                    path: primary_path,
                };

                !baseline_set.contains(&entry)
            })
            .collect()
    }
}

/// Checks if a source file line contains an inline suppression directive for a diagnostic code:
/// - `// envdoctor-ignore ENV001`
/// - `# envdoctor-ignore ENV001`
/// - `/* envdoctor-ignore ENV001 */`
/// - `envdoctor-ignore-all`
pub fn is_line_suppressed(line_text: &str, code: &str) -> bool {
    let lower = line_text.to_ascii_lowercase();
    if lower.contains("envdoctor-ignore-all") {
        return true;
    }

    if let Some(pos) = lower.find("envdoctor-ignore") {
        let rest = &line_text[pos + "envdoctor-ignore".len()..];
        // Parse comma or space separated codes after directive
        for token in rest.split([',', ' ', '*', '/']) {
            let trimmed = token.trim();
            if trimmed.eq_ignore_ascii_case(code) {
                return true;
            }
        }
    }

    false
}
