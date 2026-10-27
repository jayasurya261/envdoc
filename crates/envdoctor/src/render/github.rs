use crate::diagnostics::report::Report;
use crate::diagnostics::severity::Severity;

/// Renders findings as GitHub Actions workflow command annotations.
///
/// Syntax: `::warning file={name},line={line},col={col}::{message}`
pub fn render(report: &Report) -> String {
    let mut out = String::new();

    for d in &report.diagnostics {
        let level = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "notice",
        };

        let (file, line, col) = if let Some(loc) = d.locations.first() {
            (loc.path.as_str(), loc.span.line, loc.span.column)
        } else {
            (".env.example", 1, 1)
        };

        out.push_str(&format!(
            "::{} file={},line={},col={}::{}: {}\n",
            level,
            file,
            line,
            col,
            d.code.as_str(),
            d.message
        ));
    }

    out
}
