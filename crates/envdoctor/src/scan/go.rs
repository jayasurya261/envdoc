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
/// - `os.Environ()` (whole-environment escape)
/// - `viper.BindEnv("VAR")` or `viper.BindEnv("configKey", "ENV_VAR")`
///
/// NOTE: `viper.GetString("key")` reads config keys, NOT env vars by default.
/// It is deliberately omitted here to eliminate false ENV001 findings on config keys.
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

        // 1. Check for os.Environ() whole-object escape
        if let Some(pos) = code_line.find("os.Environ()") {
            let col = (pos + 1) as u32;
            let span = Span::point(line_num, col, pos as u32);
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::ENV012,
                Severity::Note,
                "os.Environ() accesses entire process environment".to_string(),
                None,
                vec![Location::new(rel_path.clone(), span)],
                Some(EvidenceKind::WholeObjectEscape),
            ));
        }

        // 2. Scan standard os.Getenv and os.LookupEnv calls
        for pattern in &["os.Getenv", "os.LookupEnv"] {
            let mut search_from = 0;
            while let Some(pos) = code_line[search_from..].find(pattern) {
                let abs_pos = search_from + pos;
                let after_pattern = &code_line[abs_pos + pattern.len()..];

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

        // 3. Scan viper.BindEnv calls
        // In Viper:
        // BindEnv("ENV_VAR") -> 1 arg: the arg is the env var name
        // BindEnv("configKey", "ENV_VAR") -> 2 args: the 2nd arg is the env var name!
        let mut viper_search = 0;
        while let Some(pos) = code_line[viper_search..].find("viper.BindEnv") {
            let abs_pos = viper_search + pos;
            let after = &code_line[abs_pos + "viper.BindEnv".len()..];
            let trimmed = after.trim_start();
            if let Some(stripped) = trimmed.strip_prefix('(') {
                if let Some(closing_paren) = stripped.find(')') {
                    let args_str = &stripped[..closing_paren];
                    let col = (abs_pos + 1) as u32;
                    let span = Span::point(line_num, col, abs_pos as u32);

                    // Parse comma-separated string literals
                    let raw_args: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
                    let string_args: Vec<String> = raw_args
                        .iter()
                        .filter_map(|arg| {
                            if (arg.starts_with('"') && arg.ends_with('"'))
                                || (arg.starts_with('`') && arg.ends_with('`'))
                            {
                                if arg.len() >= 2 {
                                    Some(arg[1..arg.len() - 1].to_string())
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .collect();

                    if string_args.len() == 1 {
                        // 1 argument: the argument is the environment variable name
                        let env_name = EnvName::new(&string_args[0]);
                        if env_name.is_valid_identifier() {
                            references.push(Reference::new(
                                env_name,
                                rel_path.clone(),
                                span,
                                EvidenceKind::DirectMember,
                            ));
                        }
                    } else if string_args.len() >= 2 {
                        // 2+ arguments: all arguments after the first (config key) are env variable names
                        for env_str in &string_args[1..] {
                            let env_name = EnvName::new(env_str);
                            if env_name.is_valid_identifier() {
                                references.push(Reference::new(
                                    env_name,
                                    rel_path.clone(),
                                    span,
                                    EvidenceKind::DirectMember,
                                ));
                            }
                        }
                    } else if !raw_args.is_empty() && !raw_args[0].is_empty() {
                        diagnostics.push(Diagnostic::new(
                            DiagnosticCode::ENV010,
                            Severity::Note,
                            "Dynamic access pattern viper.BindEnv(...) cannot be statically resolved"
                                .to_string(),
                            None,
                            vec![Location::new(rel_path.clone(), span)],
                            Some(EvidenceKind::DynamicUnresolved),
                        ));
                    }
                }
            }
            viper_search = abs_pos + "viper.BindEnv".len();
        }
    }

    ScanResult {
        references,
        diagnostics,
    }
}
