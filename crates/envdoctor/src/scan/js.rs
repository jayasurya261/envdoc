use crate::diagnostics::code::DiagnosticCode;
use crate::diagnostics::diagnostic::Diagnostic;
use crate::diagnostics::severity::Severity;
use crate::model::evidence::EvidenceKind;
use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::reference::Reference;
use crate::model::span::{Location, Span};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use std::collections::{HashMap, HashSet};

/// Result of scanning JavaScript/TypeScript source code.
///
/// Invariant: Completely encapsulates parser types; no `oxc` types leak out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    /// Environment variable references found in the AST.
    pub references: Vec<Reference>,
    /// Diagnostics emitted during scanning (e.g. syntax errors, dynamic access, escapes).
    pub diagnostics: Vec<Diagnostic>,
}

/// Scans in-memory JavaScript or TypeScript source code for environment variable accesses.
///
/// Pure function: 100% in-memory with zero file I/O.
pub fn scan_source(source_text: &str, path: impl Into<RelPath>) -> ScanResult {
    let rel_path = path.into();
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(rel_path.as_path())
        .unwrap_or_default()
        .with_jsx(true);

    let ret = Parser::new(&allocator, source_text, source_type).parse();

    let line_index = LineIndex::new(source_text);
    let mut scanner = AstScanner::new(rel_path, line_index);

    // Step 4.4: Recovery from Parse Errors
    // Emit ENV011 for any syntax errors while still analyzing partial AST
    for err in ret.diagnostics {
        let msg = err.to_string();
        let span = if let Some(first_label) = err.labels.first() {
            scanner.line_index.to_span(
                first_label.offset(),
                first_label.offset() + first_label.len(),
            )
        } else {
            Span::point(1, 1, 0)
        };

        scanner.diagnostics.push(Diagnostic::new(
            DiagnosticCode::ENV011,
            Severity::Warning,
            format!("Source file parsing problem: {}", msg),
            None,
            vec![Location::new(scanner.path.clone(), span)],
            None,
        ));
    }

    scanner.visit_program(&ret.program);

    // Deterministically sort diagnostics: path -> line -> col -> code -> name
    Diagnostic::sort_diagnostics(&mut scanner.diagnostics);

    ScanResult {
        references: scanner.references,
        diagnostics: scanner.diagnostics,
    }
}

/// Fast line index for computing 1-based (line, column) coordinates from 0-based byte offsets.
#[derive(Debug, Clone)]
struct LineIndex {
    line_starts: Vec<u32>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (idx, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push((idx + 1) as u32);
            }
        }
        Self { line_starts }
    }

    fn to_span(&self, start: u32, end: u32) -> Span {
        let line_idx = match self.line_starts.binary_search(&start) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        let line = (line_idx + 1) as u32;
        let line_start = self.line_starts[line_idx];
        let column = start.saturating_sub(line_start) + 1;
        Span::new(line, column, start, end)
    }
}

struct AstScanner {
    path: RelPath,
    line_index: LineIndex,
    references: Vec<Reference>,
    diagnostics: Vec<Diagnostic>,
    const_strings: HashMap<String, String>,
    local_aliases: HashSet<String>,
}

impl AstScanner {
    fn new(path: RelPath, line_index: LineIndex) -> Self {
        Self {
            path,
            line_index,
            references: Vec::new(),
            diagnostics: Vec::new(),
            const_strings: HashMap::new(),
            local_aliases: HashSet::new(),
        }
    }

    fn span_of(&self, span: oxc_span::Span) -> Span {
        self.line_index.to_span(span.start, span.end)
    }

