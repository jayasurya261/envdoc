//! Rust source code AST scanner using `syn`.

use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::severity::Severity;
use crate::model::evidence::EvidenceKind;
use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::reference::Reference;
use crate::model::span::{Location, Span};
use crate::scan::ScanResult;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

struct RustScanner {
    path: RelPath,
    references: Vec<Reference>,
    diagnostics: Vec<Diagnostic>,
}

impl RustScanner {
    fn new(path: RelPath) -> Self {
        Self {
            path,
            references: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn to_span(&self, span: proc_macro2::Span) -> Span {
        let start = span.start();
        let end = span.end();
        Span::new(
            start.line as u32,
            (start.column + 1) as u32,
            end.line as u32,
            (end.column + 1) as u32,
        )
    }

    fn check_path_match(path: &syn::Path) -> Option<&'static str> {
        let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        let seg_refs: Vec<&str> = segments.iter().map(|s| s.as_str()).collect();

        match seg_refs.as_slice() {
            ["std", "env", "var"] | ["env", "var"] => Some("var"),
            ["std", "env", "var_os"] | ["env", "var_os"] => Some("var_os"),
            _ => None,
        }
    }

    fn extract_lit_str<'a>(&self, expr: &'a syn::Expr) -> Option<&'a syn::LitStr> {
        match expr {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) => Some(s),
            syn::Expr::Reference(syn::ExprReference { expr: inner, .. }) => {
                self.extract_lit_str(inner)
            }
            syn::Expr::Group(syn::ExprGroup { expr: inner, .. }) => self.extract_lit_str(inner),
            _ => None,
        }
    }
}

impl<'ast> Visit<'ast> for RustScanner {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(ref expr_path) = *node.func {
            if let Some(_fn_kind) = Self::check_path_match(&expr_path.path) {
                let call_span = self.to_span(node.span());
                if let Some(arg) = node.args.first() {
                    if let Some(lit_str) = self.extract_lit_str(arg) {
                        let val = lit_str.value();
                        let env_name = EnvName::new(val);
                        if env_name.is_valid_identifier() {
                            let arg_span = self.to_span(lit_str.span());
                            self.references.push(Reference::new(
                                env_name,
                                self.path.clone(),
                                arg_span,
                                EvidenceKind::DirectMember,
                            ));
                        }
                    } else {
                        // Dynamic expression passed to std::env::var(...)
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticCode::ENV010,
                            Severity::Note,
                            "Dynamic access pattern std::env::var(...) cannot be statically resolved",
                            None,
                            vec![Location::new(self.path.clone(), call_span)],
                            Some(EvidenceKind::DynamicUnresolved),
                        ));
                    }
                }
            }
        }

        visit::visit_expr_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let segs: Vec<_> = node
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let last = segs.last().map(|s| s.as_str());

        if last == Some("env") || last == Some("option_env") {
            let macro_span = self.to_span(node.span());
            // Attempt to parse macro tokens as a string literal (e.g. `env!("DATABASE_URL")`)
            let parsed_lit: Result<syn::LitStr, _> = syn::parse2(node.tokens.clone());
            match parsed_lit {
                Ok(lit_str) => {
                    let val = lit_str.value();
                    let env_name = EnvName::new(val);
                    if env_name.is_valid_identifier() {
                        let arg_span = self.to_span(lit_str.span());
                        self.references.push(Reference::new(
                            env_name,
                            self.path.clone(),
                            arg_span,
                            EvidenceKind::DirectMember,
                        ));
                    }
                }
                Err(_) => {
                    // Try parsing as comma-separated exprs where first is a LitStr
                    let parse_first = syn::parse::Parser::parse2(
                        |input: syn::parse::ParseStream| {
                            let lit: syn::LitStr = input.parse()?;
                            Ok(lit)
                        },
                        node.tokens.clone(),
                    );
                    if let Ok(lit_str) = parse_first {
                        let val = lit_str.value();
                        let env_name = EnvName::new(val);
                        if env_name.is_valid_identifier() {
                            let arg_span = self.to_span(lit_str.span());
                            self.references.push(Reference::new(
                                env_name,
                                self.path.clone(),
                                arg_span,
                                EvidenceKind::DirectMember,
                            ));
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticCode::ENV010,
                            Severity::Note,
                            format!(
                                "Dynamic macro pattern {}!(...) cannot be statically resolved",
                                last.unwrap_or("env")
                            ),
                            None,
                            vec![Location::new(self.path.clone(), macro_span)],
                            Some(EvidenceKind::DynamicUnresolved),
                        ));
                    }
                }
            }
        }

        visit::visit_macro(self, node);
    }
}

/// Scans in-memory Rust source code for environment variable accesses.
pub fn scan_source(source_text: &str, path: impl Into<RelPath>) -> ScanResult {
    let rel_path = path.into();
    let mut scanner = RustScanner::new(rel_path.clone());

    match syn::parse_file(source_text) {
        Ok(file) => {
            scanner.visit_file(&file);
        }
        Err(err) => {
            let span = scanner.to_span(err.span());
            scanner.diagnostics.push(Diagnostic::new(
                DiagnosticCode::ENV011,
                Severity::Warning,
                format!("Source file parsing problem: {}", err),
                None,
                vec![Location::new(rel_path, span)],
                None,
            ));
        }
    }

    // Deterministically sort diagnostics & references
    Diagnostic::sort_diagnostics(&mut scanner.diagnostics);
    scanner.references.sort_by(|a, b| a.location.cmp(&b.location));

    ScanResult {
        references: scanner.references,
        diagnostics: scanner.diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_var_and_macros() {
        let code = r#"
fn main() {
    let db = std::env::var("DATABASE_URL").unwrap();
    let port = env::var("PORT").unwrap_or_default();
    let secret = env!("JWT_SECRET");
    let opt = option_env!("OPTIONAL_FEATURE");
    let dynamic_key = "DYNAMIC";
    let _ = std::env::var(dynamic_key);
}
"#;
        let res = scan_source(code, "src/main.rs");
        let ref_names: Vec<&str> = res.references.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            ref_names,
            vec!["DATABASE_URL", "PORT", "JWT_SECRET", "OPTIONAL_FEATURE"]
        );

        assert_eq!(res.diagnostics.len(), 1);
        assert_eq!(res.diagnostics[0].code, DiagnosticCode::ENV010);
    }
}
