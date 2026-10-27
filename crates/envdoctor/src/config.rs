#![allow(clippy::disallowed_methods)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Root configuration loaded from `envdoctor.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub audit: AuditConfig,
    #[serde(default)]
    pub workspaces: WorkspacesConfig,
    #[serde(default)]
    pub rules: RulesConfig,
    #[serde(default)]
    pub framework: FrameworkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuditConfig {
    pub contract: Option<PathBuf>,
    pub env: Option<Vec<PathBuf>>,
    #[serde(default)]
    pub ignore_keys: Vec<String>,
    #[serde(default)]
    pub no_default_ignores: bool,
    pub fail_on: Option<String>,
    pub case_insensitive: Option<bool>,
    pub languages: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspacesConfig {
    #[serde(default)]
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RulesConfig {
    #[serde(default)]
    pub severities: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrameworkConfig {
    pub preset: Option<String>,
}

impl Config {
    /// Loads configuration from a specific file path.
    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file '{}': {}", path.display(), e))?;
        toml::from_str(&content)
            .map_err(|e| format!("Invalid TOML syntax in '{}': {}", path.display(), e))
    }

    /// Searches for `envdoctor.toml` in the given directory or parent directories.
    pub fn find_and_load(start_dir: &Path) -> Option<Self> {
        let mut curr = start_dir;
        loop {
            let candidate = curr.join("envdoctor.toml");
            if candidate.is_file() {
                if let Ok(cfg) = Self::load_from_path(&candidate) {
                    return Some(cfg);
                }
            }
            match curr.parent() {
                Some(parent) => curr = parent,
                None => break,
            }
        }
        None
    }
}