    fn emit_escape(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(
            DiagnosticCode::ENV012,
            Severity::Note,
            message,
            None,
            vec![Location::new(self.path.clone(), span)],
            Some(EvidenceKind::WholeObjectEscape),
        ));
    }

    fn emit_dynamic_access(&mut self, span: Span) {
        self.diagnostics.push(Diagnostic::new(
            DiagnosticCode::ENV010,
            Severity::Note,
            "Dynamic access pattern process.env[...] cannot be statically resolved",
            None,
            vec![Location::new(self.path.clone(), span)],
            Some(EvidenceKind::DynamicUnresolved),
        ));
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

    fn is_env_source(&self, expr: &Expression) -> bool {
        if is_env_object(expr) {
            return true;
        }
        if let Expression::Identifier(id) = expr {
            return self.local_aliases.contains(id.name.as_str());
        }
        false
    }

    fn visit_program<'a>(&mut self, program: &Program<'a>) {
        for stmt in &program.body {
            self.visit_statement(stmt);
        }
    }

    fn visit_statement<'a>(&mut self, stmt: &Statement<'a>) {
        match stmt {
            Statement::VariableDeclaration(decl) => {
                self.visit_variable_declaration(decl);
            }
            Statement::ExpressionStatement(s) => {
                self.visit_expression(&s.expression);
            }
            Statement::BlockStatement(s) => {
                for inner in &s.body {
                    self.visit_statement(inner);
                }
            }
            Statement::IfStatement(s) => {
                self.visit_expression(&s.test);
                self.visit_statement(&s.consequent);
                if let Some(alt) = &s.alternate {
                    self.visit_statement(alt);
                }
            }
            Statement::ReturnStatement(s) => {
                if let Some(arg) = &s.argument {
                    if self.is_env_source(arg) {
                        let span = self.span_of(arg.span());
                        self.emit_escape(span, "Environment object returned directly");
                    } else {
                        self.visit_expression(arg);
                    }
                }
            }
            Statement::ThrowStatement(s) => {
                self.visit_expression(&s.argument);
            }
            Statement::TryStatement(s) => {
                for inner in &s.block.body {
                    self.visit_statement(inner);
                }
                if let Some(handler) = &s.handler {
                    for inner in &handler.body.body {
                        self.visit_statement(inner);
                    }
                }
                if let Some(finalizer) = &s.finalizer {
                    for inner in &finalizer.body {
                        self.visit_statement(inner);
                    }
                }
            }
            Statement::WhileStatement(s) => {
                self.visit_expression(&s.test);
                self.visit_statement(&s.body);
            }
            Statement::DoWhileStatement(s) => {
                self.visit_statement(&s.body);
                self.visit_expression(&s.test);
            }
            Statement::ForStatement(s) => {
                if let Some(init) = &s.init {
                    match init {
                        ForStatementInit::VariableDeclaration(decl) => {
                            self.visit_variable_declaration(decl);
                        }
                        _ => {
                            if let Some(e) = init.as_expression() {
                                self.visit_expression(e);
                            }
                        }
                    }
                }
                if let Some(test) = &s.test {
                    self.visit_expression(test);
                }
                if let Some(update) = &s.update {
                    self.visit_expression(update);
                }
                self.visit_statement(&s.body);
            }
            Statement::ForInStatement(s) => {
                self.visit_expression(&s.right);
                self.visit_statement(&s.body);
            }
            Statement::ForOfStatement(s) => {
                self.visit_expression(&s.right);
                self.visit_statement(&s.body);
            }
            Statement::SwitchStatement(s) => {
                self.visit_expression(&s.discriminant);
                for case in &s.cases {
                    if let Some(test) = &case.test {
                        self.visit_expression(test);
                    }
                    for inner in &case.consequent {
                        self.visit_statement(inner);
                    }
                }
            }
            Statement::FunctionDeclaration(s) => {
                if let Some(body) = &s.body {
                    for inner in &body.statements {
                        self.visit_statement(inner);
                    }
                }
            }
            Statement::ClassDeclaration(c) => {
                self.visit_class_elements(&c.body.body);
            }
            Statement::ExportDefaultDeclaration(s) => match &s.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    if let Some(body) = &func.body {
                        for inner in &body.statements {
                            self.visit_statement(inner);
                        }
                    }
                }
                ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                    self.visit_class_elements(&c.body.body);
                }
                _ => {
                    if let Some(e) = s.declaration.as_expression() {
                        if self.is_env_source(e) {
                            let span = self.span_of(e.span());
                            self.emit_escape(span, "Environment object exported directly");
                        } else {
                            self.visit_expression(e);
                        }
                    }
                }
            },
            Statement::ExportDeclaration(s) => match &s.declaration {
                Declaration::VariableDeclaration(decl) => {
                    self.visit_variable_declaration(decl);
                }
                Declaration::FunctionDeclaration(func) => {
                    if let Some(body) = &func.body {
                        for inner in &body.statements {
                            self.visit_statement(inner);
                        }
                    }
                }
                Declaration::ClassDeclaration(c) => {
                    self.visit_class_elements(&c.body.body);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn visit_variable_declaration<'a>(&mut self, decl: &VariableDeclaration<'a>) {
        for declarator in &decl.declarations {
            if let Some(init) = &declarator.init {
                // Constant string tracking: const K = "X" (screaming snake case or all caps)
                if decl.kind == oxc_ast::ast::VariableDeclarationKind::Const {
                    if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                        let name = id.name.as_str();
                        if name
                            .chars()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                            && name.chars().any(|c| c.is_ascii_uppercase())
                        {
                            if let Expression::StringLiteral(s) = init {
                                if let Some(val) = s.value.as_str() {
                                    self.const_strings
                                        .insert(id.name.to_string(), val.to_string());
                                }
                            }
                        }
                    }
                }

                if self.is_env_source(init) {
                    match &declarator.id {
                        BindingPattern::ObjectPattern(obj) => {
                            self.handle_object_destructuring(obj);
                        }
                        BindingPattern::BindingIdentifier(id) => {
                            // Intraprocedural flow: track local alias and emit note for aliasing
                            self.local_aliases.insert(id.name.to_string());
                            let span = self.span_of(init.span());
                            self.emit_escape(span, "Environment object aliased to variable");
                        }
                        _ => {
                            let span = self.span_of(init.span());
                            self.emit_escape(span, "Environment object aliased to complex pattern");
                        }
                    }
                } else {
                    self.visit_expression(init);
                }
            }
        }
    }

    fn handle_object_destructuring<'a>(&mut self, obj: &ObjectPattern<'a>) {
        // Pattern 4: Object destructuring with renames and defaults
        for prop in &obj.properties {
            let span = self.span_of(prop.span);
            match &prop.key {
                PropertyKey::StaticIdentifier(id) => {
                    self.record_reference(id.name.as_str(), span, EvidenceKind::Destructured);
                }
                PropertyKey::StringLiteral(s) => {
                    if let Some(val) = s.value.as_str() {
                        self.record_reference(val, span, EvidenceKind::Destructured);
                    }
                }
                _ => {}
            }
        }

        // Pattern 5: Rest pattern destructuring: emit ENV012
        if let Some(rest) = &obj.rest {
            let span = self.span_of(rest.span);
            self.emit_escape(
                span,
                "Rest pattern destructuring captures entire environment object",
            );
        }
    }

    fn visit_class_elements<'a>(&mut self, elements: &[ClassElement<'a>]) {
        for elem in elements {
            match elem {
                ClassElement::MethodDefinition(m) => {
                    if let Some(body) = &m.value.body {
                        for inner in &body.statements {
                            self.visit_statement(inner);
                        }
                    }
                }
                ClassElement::PropertyDefinition(p) => {
                    if let Some(val) = &p.value {
                        self.visit_expression(val);
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_expression<'a>(&mut self, expr: &Expression<'a>) {
        // Pattern 1: Direct Member Expression (process.env.FOO, e.FOO, Bun.env.FOO, import.meta.env.FOO)
        if let Some(mem) = expr.as_member_expression() {
            match mem {
                MemberExpression::StaticMemberExpression(m) => {
                    if self.is_env_source(&m.object) {
                        let span = self.span_of(m.span);
                        self.record_reference(
                            m.property.name.as_str(),
                            span,
                            EvidenceKind::DirectMember,
                        );
                        return;
                    }
                }
                MemberExpression::ComputedMemberExpression(m) => {
                    if self.is_env_source(&m.object) {
                        let span = self.span_of(m.span);
                        match &m.expression {
                            // Pattern 2: Bracket access with string literal
                            Expression::StringLiteral(s) => {
                                if let Some(val) = s.value.as_str() {
                                    self.record_reference(val, span, EvidenceKind::BracketLiteral);
                                }
                            }
                            // Pattern 3: Template literal without expressions
                            Expression::TemplateLiteral(t) => {
                                if t.expressions.is_empty() && t.quasis.len() == 1 {
                                    let key = t.quasis[0].value.raw.as_str();
                                    self.record_reference(key, span, EvidenceKind::BracketLiteral);
                                } else {
                                    self.emit_dynamic_access(span);
                                    for inner in &t.expressions {
                                        self.visit_expression(inner);
                                    }
                                }
                            }
                            // Constant folding for process.env[IDENTIFIER]
                            Expression::Identifier(id) => {
                                if let Some(val) = self.const_strings.get(id.name.as_str()).cloned()
                                {
                                    self.record_reference(&val, span, EvidenceKind::BracketLiteral);
                                } else {
                                    self.emit_dynamic_access(span);
                                    self.visit_expression(&m.expression);
                                }
                            }
                            // Pattern 7: Unresolved dynamic bracket access
                            _ => {
                                self.emit_dynamic_access(span);
                                self.visit_expression(&m.expression);
                            }
                        }
                        return;
                    }
                }
                MemberExpression::PrivateFieldExpression(_) => {}
            }
        }

        // Pattern 6: Optional Chaining (process.env?.FOO, Bun.env?.FOO, etc.)
        if let Expression::ChainExpression(chain) = expr {
            match &chain.expression {
                ChainElement::StaticMemberExpression(m) if self.is_env_source(&m.object) => {
                    let span = self.span_of(m.span);
                    self.record_reference(
                        m.property.name.as_str(),
                        span,
                        EvidenceKind::DirectMember,
                    );
                    return;
                }
                ChainElement::ComputedMemberExpression(m) if self.is_env_source(&m.object) => {
                    let span = self.span_of(m.span);
                    match &m.expression {
                        Expression::StringLiteral(s) => {
                            if let Some(val) = s.value.as_str() {
                                self.record_reference(val, span, EvidenceKind::BracketLiteral);
                            }
                        }
                        Expression::TemplateLiteral(t) => {
                            if t.expressions.is_empty() && t.quasis.len() == 1 {
                                let key = t.quasis[0].value.raw.as_str();
                                self.record_reference(key, span, EvidenceKind::BracketLiteral);
                            } else {
                                self.emit_dynamic_access(span);
                                for inner in &t.expressions {
                                    self.visit_expression(inner);
                                }
                            }
                        }
                        Expression::Identifier(id) => {
                            if let Some(val) = self.const_strings.get(id.name.as_str()).cloned() {
                                self.record_reference(&val, span, EvidenceKind::BracketLiteral);
                            } else {
                                self.emit_dynamic_access(span);
                                self.visit_expression(&m.expression);
                            }
                        }
                        _ => {
                            self.emit_dynamic_access(span);
                            self.visit_expression(&m.expression);
                        }
                    }
                    return;
                }
                _ => {}
            }
        }

        // Pattern 8: Deno.env.get("FOO")
        if let Expression::CallExpression(call) = expr {
            if is_deno_env_get(&call.callee) {
                if let Some(arg) = call.arguments.first() {
                    if let Some(e) = arg.as_expression() {
                        let span = self.span_of(call.span);
                        if let Expression::StringLiteral(s) = e {
                            if let Some(val) = s.value.as_str() {
                                self.record_reference(val, span, EvidenceKind::DirectMember);
                                return;
                            }
                        } else if let Expression::Identifier(id) = e {
                            if let Some(val) = self.const_strings.get(id.name.as_str()).cloned() {
                                self.record_reference(&val, span, EvidenceKind::DirectMember);
                                return;
                            }
                        }
                    }
                }
            }
        }

        // Object spread checks: { ...process.env } -> emit ENV012
        if let Expression::ObjectExpression(obj) = expr {
            for prop in &obj.properties {
                if let ObjectPropertyKind::SpreadProperty(spread) = prop {
                    if self.is_env_source(&spread.argument) {
                        let span = self.span_of(spread.span);
                        self.emit_escape(span, "Environment object spread into object literal");
                    } else {
                        self.visit_expression(&spread.argument);
                    }
                } else if let ObjectPropertyKind::ObjectProperty(p) = prop {
                    self.visit_expression(&p.value);
                }
            }
            return;
        }

        // Call expression: foo(process.env) -> emit ENV012
        if let Expression::CallExpression(call) = expr {
            self.visit_expression(&call.callee);
            for arg in &call.arguments {
                if let Some(e) = arg.as_expression() {
                    if self.is_env_source(e) {
                        let span = self.span_of(e.span());
                        self.emit_escape(
                            span,
                            "Environment object passed directly as function argument",
                        );
                    } else {
                        self.visit_expression(e);
                    }
                }
            }
            return;
        }

        // Generic expression traversals
        match expr {
            Expression::BinaryExpression(b) => {
                self.visit_expression(&b.left);
                self.visit_expression(&b.right);
            }
            Expression::LogicalExpression(b) => {
                self.visit_expression(&b.left);
                self.visit_expression(&b.right);
            }
            Expression::UnaryExpression(u) => {
                self.visit_expression(&u.argument);
            }
            Expression::ConditionalExpression(c) => {
                self.visit_expression(&c.test);
                self.visit_expression(&c.consequent);
                self.visit_expression(&c.alternate);
            }
            Expression::ArrayExpression(arr) => {
                for el in &arr.elements {
                    match el {
                        ArrayExpressionElement::SpreadElement(s) => {
                            if self.is_env_source(&s.argument) {
                                let span = self.span_of(s.span);
                                self.emit_escape(span, "Environment object spread into array");
                            } else {
                                self.visit_expression(&s.argument);
                            }
                        }
                        ArrayExpressionElement::Elision(_) => {}
                        _ => {
                            if let Some(e) = el.as_expression() {
                                if self.is_env_source(e) {
                                    let span = self.span_of(e.span());
                                    self.emit_escape(span, "Environment object placed into array");
                                } else {
                                    self.visit_expression(e);
                                }
                            }
                        }
                    }
                }
            }
            Expression::ParenthesizedExpression(p) => {
                self.visit_expression(&p.expression);
            }
            Expression::SequenceExpression(s) => {
                for inner in &s.expressions {
                    self.visit_expression(inner);
                }
            }
            Expression::FunctionExpression(func) => {
                if let Some(body) = &func.body {
                    for inner in &body.statements {
                        self.visit_statement(inner);
                    }
                }
            }
            Expression::ClassExpression(c) => {
                self.visit_class_elements(&c.body.body);
            }
            Expression::AwaitExpression(a) => {
                self.visit_expression(&a.argument);
            }
            Expression::YieldExpression(y) => {
                if let Some(arg) = &y.argument {
                    self.visit_expression(arg);
                }
            }
            Expression::NewExpression(n) => {
                self.visit_expression(&n.callee);
                for arg in &n.arguments {
                    if let Some(e) = arg.as_expression() {
                        self.visit_expression(e);
                    }
                }
            }
            _ => {
                if let Some(m) = expr.as_member_expression() {
                    match m {
                        MemberExpression::StaticMemberExpression(s) => {
                            self.visit_expression(&s.object);
                        }
                        MemberExpression::ComputedMemberExpression(c) => {
                            self.visit_expression(&c.object);
                            self.visit_expression(&c.expression);
                        }
                        MemberExpression::PrivateFieldExpression(p) => {
                            self.visit_expression(&p.object);
                        }
                    }
                }
            }
        }
    }
}

/// Returns `true` if this expression is `process.env`, `Bun.env`, or `import.meta.env`.
fn is_env_object(expr: &Expression) -> bool {
    match expr {
        Expression::StaticMemberExpression(mem) => {
            if mem.property.name == "env" {
                if let Expression::Identifier(id) = &mem.object {
                    return id.name == "process" || id.name == "Bun";
                }
                if let Expression::ImportMeta(_) = &mem.object {
                    return true;
                }
            }
            false
        }
        Expression::ChainExpression(chain) => {
            if let ChainElement::StaticMemberExpression(mem) = &chain.expression {
                if mem.property.name == "env" {
                    if let Expression::Identifier(id) = &mem.object {
                        return id.name == "process" || id.name == "Bun";
                    }
                    if let Expression::ImportMeta(_) = &mem.object {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Returns `true` if this expression is `Deno.env.get`.
fn is_deno_env_get(callee: &Expression) -> bool {
    if let Expression::StaticMemberExpression(mem) = callee {
        if mem.property.name == "get" {
            if let Expression::StaticMemberExpression(inner) = &mem.object {
                if inner.property.name == "env" {
                    if let Expression::Identifier(id) = &inner.object {
                        return id.name == "Deno";
                    }
                }
            }
        }
    }
    false
}
