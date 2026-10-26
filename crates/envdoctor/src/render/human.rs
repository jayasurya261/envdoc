//! Human-readable terminal output with ANSI colors and advice hints.

use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::report::Report;
use crate::diagnostics::severity::Severity;
use std::io::{self, Write};

/// Terminal color codes for ANSI rendering.
struct Colors {
    error: &'static str,
    warning: &'static str,
    note: &'static str,
    bold: &'static str,
    dim: &'static str,
    green: &'static str,
    reset: &'static str,
}

impl Colors {
    const ENABLED: Self = Self {
        error: "\x1b[1;31m",
        warning: "\x1b[1;33m",
        note: "\x1b[1;36m",
        bold: "\x1b[1m",
        dim: "\x1b[2m",
        green: "\x1b[1;32m",
        reset: "\x1b[0m",
    };

    const DISABLED: Self = Self {
        error: "",
        warning: "",
        note: "",
        bold: "",
        dim: "",
        green: "",
        reset: "",
    };

    fn new(use_color: bool) -> Self {
        if use_color {
            Self::ENABLED
        } else {
            Self::DISABLED
        }
    }
}

/// Generates an actionable advice hint for a diagnostic.
pub fn advice_for_diagnostic(diag: &Diagnostic) -> Option<String> {
    let name = diag.key.as_ref().map(|n| n.as_str()).unwrap_or("");
    match diag.code {
        DiagnosticCode::ENV001 => Some(format!(
            "Declare '{}' in contract (e.g. .env.example) or verify code spelling",
            name
        )),
        DiagnosticCode::ENV002 => Some(format!(
            "Remove unused '{}' from contract or verify if referenced code was scanned",
            name
        )),
        DiagnosticCode::ENV003 => Some(format!(
            "Define '{}' in your active .env file or annotate with '# @optional' in contract",
            name
        )),
        DiagnosticCode::ENV004 => Some(format!(
            "Remove duplicate assignment for '{}'; only one definition is allowed",
            name
        )),
        DiagnosticCode::ENV005 => Some(
            "Fix syntax error: line must be a comment, empty line, or KEY=VALUE assignment".into(),
        ),
        DiagnosticCode::ENV006 => Some(
            "Use literal string values; variable interpolation and subshells are unsupported".into(),
        ),
        DiagnosticCode::ENV007 => Some(format!(
            "Provide a non-empty value for '{}' or mark optional with '# @optional' in contract",
            name
        )),
        DiagnosticCode::ENV010 => Some(
            "Replace dynamic bracket access with static member or literal bracket access".into(),
        ),
        DiagnosticCode::ENV011 => Some(
            "Fix JavaScript / TypeScript syntax error so the file can be fully analyzed".into(),
        ),
        DiagnosticCode::ENV012 => Some(
            "Access specific properties on process.env instead of whole-object spreading or aliasing".into(),
        ),
        DiagnosticCode::ENV020 => Some(
            "Create the contract file (e.g. .env.example) or pass path via --contract <PATH>".into(),
        ),
        DiagnosticCode::ENV021 => Some(
            "Add variable declarations to your contract file (e.g. FOO=value)".into(),
        ),
        DiagnosticCode::IO001 => Some(
            "Ensure the file exists and that process permissions allow reading it".into(),
        ),
        DiagnosticCode::IO002 => Some(
            "Re-encode file as valid UTF-8 without byte-order marks (BOM)".into(),
        ),
        DiagnosticCode::IO003 => Some(
            "Provide a path to a regular file instead of a directory or special device".into(),
        ),
        DiagnosticCode::IO010 => Some(
            "Increase limits using --max-file-size, --max-files, or --max-depth".into(),
        ),
    }
}

