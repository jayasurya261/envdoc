//! Incremental scan cache keyed by file content hash.
//!
//! Stores extracted references and diagnostics in `.envdoctor-cache` across runs,
//! enabling instant no-op re-runs and rapid incremental watch mode.
//! Keyed using cryptographic BLAKE3 hashes to eliminate collision vulnerabilities in CI.

#![allow(clippy::disallowed_methods)]

use crate::diagnostics::diagnostic::Diagnostic;
use crate::model::reference::Reference;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Cache schema version to prevent format drift.
pub const CACHE_VERSION: u32 = 2;

/// Default name of the cache file stored at workspace root.
pub const DEFAULT_CACHE_FILE: &str = ".envdoctor-cache";

/// Computes a cryptographically secure, collision-resistant 256-bit BLAKE3 hash across file bytes.
pub fn compute_content_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Computes a cryptographic BLAKE3 hash of engine configuration parameters.
pub fn compute_config_hash(
    max_file_size: u64,
    max_line_length: usize,
    max_total_bytes: u64,
    max_diagnostics: usize,
    duplicate_strategy: u8,
    languages: &[String],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&max_file_size.to_le_bytes());
    hasher.update(&(max_line_length as u64).to_le_bytes());
    hasher.update(&max_total_bytes.to_le_bytes());
    hasher.update(&(max_diagnostics as u64).to_le_bytes());
    hasher.update(&[duplicate_strategy]);
    for lang in languages {
        hasher.update(lang.as_bytes());
        hasher.update(b",");
    }
    hasher.finalize().to_hex().to_string()
}

/// Cache entry stored for an individual source file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedEntry {
    /// 256-bit BLAKE3 content hash of the file bytes.
    pub content_hash: String,
    /// File size in bytes.
    pub file_size: u64,
    /// Discovered references within the file.
    pub references: Vec<Reference>,
    /// Diagnostics produced while scanning the file.
    pub diagnostics: Vec<Diagnostic>,
}

/// Persistent cache tracking scan results across files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanCache {
    /// Format version.
    pub version: u32,
    /// Scanner package version.
    pub tool_version: String,
    /// Cryptographic hash of engine configuration options.
    pub config_hash: String,
    /// Active language set.
    pub languages: String,
    /// Map from relative file path string to cached scan results.
    pub entries: HashMap<String, CachedEntry>,
}

impl Default for ScanCache {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            config_hash: String::new(),
            languages: String::new(),
            entries: HashMap::new(),
        }
    }
}

impl ScanCache {
    /// Creates a new empty scan cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempts to load and deserialize a cache file from disk.
    /// Returns `None` if the file does not exist, has an outdated version, or is corrupted.
    pub fn load(path: &Path) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        let cache: Self = serde_json::from_str(&content).ok()?;
        if cache.version != CACHE_VERSION {
            return None;
        }
        Some(cache)
    }

    /// Loads cache from disk verifying that schema version, tool version, config hash,
    /// and enabled language set match exactly.
    pub fn load_matching(
        path: &Path,
        expected_config_hash: &str,
        expected_languages: &str,
    ) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        let cache: Self = serde_json::from_str(&content).ok()?;
        if cache.version != CACHE_VERSION
            || cache.tool_version != env!("CARGO_PKG_VERSION")
            || cache.config_hash != expected_config_hash
            || cache.languages != expected_languages
        {
            return None;
        }
        Some(cache)
    }

    /// Serializes and writes this cache to disk.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, json)
    }

    /// Retrieves cached results if the file's current content hash matches the cached entry.
    pub fn get(&self, rel_path: &str, current_hash: &str) -> Option<(&[Reference], &[Diagnostic])> {
        let entry = self.entries.get(rel_path)?;
        if entry.content_hash == current_hash {
            Some((&entry.references, &entry.diagnostics))
        } else {
            None
        }
    }

    /// Inserts or updates scan results for a file in the cache.
    pub fn insert(
        &mut self,
        rel_path: String,
        content_hash: String,
        file_size: u64,
        references: Vec<Reference>,
        diagnostics: Vec<Diagnostic>,
    ) {
        self.entries.insert(
            rel_path,
            CachedEntry {
                content_hash,
                file_size,
                references,
                diagnostics,
            },
        );
    }

    /// Retains only entries for files that currently exist in the provided active list.
    pub fn prune(&mut self, active_paths: &[String]) {
        let active_set: std::collections::HashSet<_> = active_paths.iter().collect();
        self.entries.retain(|path, _| active_set.contains(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_compute_content_hash() {
        let hash1 = compute_content_hash(b"hello world");
        let hash2 = compute_content_hash(b"hello world");
        let hash3 = compute_content_hash(b"hello world!");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // 256-bit BLAKE3 in hex format
    }

    #[test]
    fn test_cache_roundtrip() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache_path = temp_file.path();

        let mut cache = ScanCache::new();
        let hash_val = compute_content_hash(b"dummy code");
        cache.insert(
            "src/index.ts".to_string(),
            hash_val.clone(),
            100,
            Vec::new(),
            Vec::new(),
        );

        cache.save(cache_path).unwrap();

        let loaded = ScanCache::load(cache_path).expect("Failed to load cache");
        assert_eq!(loaded.version, CACHE_VERSION);
        assert!(loaded.get("src/index.ts", &hash_val).is_some());
        assert!(loaded.get("src/index.ts", "wrong_hash").is_none());
    }

    #[test]
    fn test_cache_matching_invalidation_on_config_or_lang_change() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache_path = temp_file.path();

        let mut cache = ScanCache::new();
        cache.config_hash = "conf_a".to_string();
        cache.languages = "js,ts".to_string();
        cache.save(cache_path).unwrap();

        // Matches
        assert!(ScanCache::load_matching(cache_path, "conf_a", "js,ts").is_some());

        // Invalidate on config change
        assert!(ScanCache::load_matching(cache_path, "conf_b", "js,ts").is_none());

        // Invalidate on languages change
        assert!(ScanCache::load_matching(cache_path, "conf_a", "js,ts,python").is_none());
    }
}
