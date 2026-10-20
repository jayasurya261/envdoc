use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Borrow;
use std::fmt;
use std::path::{Path, PathBuf};

/// A normalized, workspace-relative path.
///
/// Invariants:
/// - Path separators are ALWAYS normalized to forward slashes (`/`), even on Windows.
/// - Leading `./` prefixes are stripped.
/// - Guarantees deterministic serialization and sorting across all operating systems.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelPath(String);

impl RelPath {
    /// Creates a new `RelPath` from a path or path string, normalizing separators and prefixes.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        let path_str = path.to_string_lossy();
        let normalized = Self::normalize(&path_str);
        Self(normalized)
    }

    /// Normalizes path string by converting backslashes to slashes and removing redundant leading `./`.
    fn normalize(s: &str) -> String {
        let mut replaced = s.replace('\\', "/");
        while let Some(stripped) = replaced.strip_prefix("./") {
            replaced = stripped.to_string();
        }
        if replaced.is_empty() {
            ".".to_string()
        } else {
            replaced
        }
    }

    /// Returns the normalized path as a string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the path as a standard `Path`.
    #[inline]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    /// Returns `true` if the path is empty or represents current directory `.`.
    #[inline]
    pub fn is_root(&self) -> bool {
        self.0 == "." || self.0.is_empty()
    }
}

impl fmt::Debug for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RelPath({:?})", self.0)
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for RelPath {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Path> for RelPath {
    #[inline]
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl Borrow<str> for RelPath {
    #[inline]
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<&str> for RelPath {
    #[inline]
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for RelPath {
    #[inline]
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&Path> for RelPath {
    #[inline]
    fn from(p: &Path) -> Self {
        Self::new(p)
    }
}

impl From<PathBuf> for RelPath {
    #[inline]
    fn from(p: PathBuf) -> Self {
        Self::new(p)
    }
}

impl Serialize for RelPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RelPath {
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
    fn test_normalization() {
        assert_eq!(RelPath::new("src\\foo\\bar.rs").as_str(), "src/foo/bar.rs");
        assert_eq!(RelPath::new("./src/foo.ts").as_str(), "src/foo.ts");
        assert_eq!(RelPath::new(".\\src\\foo.ts").as_str(), "src/foo.ts");
        assert_eq!(RelPath::new("").as_str(), ".");
        assert_eq!(RelPath::new(".").as_str(), ".");
    }

    #[test]
    fn test_serialization() {
        let path = RelPath::new("src/main.rs");
        let json = serde_json::to_string(&path).unwrap();
        assert_eq!(json, "\"src/main.rs\"");
        let deserialized: RelPath = serde_json::from_str(&json).unwrap();
        assert_eq!(path, deserialized);
    }
}

