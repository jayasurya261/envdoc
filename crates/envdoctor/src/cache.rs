//! Incremental scan cache keyed by file content hash.
//!
//! Stores extracted references and diagnostics in `.envdoctor-cache` across runs,
//! enabling instant no-op re-runs and rapid incremental watch mode.

#![allow(clippy::disallowed_methods)]

use crate::diagnostics::diagnostic::Diagnostic;
use crate::model::reference::Reference;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Cache schema version to prevent format drift.
pub const CACHE_VERSION: u32 = 1;

/// Default name of the cache file stored at workspace root.
pub const DEFAULT_CACHE_FILE: &str = ".envdoctor-cache";

/// Computes an extremely fast, deterministic 64-bit FNV-1a content hash across file bytes.
pub fn compute_content_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Cache entry stored for an individual source file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedEntry {
    /// 64-bit FNV-1a content hash of the file bytes.
    pub content_hash: u64,
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
    /// Map from relative file path string to cached scan results.
    pub entries: HashMap<String, CachedEntry>,
}

impl Default for ScanCache {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
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

    /// Serializes and writes this cache to disk.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, json)
    }

    /// Retrieves cached results if the file's current content hash matches the cached entry.
    pub fn get(&self, rel_path: &str, current_hash: u64) -> Option<(&[Reference], &[Diagnostic])> {
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
        content_hash: u64,
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
    }

    #[test]
    fn test_cache_roundtrip() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache_path = temp_file.path();

        let mut cache = ScanCache::new();
        cache.insert(
            "src/index.ts".to_string(),
            12345,
            100,
            Vec::new(),
            Vec::new(),
        );

        cache.save(cache_path).unwrap();

        let loaded = ScanCache::load(cache_path).expect("Failed to load cache");
        assert_eq!(loaded.version, CACHE_VERSION);
        assert!(loaded.get("src/index.ts", 12345).is_some());
        assert!(loaded.get("src/index.ts", 99999).is_none());
    }
}
