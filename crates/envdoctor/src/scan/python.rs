use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::severity::Severity;
use crate::model::evidence::EvidenceKind;
use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::reference::Reference;
use crate::model::span::{Location, Span};
use crate::scan::ScanResult;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_parser::parse_module;
use ruff_text_size::Ranged;

pub fn scan_source(source_text: &str, path: impl Into<RelPath>) -> ScanResult {
    let rel_path = path.into();
    let mut scanner = PythonScannerImpl {
        path: rel_path,
        source: source_text,
        references: Vec::new(),
        diagnostics: Vec::new(),
    };

    match parse_module(source_text) {
        Ok(parsed) => {
            if !parsed.errors().is_empty() {
                scanner.diagnostics.push(Diagnostic::new(
                    DiagnosticCode::ENV011,
                    Severity::Warning,
                    "Python parser encountered syntax errors; results may be partial".to_string(),
                    None,
                    vec![Location::new(scanner.path.clone(), Span::point(1, 1, 0))],
                    None,
                ));
            }
            let module = parsed.into_syntax();
            for stmt in &module.body {
                scanner.visit_stmt(stmt);
            }
        }
        Err(_) => {
            scanner.diagnostics.push(Diagnostic::new(
                DiagnosticCode::ENV011,
                Severity::Warning,
                "Python parser failed to parse file; recovered safely".to_string(),
                None,
                vec![Location::new(scanner.path.clone(), Span::point(1, 1, 0))],
                None,
            ));
        }
    }

    ScanResult {
        references: scanner.references,
        diagnostics: scanner.diagnostics,
    }
}