/// Renders a human-readable report to the specified writer.
pub fn render<W: Write>(report: &Report, writer: &mut W, use_color: bool) -> io::Result<()> {
    let c = Colors::new(use_color);

    if report.diagnostics.is_empty() {
        writeln!(
            writer,
            "{}{}Success:{} No environment contract violations found.",
            c.bold, c.green, c.reset
        )?;
        writeln!(
            writer,
            "{}Files scanned:{} {} | {}Files skipped:{} {}",
            c.dim,
            c.reset,
            report.summary.scanned_files,
            c.dim,
            c.reset,
            report.summary.skipped_files
        )?;
        return Ok(());
    }

    for diag in &report.diagnostics {
        let (badge, color) = match diag.severity {
            Severity::Error => ("[ERROR]", c.error),
            Severity::Warning => ("[WARN]", c.warning),
            Severity::Note => ("[NOTE]", c.note),
        };

        // Header: [ERROR] ENV001 (undocumented-reference): message
        writeln!(
            writer,
            "{}{}{} {}{}{} ({}{}{}){}: {}",
            color,
            badge,
            c.reset,
            c.bold,
            diag.code.as_str(),
            c.reset,
            c.dim,
            diag.code.identifier(),
            c.reset,
            c.bold,
            diag.message
        )?;

        // Primary location
        for loc in &diag.locations {
            writeln!(
                writer,
                "   {}-->{} {}:{}:{}",
                c.dim, c.reset, loc.path, loc.span.line, loc.span.column
            )?;
        }

        // Evidence kind
        if let Some(evidence) = diag.evidence {
            writeln!(
                writer,
                "   {}={}{} evidence: {}",
                c.dim,
                c.reset,
                c.dim,
                evidence.as_str()
            )?;
        }

        // Actionable advice
        if let Some(advice) = advice_for_diagnostic(diag) {
            writeln!(
                writer,
                "   {}={}{} advice: {}",
                c.dim, c.reset, c.dim, advice
            )?;
        }

        writeln!(writer)?;
    }

    // Summary divider
    writeln!(writer, "{}", "-".repeat(80))?;

    let status_color = if report.summary.total_errors > 0 || !report.summary.complete {
        c.error
    } else if report.summary.total_warnings > 0 {
        c.warning
    } else {
        c.green
    };

    let complete_str = if report.summary.complete {
        "complete"
    } else {
        "incomplete (limits or I/O errors encountered)"
    };

    writeln!(
        writer,
        "Scan {}{}{}: {} scanned, {} skipped | {}{} error(s){}, {}{} warning(s){}, {}{} note(s){}",
        status_color,
        complete_str,
        c.reset,
        report.summary.scanned_files,
        report.summary.skipped_files,
        if report.summary.total_errors > 0 {
            c.error
        } else {
            c.reset
        },
        report.summary.total_errors,
        c.reset,
        if report.summary.total_warnings > 0 {
            c.warning
        } else {
            c.reset
        },
        report.summary.total_warnings,
        c.reset,
        if report.summary.total_notes > 0 {
            c.note
        } else {
            c.reset
        },
        report.summary.total_notes,
        c.reset
    )?;

    Ok(())
}

/// Helper that renders report directly to a `String`.
pub fn render_to_string(report: &Report, use_color: bool) -> String {
    let mut buf = Vec::new();
    let _ = render(report, &mut buf, use_color);
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::span::{Location, Span};

    #[test]
    fn test_human_render_clean() {
        let report = Report::new(vec![], 10, 0, true);
        let output = render_to_string(&report, false);
        assert!(output.contains("Success: No environment contract violations found."));
        assert!(output.contains("Files scanned: 10"));
    }

    #[test]
    fn test_human_render_with_diagnostics() {
        let diag = Diagnostic::new(
            DiagnosticCode::ENV001,
            Severity::Warning,
            "Environment variable 'API_KEY' referenced in code is not documented",
            Some("API_KEY".into()),
            vec![Location::new("src/api.ts", Span::new(5, 12, 100, 115))],
            None,
        );
        let report = Report::new(vec![diag], 3, 0, true);
        let output = render_to_string(&report, false);
        assert!(output.contains("[WARN] ENV001 (undocumented-reference)"));
        assert!(output.contains("src/api.ts:5:12"));
        assert!(output.contains("advice: Declare 'API_KEY' in contract"));
        assert!(output.contains("1 warning(s)"));
    }
}
