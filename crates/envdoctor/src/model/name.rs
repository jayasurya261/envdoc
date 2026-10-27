use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;

/// A strongly-typed environment variable name identifier.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnvName(String);

impl EnvName {
    /// Constructs a new `EnvName`.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the environment variable name as a string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validates whether the name complies with standard env identifier rules:
    /// `[A-Za-z_][A-Za-z0-9_.-]*` (supporting dots and dashes for Docker and tooling).
    pub fn is_valid_identifier(&self) -> bool {
        let bytes = self.0.as_bytes();
        if bytes.is_empty() {
            return false;
        }

        // First character must be [A-Za-z_]
        match bytes[0] {
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {}
            _ => return false,
        }

        // Remaining characters can include [A-Za-z0-9_.-]
        for &b in &bytes[1..] {
            match b {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'.' | b'-' => {}
                _ => return false,
            }
        }

        true
    }
}

impl Deref for EnvName {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for EnvName {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for EnvName {
    #[inline]
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for EnvName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EnvName({:?})", self.0)
    }
}

impl fmt::Display for EnvName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for EnvName {
    #[inline]
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for EnvName {
    #[inline]
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl Serialize for EnvName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for EnvName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::new(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_identifiers() {
        assert!(EnvName::new("DATABASE_URL").is_valid_identifier());
        assert!(EnvName::new("_SECRET_KEY_123").is_valid_identifier());
        assert!(EnvName::new("PORT").is_valid_identifier());
        assert!(EnvName::new("a").is_valid_identifier());
        assert!(EnvName::new("FOO-BAR").is_valid_identifier());
        assert!(EnvName::new("FOO.BAR").is_valid_identifier());
    }

    #[test]
    fn test_invalid_identifiers() {
        assert!(!EnvName::new("").is_valid_identifier());
        assert!(!EnvName::new("123PORT").is_valid_identifier());
        assert!(!EnvName::new("FOO BAR").is_valid_identifier());
        assert!(!EnvName::new("FOO@BAR").is_valid_identifier());
    }
}
