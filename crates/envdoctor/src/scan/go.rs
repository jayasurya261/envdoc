use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::severity::Severity;
use crate::model::evidence::EvidenceKind;
use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::reference::Reference;
use crate::model::span::{Location, Span};
use crate::scan::ScanResult;

/// Scans Go source files for environment variable accesses:
/// - `os.Getenv("VAR")`
/// - `os.LookupEnv("VAR")`
/// - `viper.GetString("VAR")`, `viper.Get("VAR")`, `viper.BindEnv("VAR")`, etc.
pub fn scan_source(source_text: &str, path: impl Into<RelPath>) -> ScanResult {
    let rel_path = path.into();
    let mut references = Vec::new();
    let mut diagnostics = Vec::new();

    let lines: Vec<&str> = source_text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let line_num = (line_idx + 1) as u32;

        // Strip inline comments // ...
        let code_line = if let Some(idx) = line.find("//") {
            &line[..idx]
        } else {
            *line
        };

        let patterns = [
            "os.Getenv",
            "os.LookupEnv",
            "viper.GetString",
            "viper.GetBool",
            "viper.GetInt",
            "viper.Get",
            "viper.BindEnv",
        ];

        for pattern in patterns {
            let mut search_from = 0;
            while let Some(pos) = code_line[search_from..].find(pattern) {
                let abs_pos = search_from + pos;
                let after_pattern = &code_line[abs_pos + pattern.len()..];

                // Expect '('
                let trimmed = after_pattern.trim_start();
                if let Some(stripped) = trimmed.strip_prefix('(') {
                    let inside = stripped.trim_start();
                    let col = (abs_pos + 1) as u32;
                    let span = Span::point(line_num, col, abs_pos as u32);

                    if inside.starts_with('"') || inside.starts_with('`') {
                        let quote = inside.chars().next().unwrap();
                        let rest = &inside[1..];
                        if let Some(end_quote) = rest.find(quote) {
                            let key = &rest[..end_quote];
                            let env_name = EnvName::new(key);
                            if env_name.is_valid_identifier() {
                                references.push(Reference::new(
                                    env_name,
                                    rel_path.clone(),
                                    span,
                                    EvidenceKind::DirectMember,
                                ));
                            }
                        }
                    } else if !inside.starts_with(')') {
                        // Unresolved dynamic parameter in call
                        diagnostics.push(Diagnostic::new(
                            DiagnosticCode::ENV010,
                            Severity::Note,
                            format!(
                                "Dynamic access pattern {}(...) cannot be statically resolved",
                                pattern
                            ),
                            None,
                            vec![Location::new(rel_path.clone(), span)],
                            Some(EvidenceKind::DynamicUnresolved),
                        ));
                    }
                }

                search_from = abs_pos + pattern.len();
            }
        }
    }

    ScanResult {
        references,
        diagnostics,
    }
}
