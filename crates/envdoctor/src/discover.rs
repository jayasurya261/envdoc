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
    /// Configured languages to scan ('default' for js/ts/rust, 'all', or specific: 'js', 'python', etc.).
    pub languages: Vec<String>,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            paths: vec![PathBuf::from(".")],
            include_ignored: false,
            follow_symlinks: false,
            max_files: 10_000,
            max_depth: 30,
            languages: vec!["all".to_string()],
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

/// Returns true if the file has a supported programming language, script, container, or infra extension,
/// filtered by the configured list of enabled languages.
pub fn is_supported_source_file_for_languages(path: &Path, languages: &[String]) -> bool {
    let all = languages.iter().any(|l| l.eq_ignore_ascii_case("all"));
    let default =
        languages.is_empty() || languages.iter().any(|l| l.eq_ignore_ascii_case("default"));

    let allow_js_ts = all
        || default
        || languages.iter().any(|l| {
            let s = l.to_ascii_lowercase();
            s == "js" || s == "ts" || s == "javascript" || s == "typescript"
        });

    let allow_rust = all
        || default
        || languages.iter().any(|l| {
            let s = l.to_ascii_lowercase();
            s == "rust" || s == "rs"
        });

    let allow_python = all
        || languages.iter().any(|l| {
            let s = l.to_ascii_lowercase();
            s == "python" || s == "py"
        });

    let allow_go = all
        || languages.iter().any(|l| {
            let s = l.to_ascii_lowercase();
            s == "go"
        });

    let allow_infra = all
        || languages.iter().any(|l| {
            let s = l.to_ascii_lowercase();
            s == "infra"
                || s == "docker"
                || s == "shell"
                || s == "sh"
                || s == "compose"
                || s == "k8s"
                || s == "terraform"
        });

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let lower_name = file_name.to_ascii_lowercase();

    if allow_infra {
        if lower_name == "dockerfile"
            || lower_name.starts_with("dockerfile.")
            || lower_name.ends_with(".dockerfile")
        {
            return true;
        }

        if lower_name == "docker-compose.yml"
            || lower_name == "docker-compose.yaml"
            || lower_name == "compose.yml"
            || lower_name == "compose.yaml"
        {
            return true;
        }
    }

    let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

    if allow_js_ts
        && matches!(
            ext,
            "js" | "mjs"
                | "cjs"
                | "jsx"
                | "ts"
                | "mts"
                | "cts"
                | "tsx"
                | "vue"
                | "svelte"
                | "astro"
        )
    {
        return true;
    }

    if allow_rust && ext == "rs" {
        return true;
    }

    if allow_python && matches!(ext, "py" | "pyi" | "pyw") {
        return true;
    }

    if allow_go && ext == "go" {
        return true;
    }

    if allow_infra
        && (matches!(ext, "sh" | "bash" | "zsh" | "tf" | "tfvars" | "hcl")
            || (matches!(ext, "yml" | "yaml") && path.to_string_lossy().contains(".github")))
    {
        return true;
    }

    false
}

/// Returns true if the file has a supported programming language, script, container, or infra extension.
pub fn is_supported_source_file(path: &Path) -> bool {
    is_supported_source_file_for_languages(path, &["all".to_string()])
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
                        if is_supported_source_file_for_languages(&path, &options.languages) {
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
        assert!(is_supported_source_file(Path::new("main.rs")));
        assert!(is_supported_source_file(Path::new("lib.rs")));

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
