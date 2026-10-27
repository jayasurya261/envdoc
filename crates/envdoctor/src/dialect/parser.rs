use crate::dialect::diagnostic::{DiagnosticCode, DialectDiagnostic, Severity};
use crate::model::contract::ContractAnnotations;
use crate::model::definition::Definition;
use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::span::Span;
use crate::model::value::ValueKind;
use crate::redaction::RawEnvValue;
use std::collections::{HashMap, HashSet};
use zeroize::Zeroizing;

/// Duplicate key resolution strategy in `.env` files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DuplicateKeyStrategy {
    /// The last definition overwrites earlier definitions (default dotenvy/Node dotenv behavior).
    #[default]
    LastWins,
    /// The first definition is retained; subsequent duplicates are ignored.
    FirstWins,
    /// Duplicate keys are treated as fatal errors.
    Error,
}

/// Options configuring dialect parsing.
#[derive(Debug, Clone, Default)]
pub struct DialectOptions {
    /// Strategy for handling duplicate variable definitions.
    pub duplicate_strategy: DuplicateKeyStrategy,
}

/// The structured output produced by parsing a `.env` or `.env.example` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOutput {
    /// Discovered environment variable definitions.
    pub definitions: Vec<Definition>,
    /// Diagnostics emitted during parsing.
    pub diagnostics: Vec<DialectDiagnostic>,
}

/// Parses `.env` or contract file content with default options.
pub fn parse(input: &str, path: impl Into<RelPath>) -> ParseOutput {
    parse_with_options(input, path, &DialectOptions::default())
}

