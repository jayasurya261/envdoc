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
use std::collections::HashMap;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

struct RustScanner {
    path: RelPath,
    references: Vec<Reference>,
    diagnostics: Vec<Diagnostic>,
    const_strings: HashMap<String, String>,
}

impl RustScanner {
    fn new(path: RelPath) -> Self {
        Self {
            path,
            references: Vec::new(),
            diagnostics: Vec::new(),
            const_strings: HashMap::new(),
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
            ["std", "env", "vars"] | ["env", "vars"] => Some("vars"),
            ["dotenvy", "var"] => Some("dotenvy_var"),
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

    fn record_reference(&mut self, name: &str, span: Span, evidence_kind: EvidenceKind) {
        let env_name = EnvName::new(name);
        if env_name.is_valid_identifier() {
            self.references.push(Reference::new(
                env_name,
                self.path.clone(),
                span,
                evidence_kind,
            ));
        }
    }
}

impl<'ast> Visit<'ast> for RustScanner {
    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        let const_name = node.ident.to_string();
        if let Some(lit_str) = self.extract_lit_str(&node.expr) {
            self.const_strings.insert(const_name, lit_str.value());
        }
        visit::visit_item_const(self, node);
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        // Inspect attributes for clap / arg env bindings: #[arg(env = "...")] or #[clap(env = "...")]
        for attr in &node.attrs {
            let path_str = attr
                .path()
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");

            if path_str == "arg" || path_str == "clap" {
                if let syn::Meta::List(meta_list) = &attr.meta {
                    let token_str = meta_list.tokens.to_string();
                    if let Some(env_idx) = token_str.find("env") {
                        let after_env = &token_str[env_idx + 3..];
                        if let Some(eq_idx) = after_env.find('=') {
                            let after_eq = after_env[eq_idx + 1..].trim();
                            if let Some(first_quote) = after_eq.find('"') {
                                let after_quote = &after_eq[first_quote + 1..];
                                if let Some(end_quote) = after_quote.find('"') {
                                    let var_name = &after_quote[..end_quote];
                                    let span = self.to_span(attr.span());
                                    self.record_reference(
                                        var_name,
                                        span,
                                        EvidenceKind::DirectMember,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        visit::visit_field(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(ref expr_path) = *node.func {
            if let Some(fn_kind) = Self::check_path_match(&expr_path.path) {
                let call_span = self.to_span(node.span());

                if fn_kind == "vars" {
                    // std::env::vars() whole environment escape
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticCode::ENV012,
                        Severity::Note,
                        "Whole environment map accessed via std::env::vars()",
                        None,
                        vec![Location::new(self.path.clone(), call_span)],
                        Some(EvidenceKind::WholeObjectEscape),
                    ));
                    return;
                }

                if let Some(arg) = node.args.first() {
                    if let Some(lit_str) = self.extract_lit_str(arg) {
                        let val = lit_str.value();
                        let arg_span = self.to_span(lit_str.span());
                        self.record_reference(&val, arg_span, EvidenceKind::DirectMember);
                    } else if let syn::Expr::Path(ref arg_path) = arg {
                        // Check constant folding for local string constants
                        if let Some(ident) = arg_path.path.get_ident() {
                            if let Some(val) = self.const_strings.get(&ident.to_string()).cloned() {
                                self.record_reference(&val, call_span, EvidenceKind::DirectMember);
                            } else {
                                self.diagnostics.push(Diagnostic::new(
                                    DiagnosticCode::ENV010,
                                    Severity::Note,
                                    "Dynamic access pattern std::env::var(...) cannot be statically resolved",
                                    None,
                                    vec![Location::new(self.path.clone(), call_span)],
                                    Some(EvidenceKind::DynamicUnresolved),
                                ));
                            }
                        } else {
                            self.diagnostics.push(Diagnostic::new(
                                DiagnosticCode::ENV010,
                                Severity::Note,
                                "Dynamic access pattern std::env::var(...) cannot be statically resolved",
                                None,
                                vec![Location::new(self.path.clone(), call_span)],
                                Some(EvidenceKind::DynamicUnresolved),
                            ));
                        }
                    } else {
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
            let parsed_lit: Result<syn::LitStr, _> = syn::parse2(node.tokens.clone());
            match parsed_lit {
                Ok(lit_str) => {
                    let val = lit_str.value();
                    let arg_span = self.to_span(lit_str.span());
                    self.record_reference(&val, arg_span, EvidenceKind::DirectMember);
                }
                Err(_) => {
                    let parse_first = syn::parse::Parser::parse2(
                        |input: syn::parse::ParseStream| {
                            let lit: syn::LitStr = input.parse()?;
                            Ok(lit)
                        },
                        node.tokens.clone(),
                    );
                    if let Ok(lit_str) = parse_first {
                        let val = lit_str.value();
                        let arg_span = self.to_span(lit_str.span());
                        self.record_reference(&val, arg_span, EvidenceKind::DirectMember);
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
        } else {
            // General macro inspection (println!, format!, tracing::*, assert!, panic!)
            // Attempt to parse macro tokens as a punctuated list of expressions
            let parse_exprs = syn::parse::Parser::parse2(
                syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
                node.tokens.clone(),
            );

            if let Ok(exprs) = parse_exprs {
                for expr in &exprs {
                    // Check for build.rs "cargo:rerun-if-env-changed=VAR"
                    if let Some(lit_str) = self.extract_lit_str(expr) {
                        let val = lit_str.value();
                        if let Some(prefix_idx) = val.find("cargo:rerun-if-env-changed=") {
                            let var_name = val[prefix_idx + 27..].trim();
                            let span = self.to_span(lit_str.span());
                            self.record_reference(var_name, span, EvidenceKind::DirectMember);
                        }
                    }
                    self.visit_expr(expr);
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
    scanner
        .references
        .sort_by(|a, b| a.location.cmp(&b.location));

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
            const DB_KEY: &str = "DATABASE_URL";
            fn main() {
                let a = std::env::var(DB_KEY).unwrap();
                let b = env::var("API_KEY");
                let c = env!("BUILD_TIME_VAR");
                let d = option_env!("OPTIONAL_VAR");
                println!("Database URL: {}", std::env::var("PORT").unwrap());
                println!("cargo:rerun-if-env-changed=CUSTOM_BUILD_FLAG");
            }
            #[derive(clap::Parser)]
            struct Cli {
                #[arg(env = "CLAP_ENV_VAR")]
                flag: String,
            }
        "#;

        let result = scan_source(code, "src/main.rs");
        let names: Vec<_> = result.references.iter().map(|r| r.name.as_str()).collect();

        assert!(names.contains(&"DATABASE_URL"), "const folding failed");
        assert!(names.contains(&"API_KEY"));
        assert!(names.contains(&"BUILD_TIME_VAR"));
        assert!(names.contains(&"OPTIONAL_VAR"));
        assert!(names.contains(&"PORT"), "macro token traversal failed");
        assert!(
            names.contains(&"CUSTOM_BUILD_FLAG"),
            "cargo rerun-if failed"
        );
        assert!(names.contains(&"CLAP_ENV_VAR"), "clap attribute failed");
    }
}
