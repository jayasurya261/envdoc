//! Strict JSON report serialization conforming to `schemas/report.v1.schema.json`.

use crate::diagnostics::report::Report;

/// Renders the report as pretty-printed JSON conforming to schema version 1.
pub fn render(report: &Report) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_render_validity() {
        let report = Report::new(vec![], 5, 1, true);
        let rendered = render(&report).expect("JSON rendering should succeed");
        assert!(rendered.contains("\"schema_version\": 1"));
        assert!(rendered.contains("\"scanned_files\": 5"));
        assert!(rendered.contains("\"skipped_files\": 1"));
        assert!(rendered.contains("\"complete\": true"));
    }
}
