use serde::{Deserialize, Serialize};

/// Supported contract types for `.env.example` annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContractType {
    Url,
    Int,
    Port,
    Bool,
    Email,
    String,
}

impl ContractType {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "url" | "uri" => Some(Self::Url),
            "int" | "integer" | "number" => Some(Self::Int),
            "port" => Some(Self::Port),
            "bool" | "boolean" => Some(Self::Bool),
            "email" => Some(Self::Email),
            "str" | "string" => Some(Self::String),
            _ => None,
        }
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::Int => "int",
            Self::Port => "port",
            Self::Bool => "bool",
            Self::Email => "email",
            Self::String => "string",
        }
    }

    /// Validates a raw string value against this type.
    ///
    /// INVARIANT: Never returns the value itself in error messages to guarantee zero leakage!
    pub fn validate(&self, val: &str) -> Result<(), &'static str> {
        let trimmed = val.trim();
        match self {
            Self::Url => {
                if trimmed.contains("://") && !trimmed.starts_with("://") {
                    Ok(())
                } else {
                    Err("expected valid URL with scheme (e.g. https://, postgres://)")
                }
            }
            Self::Int => {
                if trimmed.parse::<i64>().is_ok() {
                    Ok(())
                } else {
                    Err("expected integer value")
                }
            }
            Self::Port => match trimmed.parse::<u16>() {
                Ok(p) if p > 0 => Ok(()),
                _ => Err("expected port number between 1 and 65535"),
            },
            Self::Bool => {
                let lower = trimmed.to_ascii_lowercase();
                if matches!(lower.as_str(), "true" | "false" | "1" | "0" | "yes" | "no") {
                    Ok(())
                } else {
                    Err("expected boolean value ('true', 'false', '1', '0')")
                }
            }
            Self::Email => {
                if let Some(at_idx) = trimmed.find('@') {
                    if at_idx > 0 && trimmed[at_idx..].contains('.') && !trimmed.ends_with('.') {
                        return Ok(());
                    }
                }
                Err("expected valid email address")
            }
            Self::String => Ok(()),
        }
    }
}

/// Metadata and typed annotations extracted from comments preceding or adjacent
/// to a variable definition in `.env.example`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub struct ContractAnnotations {
    /// Whether marked with `# @optional`
    pub is_optional: bool,
    /// Explicit type annotation `# @type <type>`
    pub expected_type: Option<ContractType>,
    /// Enumerated allowed values `# @enum <val1|val2|...>`
    pub enum_values: Option<Vec<String>>,
    /// Marked as sensitive secret `# @secret`
    pub is_secret: bool,
    /// Marked as explicitly public `# @public`
    pub is_public: bool,
    /// Metadata version `# @since <ver>`
    pub since_version: Option<String>,
}

impl ContractAnnotations {
    /// Parse a single comment line and update annotations.
    pub fn parse_comment_directive(&mut self, line: &str) {
        let trimmed = line.trim();
        let comment_text = trimmed.strip_prefix('#').unwrap_or(trimmed).trim();

        if !comment_text.starts_with('@') {
            return;
        }

        let mut parts = comment_text.split_whitespace();
        let raw_directive = parts.next().unwrap_or("");
        let directive = raw_directive.trim_end_matches(':');
        let rest = comment_text[raw_directive.len()..].trim();

        match directive {
            "@optional" => {
                self.is_optional = true;
            }
            "@secret" => {
                self.is_secret = true;
            }
            "@public" => {
                self.is_public = true;
            }
            "@type" => {
                if let Some(t) = ContractType::parse(rest) {
                    self.expected_type = Some(t);
                }
            }
            "@enum" => {
                let values: Vec<String> = rest
                    .split('|')
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect();
                if !values.is_empty() {
                    self.enum_values = Some(values);
                }
            }
            "@since" if !rest.is_empty() => {
                self.since_version = Some(rest.to_string());
            }
            _ => {}
        }
    }

    /// Validates a raw value against any type or enum annotations.
    ///
    /// INVARIANT: Pass or fail only; never exposes or echoes the secret value!
    pub fn validate_value(&self, val: &str) -> Result<(), &'static str> {
        if let Some(expected_type) = &self.expected_type {
            expected_type.validate(val)?;
        }
        if let Some(allowed) = &self.enum_values {
            let trimmed = val.trim();
            if !allowed.iter().any(|v| v == trimmed) {
                return Err("value does not match any of the allowed enum variants");
            }
        }
        Ok(())
    }
}
