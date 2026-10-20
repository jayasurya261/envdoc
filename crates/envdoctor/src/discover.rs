//! File discovery implementation respecting ignore rules, boundaries, and vendor filters.

#![allow(clippy::disallowed_methods)]

use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Options configuring file discovery.
#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    /// Paths to search (directories or files).
    pub paths: Vec<PathBuf>,
    /// Whether to include ignored/hidden files.
    pub include_ignored: bool,
    /// Follow symlinks (default: false).
    pub follow_symlinks: bool,
    /// Maximum number of files to return before aborting.
    pub max_files: usize,
    /// Maximum directory recursion depth.
    pub max_depth: usize,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            paths: vec![PathBuf::from(".")],
            include_ignored: false,
            follow_symlinks: false,
            max_files: 10_000,
            max_depth: 30,
        }
    }
}

/// Results of running file discovery.
#[derive(Debug, Clone)]
pub struct DiscoveredFiles {
    /// All discovered source file paths.
    pub files: Vec<PathBuf>,
    /// True if discovery was halted due to exceeding `max_files`.
    pub file_count_limit_exceeded: bool,
    /// True if recursion encountered directories beyond `max_depth`.
    pub depth_limit_exceeded: bool,
}

/// Returns true if the file has a supported JavaScript or TypeScript extension.
pub fn is_supported_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("js" | "mjs" | "cjs" | "jsx" | "ts" | "mts" | "cts" | "tsx")
    )
}

/// Returns true if the directory is a standard vendor/build artifact directory.
fn is_standard_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | "target" | "dist" | "build" | ".next" | ".git"
    )
}

/// Discovers all candidate source files matching configured options.
pub fn discover_files(options: &DiscoveryOptions) -> DiscoveredFiles {
    let mut files = Vec::new();
    let mut file_count_limit_exceeded = false;
    let mut depth_limit_exceeded = false;

    let targets = if options.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        options.paths.clone()
    };

    for target in targets {
        // If target is directly a file:
        if target.is_file() {
            if is_supported_source_file(&target) {
                if files.len() >= options.max_files {
                    file_count_limit_exceeded = true;
                    break;
                }
                files.push(target);
            }
            continue;
        }

        let mut builder = WalkBuilder::new(&target);
        builder.follow_links(options.follow_symlinks);
        builder.max_depth(Some(options.max_depth));

        if options.include_ignored {
            builder
                .standard_filters(false)
                .hidden(false)
                .parents(false)
                .ignore(false)
                .git_ignore(false)
                .git_global(false)
                .git_exclude(false);
        } else {
            builder.filter_entry(|entry| {
                if let Some(name) = entry.file_name().to_str() {
                    if entry.file_type().is_some_and(|ft| ft.is_dir())
                        && is_standard_ignored_dir(name)
                    {
                        return false;
                    }
                }
                true
            });
        }

        for result in builder.build() {
            match result {
                Ok(entry) => {
                    if entry.depth() >= options.max_depth
                        && entry.file_type().is_some_and(|ft| ft.is_dir())
                    {
                        depth_limit_exceeded = true;
                    }

                    if entry.file_type().is_some_and(|ft| ft.is_file()) {
                        let path = entry.into_path();
                        if is_supported_source_file(&path) {
                            if files.len() >= options.max_files {
                                file_count_limit_exceeded = true;
                                break;
                            }
                            files.push(path);
                        }
                    }
                }
                Err(_) => {
                    // Ignored or inaccessible directory/file handled by engine
                }
            }

            if file_count_limit_exceeded {
                break;
            }
        }

        if file_count_limit_exceeded {
            break;
        }
    }

    DiscoveredFiles {
        files,
        file_count_limit_exceeded,
        depth_limit_exceeded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported_source_file() {
        assert!(is_supported_source_file(Path::new("index.ts")));
        assert!(is_supported_source_file(Path::new("app.tsx")));
        assert!(is_supported_source_file(Path::new("server.js")));
        assert!(is_supported_source_file(Path::new("config.mjs")));
        assert!(is_supported_source_file(Path::new("helper.cjs")));
        assert!(is_supported_source_file(Path::new("view.jsx")));
        assert!(is_supported_source_file(Path::new("api.mts")));
        assert!(is_supported_source_file(Path::new("db.cts")));

        assert!(!is_supported_source_file(Path::new(".env")));
        assert!(!is_supported_source_file(Path::new(".env.example")));
        assert!(!is_supported_source_file(Path::new("Cargo.toml")));
        assert!(!is_supported_source_file(Path::new("package.json")));
        assert!(!is_supported_source_file(Path::new("README.md")));
    }

    #[test]
    fn test_is_standard_ignored_dir() {
        assert!(is_standard_ignored_dir("node_modules"));
        assert!(is_standard_ignored_dir("target"));
        assert!(is_standard_ignored_dir("dist"));
        assert!(is_standard_ignored_dir("build"));
        assert!(is_standard_ignored_dir(".next"));
        assert!(is_standard_ignored_dir(".git"));

        assert!(!is_standard_ignored_dir("src"));
        assert!(!is_standard_ignored_dir("components"));
        assert!(!is_standard_ignored_dir("lib"));
    }
}