/// Parses `.env` or contract file content with configurable options.
pub fn parse_with_options(
    input: &str,
    path: impl Into<RelPath>,
    options: &DialectOptions,
) -> ParseOutput {
    let rel_path = path.into();
    let mut parser = Parser::new(input, rel_path, options.clone());
    parser.parse_all()
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
    path: RelPath,
    options: DialectOptions,
    definitions: Vec<Definition>,
    diagnostics: Vec<DialectDiagnostic>,
    seen_keys: HashMap<String, usize>,
    var_dependencies: HashMap<String, Vec<String>>,
    pending_annotations: ContractAnnotations,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, path: RelPath, options: DialectOptions) -> Self {
        Self {
            bytes: source.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
            path,
            options,
            definitions: Vec::new(),
            diagnostics: Vec::new(),
            seen_keys: HashMap::new(),
            var_dependencies: HashMap::new(),
            pending_annotations: ContractAnnotations::default(),
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        if self.pos < self.bytes.len() {
            Some(self.bytes[self.pos])
        } else {
            None
        }
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        let target = self.pos + offset;
        if target < self.bytes.len() {
            Some(self.bytes[target])
        } else {
            None
        }
    }

    fn advance(&mut self) -> Option<u8> {
        if self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            self.pos += 1;
            if b == b'\r' {
                if self.pos < self.bytes.len() && self.bytes[self.pos] == b'\n' {
                    self.pos += 1;
                }
                self.line += 1;
                self.col = 1;
                Some(b'\n')
            } else if b == b'\n' {
                self.line += 1;
                self.col = 1;
                Some(b)
            } else {
                self.col += 1;
                Some(b)
            }
        } else {
            None
        }
    }

    fn skip_whitespace_inline(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_to_eol(&mut self) {
        while let Some(b) = self.peek() {
            if b == b'\n' || b == b'\r' {
                self.advance();
                break;
            }
            self.advance();
        }
    }

    fn parse_all(&mut self) -> ParseOutput {
        // Strip UTF-8 BOM if present at the start of file
        if self.bytes.starts_with(b"\xEF\xBB\xBF") {
            self.pos = 3;
        }

        while !self.is_eof() {
            self.skip_whitespace_inline();

            match self.peek() {
                None => break,
                Some(b'\n') | Some(b'\r') => {
                    self.advance();
                    self.pending_annotations = ContractAnnotations::default();
                }
                Some(b'#') => {
                    self.parse_comment_line();
                }
                Some(_) => {
                    self.parse_assignment_line();
                }
            }
        }

        // Structural cycle detection across variable expansion reference graph
        self.detect_variable_expansion_cycles();

        ParseOutput {
            definitions: std::mem::take(&mut self.definitions),
            diagnostics: std::mem::take(&mut self.diagnostics),
        }
    }

    fn parse_comment_line(&mut self) {
        let mut comment_content = String::new();

        while let Some(b) = self.peek() {
            if b == b'\n' || b == b'\r' {
                self.advance();
                break;
            }
            comment_content.push(b as char);
            self.advance();
        }

        self.pending_annotations
            .parse_comment_directive(&comment_content);
    }

    fn parse_assignment_line(&mut self) {
        let line_start_pos = self.pos;
        let line_start_line = self.line;
        let line_start_col = self.col;

        // Check for empty key: line starts with '='
        if self.peek() == Some(b'=') {
            let span = Span::new(
                line_start_line,
                line_start_col,
                line_start_pos as u32,
                (self.pos + 1) as u32,
            );
            self.diagnostics.push(DialectDiagnostic::new(
                DiagnosticCode::ENV005,
                "Malformed line: missing variable name before '='",
                span,
                None,
            ));
            self.skip_to_eol();
            self.pending_annotations = ContractAnnotations::default();
            return;
        }

        // Check for optional `export ` keyword
        if self.starts_with_keyword(b"export") {
            let after_export = self.pos + 6;
            if after_export < self.bytes.len()
                && (self.bytes[after_export] == b' ' || self.bytes[after_export] == b'\t')
            {
                for _ in 0..6 {
                    self.advance();
                }
                self.skip_whitespace_inline();
            }
        }

        // Parse key
        let key_start_line = self.line;
        let key_start_col = self.col;
        let key_start_pos = self.pos;

        let key_opt = self.parse_key();
        let key_str = match key_opt {
            Some(k) => k,
            None => {
                let span = Span::new(
                    line_start_line,
                    line_start_col,
                    line_start_pos as u32,
                    self.pos as u32,
                );
                self.diagnostics.push(DialectDiagnostic::new(
                    DiagnosticCode::ENV005,
                    "Malformed line: missing or invalid variable name",
                    span,
                    None,
                ));
                self.skip_to_eol();
                self.pending_annotations = ContractAnnotations::default();
                return;
            }
        };

        let env_name = EnvName::new(&key_str);
        if !env_name.is_valid_identifier() {
            let span = Span::new(
                key_start_line,
                key_start_col,
                key_start_pos as u32,
                self.pos as u32,
            );
            self.diagnostics.push(DialectDiagnostic::new(
                DiagnosticCode::ENV005,
                "Malformed line: invalid environment variable name",
                span,
                None,
            ));
            self.skip_to_eol();
            self.pending_annotations = ContractAnnotations::default();
            return;
        }

        self.skip_whitespace_inline();

        // Expect '=' delimiter
        if self.peek() != Some(b'=') {
            let span = Span::new(
                line_start_line,
                line_start_col,
                line_start_pos as u32,
                self.pos as u32,
            );
            self.diagnostics.push(DialectDiagnostic::new(
                DiagnosticCode::ENV005,
                "Malformed line: expected '=' assignment delimiter",
                span,
                None,
            ));
            self.skip_to_eol();
            self.pending_annotations = ContractAnnotations::default();
            return;
        }

        // Consume '='
        self.advance();
        self.skip_whitespace_inline();

        // Parse value
        let value_res = self.parse_value(&env_name);
        let (value_kind, has_unsupported, var_deps) = match value_res {
            Some(v) => v,
            None => {
                self.pending_annotations = ContractAnnotations::default();
                return;
            }
        };

        let line_end_pos = self.pos;
        let total_span = Span::new(
            line_start_line,
            line_start_col,
            line_start_pos as u32,
            line_end_pos as u32,
        );

        if has_unsupported {
            self.diagnostics.push(DialectDiagnostic::new(
                DiagnosticCode::ENV006,
                format!(
                    "Variable '{}' contains unsupported shell syntax (command substitution or subshell)",
                    key_str
                ),
                total_span,
                Some(env_name.clone()),
            ));
        }

        if !var_deps.is_empty() {
            self.var_dependencies
                .entry(key_str.clone())
                .or_default()
                .extend(var_deps);
        }

        let annotations = std::mem::take(&mut self.pending_annotations);

        let def = Definition::with_annotations(
            env_name.clone(),
            self.path.clone(),
            total_span,
            value_kind,
            annotations,
        );

        // Track duplicate keys based on configured strategy
        if let Some(&existing_idx) = self.seen_keys.get(key_str.as_str()) {
            match self.options.duplicate_strategy {
                DuplicateKeyStrategy::LastWins => {
                    self.diagnostics.push(DialectDiagnostic::new(
                        DiagnosticCode::ENV004,
                        format!(
                            "Duplicate variable '{}' defined; last definition wins",
                            key_str
                        ),
                        total_span,
                        Some(env_name),
                    ));
                    self.definitions[existing_idx] = def;
                }
                DuplicateKeyStrategy::FirstWins => {
                    self.diagnostics.push(DialectDiagnostic::new(
                        DiagnosticCode::ENV004,
                        format!(
                            "Duplicate variable '{}' defined; first definition retained",
                            key_str
                        ),
                        total_span,
                        Some(env_name),
                    ));
                }
                DuplicateKeyStrategy::Error => {
                    self.diagnostics.push(
                        DialectDiagnostic::new(
                            DiagnosticCode::ENV004,
                            format!(
                                "Duplicate variable '{}' defined (prohibited by duplicate strategy)",
                                key_str
                            ),
                            total_span,
                            Some(env_name),
                        )
                        .with_severity(Severity::Error),
                    );
                    self.definitions[existing_idx] = def;
                }
            }
        } else {
            self.seen_keys.insert(key_str, self.definitions.len());
            self.definitions.push(def);
        }
    }

    fn starts_with_keyword(&self, kw: &[u8]) -> bool {
        if self.pos + kw.len() > self.bytes.len() {
            return false;
        }
        &self.bytes[self.pos..self.pos + kw.len()] == kw
    }

    fn parse_key(&mut self) -> Option<String> {
        if self.is_eof() {
            return None;
        }

        if self.peek() == Some(b'"') || self.peek() == Some(b'\'') {
            let quote = self.advance().unwrap();
            let mut key = String::new();
            while let Some(b) = self.peek() {
                if b == quote {
                    self.advance();
                    return Some(key);
                } else if b == b'\n' || b == b'\r' {
                    return None;
                } else {
                    key.push(b as char);
                    self.advance();
                }
            }
            return None;
        }

        let mut key = String::new();
        while let Some(b) = self.peek() {
            match b {
                b'=' | b' ' | b'\t' | b'\r' | b'\n' | b'#' => break,
                _ => {
                    key.push(b as char);
                    self.advance();
                }
            }
        }

        if key.is_empty() {
            None
        } else {
            Some(key)
        }
    }

    fn parse_value(&mut self, env_name: &EnvName) -> Option<(ValueKind, bool, Vec<String>)> {
        match self.peek() {
            None | Some(b'\n') | Some(b'\r') => {
                if self.peek() == Some(b'\n') || self.peek() == Some(b'\r') {
                    self.advance();
                }
                Some((ValueKind::Empty, false, Vec::new()))
            }
            Some(b'"') => self.parse_double_quoted_value(env_name),
            Some(b'\'') => self.parse_single_quoted_value(env_name),
            Some(_) => Some(self.parse_unquoted_value(env_name)),
        }
    }

    fn parse_double_quoted_value(
        &mut self,
        env_name: &EnvName,
    ) -> Option<(ValueKind, bool, Vec<String>)> {
        let quote_start_line = self.line;
        let quote_start_col = self.col;
        let quote_start_pos = self.pos;

        self.advance(); // consume opening "
        let mut raw_val = Zeroizing::new(Vec::new());
        let mut is_multiline = false;
        let mut has_unsupported = false;
        let mut closed = false;

        while let Some(b) = self.peek() {
            if b == b'"' {
                self.advance(); // consume closing "
                closed = true;
                break;
            } else if b == b'\\' {
                self.advance(); // consume \
                if let Some(next) = self.peek() {
                    match next {
                        // Line continuation: \ followed by newline
                        b'\n' | b'\r' => {
                            self.advance();
                            is_multiline = true;
                        }
                        b'n' => {
                            raw_val.push(b'\n');
                            self.advance();
                        }
                        b'r' => {
                            raw_val.push(b'\r');
                            self.advance();
                        }
                        b't' => {
                            raw_val.push(b'\t');
                            self.advance();
                        }
                        b'\\' => {
                            raw_val.push(b'\\');
                            self.advance();
                        }
                        b'"' => {
                            raw_val.push(b'"');
                            self.advance();
                        }
                        b'$' => {
                            raw_val.push(b'$');
                            self.advance();
                        }
                        _ => {
                            raw_val.push(b'\\');
                            raw_val.push(next);
                            self.advance();
                        }
                    }
                } else {
                    raw_val.push(b'\\');
                }
            } else if b == b'$' {
                // Command substitution like $(cmd) or `cmd` is unsupported
                if self.is_command_substitution() {
                    has_unsupported = true;
                }
                raw_val.push(b);
                self.advance();
            } else {
                if b == b'\n' || b == b'\r' {
                    is_multiline = true;
                }
                raw_val.push(b);
                self.advance();
            }
        }

        if !closed {
            let span = Span::new(
                quote_start_line,
                quote_start_col,
                quote_start_pos as u32,
                self.pos as u32,
            );
            self.diagnostics.push(DialectDiagnostic::new(
                DiagnosticCode::ENV005,
                format!("Unclosed double-quoted value for '{}'", env_name),
                span,
                Some(env_name.clone()),
            ));
            return None;
        }

        if !self.skip_trailing_comment_after_quote(env_name) {
            return None;
        }

        let utf8_val = std::str::from_utf8(&raw_val).unwrap_or("");
        let var_deps = self.extract_expansions(utf8_val, env_name);
        let raw = RawEnvValue::from_bytes(&raw_val, is_multiline);
        Some((raw.into_kind(), has_unsupported, var_deps))
    }

    fn parse_single_quoted_value(
        &mut self,
        env_name: &EnvName,
    ) -> Option<(ValueKind, bool, Vec<String>)> {
        let quote_start_line = self.line;
        let quote_start_col = self.col;
        let quote_start_pos = self.pos;

        self.advance(); // consume opening '
        let mut raw_val = Zeroizing::new(Vec::new());
        let mut is_multiline = false;
        let mut closed = false;

        while let Some(b) = self.peek() {
            if b == b'\'' {
                self.advance(); // consume closing '
                closed = true;
                break;
            } else {
                if b == b'\n' || b == b'\r' {
                    is_multiline = true;
                }
                raw_val.push(b);
                self.advance();
            }
        }

        if !closed {
            let span = Span::new(
                quote_start_line,
                quote_start_col,
                quote_start_pos as u32,
                self.pos as u32,
            );
            self.diagnostics.push(DialectDiagnostic::new(
                DiagnosticCode::ENV005,
                format!("Unclosed single-quoted value for '{}'", env_name),
                span,
                Some(env_name.clone()),
            ));
            return None;
        }

        if !self.skip_trailing_comment_after_quote(env_name) {
            return None;
        }

        // Single-quoted values are verbatim: no expansions or escapes
        let raw = RawEnvValue::from_bytes(&raw_val, is_multiline);
        Some((raw.into_kind(), false, Vec::new()))
    }

    fn parse_unquoted_value(&mut self, env_name: &EnvName) -> (ValueKind, bool, Vec<String>) {
        let mut raw_val = Zeroizing::new(Vec::new());
        let mut has_unsupported = false;

        while let Some(b) = self.peek() {
            if b == b'\\' {
                // Line continuation: \ followed immediately by newline
                if self.peek_at(1) == Some(b'\n') || self.peek_at(1) == Some(b'\r') {
                    self.advance(); // consume \
                    self.advance(); // consume newline
                    continue;
                }
                raw_val.push(b);
                self.advance();
            } else if b == b'\n' || b == b'\r' {
                self.advance();
                break;
            } else if b == b'#' {
                // Inline comments only if preceded by whitespace or at beginning
                if raw_val.is_empty()
                    || raw_val.last() == Some(&b' ')
                    || raw_val.last() == Some(&b'\t')
                {
                    self.skip_to_eol();
                    break;
                } else {
                    raw_val.push(b);
                    self.advance();
                }
            } else {
                if b == b'$' && self.is_command_substitution() {
                    has_unsupported = true;
                }
                raw_val.push(b);
                self.advance();
            }
        }

        while let Some(&last) = raw_val.last() {
            if last == b' ' || last == b'\t' {
                raw_val.pop();
            } else {
                break;
            }
        }

        let utf8_val = std::str::from_utf8(&raw_val).unwrap_or("");
        let var_deps = self.extract_expansions(utf8_val, env_name);
        let raw = RawEnvValue::from_bytes(&raw_val, false);
        (raw.into_kind(), has_unsupported, var_deps)
    }

    fn skip_trailing_comment_after_quote(&mut self, env_name: &EnvName) -> bool {
        self.skip_whitespace_inline();
        match self.peek() {
            None => true,
            Some(b'\n') | Some(b'\r') => {
                self.advance();
                true
            }
            Some(b'#') => {
                self.skip_to_eol();
                true
            }
            Some(_) => {
                let start_pos = self.pos;
                let start_line = self.line;
                let start_col = self.col;
                self.skip_to_eol();
                let span = Span::new(start_line, start_col, start_pos as u32, self.pos as u32);
                self.diagnostics.push(DialectDiagnostic::new(
                    DiagnosticCode::ENV005,
                    format!(
                        "Unexpected trailing characters after closing quote for '{}'",
                        env_name
                    ),
                    span,
                    Some(env_name.clone()),
                ));
                false
            }
        }
    }

    fn is_command_substitution(&self) -> bool {
        // Look ahead for $(cmd)
        if let Some(next) = self.peek_at(1) {
            next == b'('
        } else {
            false
        }
    }

    fn extract_expansions(&mut self, s: &str, env_name: &EnvName) -> Vec<String> {
        let mut deps = Vec::new();
        let mut remainder = s;

        while let Some(start) = remainder.find("${") {
            let after_start = &remainder[start + 2..];
            if let Some(end) = after_start.find('}') {
                let inner = &after_start[..end];
                if let Some(dash_idx) = inner.find(":-") {
                    let var = inner[..dash_idx].trim();
                    deps.push(var.to_string());
                } else if let Some(q_idx) = inner.find(":?") {
                    let var = inner[..q_idx].trim();
                    let err_msg = &inner[q_idx + 2..];
                    deps.push(var.to_string());
                    // Check if variable is defined in seen keys
                    if !self.seen_keys.contains_key(var) {
                        self.diagnostics.push(DialectDiagnostic::new(
                            DiagnosticCode::ENV006,
                            format!(
                                "Variable expansion error for '{}': required variable '{}' is missing ({})",
                                env_name, var, err_msg
                            ),
                            Span::point(self.line, self.col, self.pos as u32),
                            Some(env_name.clone()),
                        ));
                    }
                } else {
                    let var = inner.trim();
                    deps.push(var.to_string());
                }
                remainder = &after_start[end + 1..];
            } else {
                break;
            }
        }

        deps
    }

    fn detect_variable_expansion_cycles(&mut self) {
        let deps = self.var_dependencies.clone();
        let mut visited = HashSet::new();
        let mut rec_stack = Vec::new();

        for key in deps.keys() {
            if !visited.contains(key) {
                self.dfs_cycle(key, &deps, &mut visited, &mut rec_stack);
            }
        }
    }

    fn dfs_cycle(
        &mut self,
        current: &str,
        deps: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        rec_stack: &mut Vec<String>,
    ) {
        visited.insert(current.to_string());
        rec_stack.push(current.to_string());

        if let Some(neighbors) = deps.get(current) {
            for next in neighbors {
                if let Some(pos) = rec_stack.iter().position(|k| k == next) {
                    let cycle_path = rec_stack[pos..].join(" -> ") + " -> " + next;
                    self.diagnostics.push(DialectDiagnostic::new(
                        DiagnosticCode::ENV006,
                        format!("Cyclic variable expansion detected: {}", cycle_path),
                        Span::point(1, 1, 0),
                        Some(EnvName::new(current)),
                    ));
                } else if !visited.contains(next) {
                    self.dfs_cycle(next, deps, visited, rec_stack);
                }
            }
        }

        rec_stack.pop();
    }
}