struct PythonScannerImpl<'a> {
    path: RelPath,
    source: &'a str,
    references: Vec<Reference>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> PythonScannerImpl<'a> {
    fn to_span(&self, range: ruff_text_size::TextRange) -> Span {
        let start_offset = u32::from(range.start());
        let end_offset = u32::from(range.end());

        let mut line = 1;
        let mut col = 1;
        for (i, b) in self.source.bytes().enumerate() {
            if i == start_offset as usize {
                break;
            }
            if b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        Span::new(line, col, start_offset, end_offset)
    }

    fn record_reference(&mut self, key: &str, span: Span, evidence: EvidenceKind) {
        let env_name = EnvName::new(key);
        if env_name.is_valid_identifier() {
            self.references
                .push(Reference::new(env_name, self.path.clone(), span, evidence));
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::ClassDef(class_def) => {
                self.handle_class_def(class_def);
                for inner in &class_def.body {
                    self.visit_stmt(inner);
                }
            }
            Stmt::FunctionDef(fn_def) => {
                for inner in &fn_def.body {
                    self.visit_stmt(inner);
                }
            }
            Stmt::Expr(expr_stmt) => {
                self.visit_expr(&expr_stmt.value);
            }
            Stmt::Assign(assign_stmt) => {
                self.visit_expr(&assign_stmt.value);
            }
            Stmt::AnnAssign(ann_assign) => {
                if let Some(val) = &ann_assign.value {
                    self.visit_expr(val);
                }
            }
            Stmt::Return(ret_stmt) => {
                if let Some(val) = &ret_stmt.value {
                    self.visit_expr(val);
                }
            }
            Stmt::If(if_stmt) => {
                self.visit_expr(&if_stmt.test);
                for s in &if_stmt.body {
                    self.visit_stmt(s);
                }
                for s in &if_stmt.elif_else_clauses {
                    if let Some(test) = &s.test {
                        self.visit_expr(test);
                    }
                    for inner in &s.body {
                        self.visit_stmt(inner);
                    }
                }
            }
            Stmt::For(for_stmt) => {
                self.visit_expr(&for_stmt.iter);
                for s in &for_stmt.body {
                    self.visit_stmt(s);
                }
            }
            Stmt::While(while_stmt) => {
                self.visit_expr(&while_stmt.test);
                for s in &while_stmt.body {
                    self.visit_stmt(s);
                }
            }
            Stmt::With(with_stmt) => {
                for item in &with_stmt.items {
                    self.visit_expr(&item.context_expr);
                }
                for s in &with_stmt.body {
                    self.visit_stmt(s);
                }
            }
            Stmt::Try(try_stmt) => {
                for s in &try_stmt.body {
                    self.visit_stmt(s);
                }
                for handler in &try_stmt.handlers {
                    match handler {
                        ast::ExceptHandler::ExceptHandler(h) => {
                            for s in &h.body {
                                self.visit_stmt(s);
                            }
                        }
                    }
                }
                for s in &try_stmt.orelse {
                    self.visit_stmt(s);
                }
                for s in &try_stmt.finalbody {
                    self.visit_stmt(s);
                }
            }
            _ => {}
        }
    }

    /// Handles Pydantic BaseSettings class declarations:
    /// class Settings(BaseSettings):
    ///     model_config = SettingsConfigDict(env_prefix="MY_APP_", case_sensitive=False)
    ///     database_url: str
    ///     api_key: str = "default"
    fn handle_class_def(&mut self, class_def: &ast::StmtClassDef) {
        let is_base_settings = class_def.bases().iter().any(|base| {
            if let Expr::Name(name) = base {
                name.id.as_str() == "BaseSettings"
            } else if let Expr::Attribute(attr) = base {
                attr.attr.as_str() == "BaseSettings"
            } else {
                false
            }
        });

        if !is_base_settings {
            return;
        }

        let mut env_prefix = String::new();
        let mut case_sensitive = false;

        // Extract class-level configuration (Pydantic v2 model_config or v1 Config class)
        for item in &class_def.body {
            match item {
                // Pydantic v2: model_config = SettingsConfigDict(env_prefix="...", case_sensitive=...)
                // or model_config = {"env_prefix": "...", "case_sensitive": ...}
                Stmt::Assign(assign) => {
                    for target in &assign.targets {
                        if let Expr::Name(name) = target {
                            if name.id.as_str() == "model_config" {
                                Self::extract_config_dict(
                                    &assign.value,
                                    &mut env_prefix,
                                    &mut case_sensitive,
                                );
                            }
                        }
                    }
                }
                Stmt::AnnAssign(ann) => {
                    if let Expr::Name(name) = ann.target.as_ref() {
                        if name.id.as_str() == "model_config" {
                            if let Some(val) = &ann.value {
                                Self::extract_config_dict(
                                    val,
                                    &mut env_prefix,
                                    &mut case_sensitive,
                                );
                            }
                        }
                    }
                }
                // Pydantic v1:
                // class Config:
                //     env_prefix = "..."
                //     case_sensitive = True
                Stmt::ClassDef(nested_class) if nested_class.name.as_str() == "Config" => {
                    for config_item in &nested_class.body {
                        if let Stmt::Assign(config_assign) = config_item {
                            for target in &config_assign.targets {
                                if let Expr::Name(name) = target {
                                    if name.id.as_str() == "env_prefix" {
                                        if let Expr::StringLiteral(s) = config_assign.value.as_ref()
                                        {
                                            env_prefix = s.value.to_str().to_string();
                                        }
                                    } else if name.id.as_str() == "case_sensitive" {
                                        if let Expr::BooleanLiteral(b) =
                                            config_assign.value.as_ref()
                                        {
                                            case_sensitive = b.value;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        for item in &class_def.body {
            if let Stmt::AnnAssign(ann) = item {
                if let Expr::Name(name) = ann.target.as_ref() {
                    let field_name = name.id.as_str();
                    if field_name == "model_config" {
                        continue;
                    }
                    let span = self.to_span(ann.range());

                    // Map field name with env_prefix and case sensitivity
                    let combined = format!("{}{}", env_prefix, field_name);
                    let env_var_name = if case_sensitive {
                        combined
                    } else {
                        combined.to_ascii_uppercase()
                    };
                    self.record_reference(&env_var_name, span, EvidenceKind::DirectMember);

                    // Check if Field(validation_alias="CUSTOM_NAME"), alias="...", or env="..." (v1)
                    if let Some(val) = &ann.value {
                        if let Expr::Call(call) = val.as_ref() {
                            for kw in &call.arguments.keywords {
                                if let Some(arg_name) = &kw.arg {
                                    if arg_name.as_str() == "validation_alias"
                                        || arg_name.as_str() == "alias"
                                        || arg_name.as_str() == "env"
                                    {
                                        if let Expr::StringLiteral(s) = &kw.value {
                                            self.record_reference(
                                                s.value.to_str(),
                                                self.to_span(s.range()),
                                                EvidenceKind::DirectMember,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn extract_config_dict(expr: &Expr, env_prefix: &mut String, case_sensitive: &mut bool) {
        match expr {
            Expr::Call(call) => {
                for kw in &call.arguments.keywords {
                    if let Some(arg_name) = &kw.arg {
                        if arg_name.as_str() == "env_prefix" {
                            if let Expr::StringLiteral(s) = &kw.value {
                                *env_prefix = s.value.to_str().to_string();
                            }
                        } else if arg_name.as_str() == "case_sensitive" {
                            if let Expr::BooleanLiteral(b) = &kw.value {
                                *case_sensitive = b.value;
                            }
                        }
                    }
                }
            }
            Expr::Dict(dict) => {
                for item in &dict.items {
                    if let Some(Expr::StringLiteral(key_lit)) = &item.key {
                        let k = key_lit.value.to_str();
                        if k == "env_prefix" {
                            if let Expr::StringLiteral(s) = &item.value {
                                *env_prefix = s.value.to_str().to_string();
                            }
                        } else if k == "case_sensitive" {
                            if let Expr::BooleanLiteral(b) = &item.value {
                                *case_sensitive = b.value;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            // Pattern 1: os.environ["KEY"] or environ["KEY"]
            Expr::Subscript(sub) => {
                if self.is_environ_expr(&sub.value) {
                    let span = self.to_span(sub.range());
                    if let Expr::StringLiteral(lit) = sub.slice.as_ref() {
                        self.record_reference(
                            lit.value.to_str(),
                            span,
                            EvidenceKind::BracketLiteral,
                        );
                    } else {
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticCode::ENV010,
                            Severity::Note,
                            "Dynamic access pattern os.environ[...] cannot be statically resolved"
                                .to_string(),
                            None,
                            vec![Location::new(self.path.clone(), span)],
                            Some(EvidenceKind::DynamicUnresolved),
                        ));
                    }
                } else {
                    self.visit_expr(&sub.value);
                    self.visit_expr(&sub.slice);
                }
            }

            // Pattern 2: os.getenv("KEY"), os.environ.get("KEY"), getenv("KEY")
            Expr::Call(call) => {
                let call_span = self.to_span(call.range());
                if self.is_getenv_callee(&call.func) {
                    if let Some(first_arg) = call.arguments.args.first() {
                        if let Expr::StringLiteral(lit) = first_arg {
                            self.record_reference(
                                lit.value.to_str(),
                                call_span,
                                EvidenceKind::DirectMember,
                            );
                        } else {
                            self.diagnostics.push(Diagnostic::new(
                                DiagnosticCode::ENV010,
                                Severity::Note,
                                "Dynamic access pattern os.getenv(...) cannot be statically resolved"
                                    .to_string(),
                                None,
                                vec![Location::new(self.path.clone(), call_span)],
                                Some(EvidenceKind::DynamicUnresolved),
                            ));
                        }
                    }
                } else {
                    self.visit_expr(&call.func);
                    for arg in &call.arguments.args {
                        self.visit_expr(arg);
                    }
                    for kw in &call.arguments.keywords {
                        self.visit_expr(&kw.value);
                    }
                }
            }

            Expr::Attribute(attr) => {
                self.visit_expr(&attr.value);
            }
            Expr::BinOp(bin_op) => {
                self.visit_expr(&bin_op.left);
                self.visit_expr(&bin_op.right);
            }
            Expr::UnaryOp(unary) => {
                self.visit_expr(&unary.operand);
            }
            Expr::Dict(dict) => {
                for item in &dict.items {
                    if let Some(key) = &item.key {
                        self.visit_expr(key);
                    }
                    self.visit_expr(&item.value);
                }
            }
            Expr::List(list) => {
                for elt in &list.elts {
                    self.visit_expr(elt);
                }
            }
            Expr::Tuple(tuple) => {
                for elt in &tuple.elts {
                    self.visit_expr(elt);
                }
            }
            _ => {}
        }
    }

    fn is_environ_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Attribute(attr) => {
                attr.attr.as_str() == "environ"
                    && matches!(attr.value.as_ref(), Expr::Name(n) if n.id.as_str() == "os")
            }
            Expr::Name(name) => name.id.as_str() == "environ",
            _ => false,
        }
    }

    fn is_getenv_callee(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Attribute(attr) => {
                let method = attr.attr.as_str();
                if method == "getenv" {
                    matches!(attr.value.as_ref(), Expr::Name(n) if n.id.as_str() == "os")
                } else if method == "get" {
                    self.is_environ_expr(&attr.value)
                } else {
                    false
                }
            }
            Expr::Name(name) => name.id.as_str() == "getenv",
            _ => false,
        }
    }
}
