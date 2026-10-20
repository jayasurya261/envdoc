use crate::dialect::diagnostic::{DiagnosticCode, DialectDiagnostic};
use crate::model::definition::Definition;
use crate::model::name::EnvName;
use crate::model::path::RelPath;
use crate::model::span::Span;
use crate::model::value::ValueKind;
use crate::redaction::RawEnvValue;
use std::collections::HashMap;

/// The structured output produced by parsing a `.env` or `.env.example` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOutput {
    /// Discovered environment variable definitions.
    pub definitions: Vec<Definition>,
    /// Diagnostics (syntax errors, warnings, unsupported constructs, duplicates) emitted during parsing.
    pub diagnostics: Vec<DialectDiagnostic>,
}

/// Parses `.env` or contract file content from in-memory UTF-8 text into definitions and diagnostics.
///
/// Invariant: Pure in-memory execution with zero file I/O.
pub fn parse(input: &str, path: impl Into<RelPath>) -> ParseOutput {
    let rel_path = path.into();
    let mut parser = Parser::new(input, rel_path);
    parser.parse_all()
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
    path: RelPath,
    definitions: Vec<Definition>,
    diagnostics: Vec<DialectDiagnostic>,
    seen_keys: HashMap<String, usize>,
    is_optional_pending: bool,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, path: RelPath) -> Self {
        Self {
            bytes: source.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
            path,
            definitions: Vec::new(),
            diagnostics: Vec::new(),
            seen_keys: HashMap::new(),
            is_optional_pending: false,
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
            if b == b'\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            Some(b)
        } else {
            None
        }
    }

    fn skip_whitespace_inline(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_to_eol(&mut self) {
        while let Some(b) = self.peek() {
            if b == b'\n' {
                self.advance();
                break;
            }
            self.advance();
        }
    }

    fn parse_all(&mut self) -> ParseOutput {
        while !self.is_eof() {
            self.skip_whitespace_inline();

            match self.peek() {
                None => break,
                Some(b'\n') => {
                    self.advance();
                    // An empty line resets pending doc annotations
                    self.is_optional_pending = false;
                }
                Some(b'#') => {
                    self.parse_comment_line();
                }
                Some(_) => {
                    self.parse_assignment_line();
                }
            }
        }

        ParseOutput {
            definitions: std::mem::take(&mut self.definitions),
            diagnostics: std::mem::take(&mut self.diagnostics),
        }
    }

    fn parse_comment_line(&mut self) {
        let mut comment_content = String::new();

        while let Some(b) = self.peek() {
            if b == b'\n' {
                self.advance();
                break;
            }
            comment_content.push(b as char);
            self.advance();
        }

        let trimmed = comment_content.trim();
        // Check for `# @optional` doc-comment annotation
        if trimmed == "# @optional"
            || trimmed.starts_with("# @optional ")
            || trimmed.starts_with("# @optional:")
            || trimmed == "#@optional"
            || trimmed.starts_with("#@optional ")
            || trimmed.starts_with("#@optional:")
        {
            self.is_optional_pending = true;
        }
    }

    fn parse_assignment_line(&mut self) {
        let line_start_pos = self.pos;
        let line_start_line = self.line;
        let line_start_col = self.col;

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
                // Malformed line
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
                self.is_optional_pending = false;
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
                format!(
                    "Invalid environment variable name '{}': must match [A-Za-z_][A-Za-z0-9_]*",
                    key_str
                ),
                span,
                Some(env_name.clone()),
            ));
            self.skip_to_eol();
            self.is_optional_pending = false;
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
                format!("Expected '=' after variable name '{}'", key_str),
                span,
                Some(env_name),
            ));
            self.skip_to_eol();
            self.is_optional_pending = false;
            return;
        }

        // Consume '='
        self.advance();
        self.skip_whitespace_inline();

        // Parse value
        let value_res = self.parse_value(&env_name);
        let (value_kind, has_unsupported) = match value_res {
            Some(v) => v,
            None => {
                self.is_optional_pending = false;
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
                    "Variable '{}' contains unsupported shell syntax (variable expansion or command substitution)",
                    key_str
                ),
                total_span,
                Some(env_name.clone()),
            ));
        }

        let is_optional = self.is_optional_pending;
        self.is_optional_pending = false;

        let def = Definition::new(
            env_name.clone(),
            self.path.clone(),
            total_span,
            value_kind,
            is_optional,
        );

        // Track duplicate keys
        if let Some(&existing_idx) = self.seen_keys.get(key_str.as_str()) {
            self.diagnostics.push(DialectDiagnostic::new(
                DiagnosticCode::ENV004,
                format!(
                    "Duplicate variable '{}' defined; last definition wins",
                    key_str
                ),
                total_span,
                Some(env_name),
            ));
            // Last definition wins: update the recorded definition in place
            self.definitions[existing_idx] = def;
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

        // Check for quoted key
        if self.peek() == Some(b'"') || self.peek() == Some(b'\'') {
            let quote = self.advance().unwrap();
            let mut key = String::new();
            while let Some(b) = self.peek() {
                if b == quote {
                    self.advance();
                    return Some(key);
                } else if b == b'\n' {
                    return None; // Unclosed quote before newline
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

    fn parse_value(&mut self, env_name: &EnvName) -> Option<(ValueKind, bool)> {
        match self.peek() {
            None | Some(b'\n') => {
                if self.peek() == Some(b'\n') {
                    self.advance();
                }
                Some((ValueKind::Empty, false))
            }
            Some(b'"') => self.parse_double_quoted_value(env_name),
            Some(b'\'') => self.parse_single_quoted_value(env_name),
            Some(_) => Some(self.parse_unquoted_value(env_name)),
        }
    }

    fn parse_double_quoted_value(&mut self, env_name: &EnvName) -> Option<(ValueKind, bool)> {
        let quote_start_line = self.line;
        let quote_start_col = self.col;
        let quote_start_pos = self.pos;

        self.advance(); // consume opening "
        let mut raw_val = String::new();
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
                        b'n' => {
                            raw_val.push('\n');
                            self.advance();
                        }
                        b'r' => {
                            raw_val.push('\r');
                            self.advance();
                        }
                        b't' => {
                            raw_val.push('\t');
                            self.advance();
                        }
                        b'\\' => {
                            raw_val.push('\\');
                            self.advance();
                        }
                        b'"' => {
                            raw_val.push('"');
                            self.advance();
                        }
                        b'$' => {
                            raw_val.push('$');
                            self.advance();
                        }
                        _ => {
                            raw_val.push('\\');
                            raw_val.push(next as char);
                            self.advance();
                        }
                    }
                } else {
                    raw_val.push('\\');
                }
            } else if b == b'$' {
                // Check for unsupported expansion
                if self.is_unsupported_expansion() {
                    has_unsupported = true;
                }
                raw_val.push(b as char);
                self.advance();
            } else {
                if b == b'\n' {
                    is_multiline = true;
                }
                raw_val.push(b as char);
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

        // Consume any inline comment or trailing spaces after closing quote
        if !self.skip_trailing_comment_after_quote(env_name) {
            return None;
        }

        let raw = RawEnvValue::new(raw_val, is_multiline);
        Some((raw.into_kind(), has_unsupported))
    }

    fn parse_single_quoted_value(&mut self, env_name: &EnvName) -> Option<(ValueKind, bool)> {
        let quote_start_line = self.line;
        let quote_start_col = self.col;
        let quote_start_pos = self.pos;

        self.advance(); // consume opening '
        let mut raw_val = String::new();
        let mut is_multiline = false;
        let mut closed = false;

        while let Some(b) = self.peek() {
            if b == b'\'' {
                self.advance(); // consume closing '
                closed = true;
                break;
            } else {
                if b == b'\n' {
                    is_multiline = true;
                }
                raw_val.push(b as char);
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

        // Consume any inline comment or trailing spaces after closing quote
        if !self.skip_trailing_comment_after_quote(env_name) {
            return None;
        }

        // Single quoted values are always literal: no expansions occur
        let raw = RawEnvValue::new(raw_val, is_multiline);
        Some((raw.into_kind(), false))
    }

    fn parse_unquoted_value(&mut self, _env_name: &EnvName) -> (ValueKind, bool) {
        let mut raw_val = String::new();
        let mut has_unsupported = false;

        while let Some(b) = self.peek() {
            if b == b'\n' {
                self.advance();
                break;
            } else if b == b'#' {
                // An inline comment occurs if '#' is preceded by whitespace or is at start of value
                if raw_val.is_empty() || raw_val.ends_with(' ') || raw_val.ends_with('\t') {
                    // Rest of line is inline comment
                    self.skip_to_eol();
                    break;
                } else {
                    raw_val.push('#');
                    self.advance();
                }
            } else if b == b'$' {
                if self.is_unsupported_expansion() {
                    has_unsupported = true;
                }
                raw_val.push('$');
                self.advance();
            } else if b == b'`' {
                // Command substitution with backticks
                has_unsupported = true;
                raw_val.push('`');
                self.advance();
            } else {
                raw_val.push(b as char);
                self.advance();
            }
        }

        let trimmed = raw_val.trim_end_matches([' ', '\t', '\r']);
        let raw = RawEnvValue::new(trimmed, false);
        (raw.into_kind(), has_unsupported)
    }

    fn skip_trailing_comment_after_quote(&mut self, env_name: &EnvName) -> bool {
        self.skip_whitespace_inline();
        match self.peek() {
            None => true,
            Some(b'\n') => {
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

    fn is_unsupported_expansion(&self) -> bool {
        // Look ahead for $VAR, ${VAR}, $(cmd)
        if let Some(next) = self.peek_at(1) {
            matches!(next, b'(' | b'{' | b'a'..=b'z' | b'A'..=b'Z' | b'_')
        } else {
            false
        }
    }
}
