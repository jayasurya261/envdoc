use crate::diagnostics::report::Report;
use crate::diagnostics::severity::Severity;

/// Renders report findings in JUnit XML format for CI systems.
pub fn render(report: &Report) -> String {
    let mut xml = String::new();
    let total_tests = report.summary.scanned_files;
    let failures = report.summary.total_errors + report.summary.total_warnings;

    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(&format!(
        "<testsuites name=\"envdoctor\" tests=\"{}\" failures=\"{}\">\n",
        total_tests, failures
    ));
    xml.push_str(&format!(
        "  <testsuite name=\"environment-contract\" tests=\"{}\" failures=\"{}\">\n",
        total_tests, failures
    ));

    for d in &report.diagnostics {
        let test_name = format!(
            "{}: {}",
            d.code.as_str(),
            d.key.as_deref().unwrap_or("general")
        );
        let file_path = d
            .locations
            .first()
            .map(|l| l.path.as_str())
            .unwrap_or("contract");

        xml.push_str(&format!(
            "    <testcase name=\"{}\" classname=\"{}\">\n",
            escape_xml(&test_name),
            escape_xml(file_path)
        ));

        if d.severity == Severity::Error || d.severity == Severity::Warning {
            let failure_type = if d.severity == Severity::Error {
                "Error"
            } else {
                "Warning"
            };
            xml.push_str(&format!(
                "      <failure type=\"{}\" message=\"{}\">{}</failure>\n",
                failure_type,
                escape_xml(&d.message),
                escape_xml(&d.message)
            ));
        }

        xml.push_str("    </testcase>\n");
    }

    if report.diagnostics.is_empty() {
        xml.push_str("    <testcase name=\"contract-audit\" classname=\"envdoctor\" />\n");
    }

    xml.push_str("  </testsuite>\n");
    xml.push_str("</testsuites>\n");

    xml
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
