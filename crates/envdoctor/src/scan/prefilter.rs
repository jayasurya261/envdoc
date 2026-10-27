//! Fast pre-filter byte search to rule out files that do not access environment variables.
//!
//! Most files in a project never touch environment variables. AST parsing, macro expansion,
//! and thread spawning for thousands of files is computationally expensive.
//! By searching for candidate byte sequences (`env`, `ENV`, `getenv`, `viper`, etc.) in
//! a single memory pass, we can rule out 90%+ of files in microseconds without AST allocation.

/// Inspects source bytes and returns `true` if candidate keywords for environment variable
/// access are present for the given file type.
pub fn quick_prefilter(bytes: &[u8], ext: &str, file_name: &str) -> bool {
    let lower_file = file_name.to_ascii_lowercase();

    // Infrastructure files
    if lower_file == "dockerfile"
        || lower_file.starts_with("dockerfile.")
        || lower_file.ends_with(".dockerfile")
    {
        return contains_subslice(bytes, b"ENV") || contains_subslice(bytes, b"ARG");
    }

    if lower_file == "docker-compose.yml"
        || lower_file == "docker-compose.yaml"
        || lower_file == "compose.yml"
        || lower_file == "compose.yaml"
        || ext == "yml"
        || ext == "yaml"
    {
        return contains_subslice(bytes, b"env")
            || contains_subslice(bytes, b"ENV")
            || contains_subslice(bytes, b"secrets");
    }

    if matches!(ext, "sh" | "bash" | "zsh") {
        // Shell scripts access env vars via '$'
        return bytes.contains(&b'$');
    }

    if matches!(ext, "tf" | "tfvars") {
        return contains_subslice(bytes, b"env")
            || contains_subslice(bytes, b"ENV")
            || contains_subslice(bytes, b"TF_VAR");
    }

    // Language source files
    match ext {
        "rs" => {
            // Rust: std::env::var, env!("..."), #[clap(env = "...")]
            contains_subslice(bytes, b"env") || contains_subslice(bytes, b"ENV")
        }
        "js" | "ts" | "jsx" | "tsx" | "mjs" | "cjs" | "mts" | "cts" | "vue" | "svelte"
        | "astro" => {
            // JS/TS: process.env, Bun.env, Deno.env, import.meta.env
            contains_subslice(bytes, b"env") || contains_subslice(bytes, b"ENV")
        }
        "py" | "pyi" | "pyw" => {
            // Python: os.environ, os.getenv, BaseSettings, SettingsConfigDict
            contains_subslice(bytes, b"environ")
                || contains_subslice(bytes, b"getenv")
                || contains_subslice(bytes, b"BaseSettings")
                || contains_subslice(bytes, b"env")
                || contains_subslice(bytes, b"ENV")
        }
        "go" => {
            // Go: os.Getenv, os.LookupEnv, viper
            contains_subslice(bytes, b"Getenv")
                || contains_subslice(bytes, b"LookupEnv")
                || contains_subslice(bytes, b"viper")
                || contains_subslice(bytes, b"env")
                || contains_subslice(bytes, b"ENV")
        }
        _ => true,
    }
}

/// Fast byte substring search without external dependencies.
#[inline]
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }

    let first = needle[0];
    let needle_tail = &needle[1..];
    let max_search_idx = haystack.len() - needle.len();

    let mut i = 0;
    while i <= max_search_idx {
        if haystack[i] == first && &haystack[i + 1..i + needle.len()] == needle_tail {
            return true;
        }
        i += 1;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefilter_rust() {
        assert!(quick_prefilter(
            b"let x = std::env::var(\"FOO\");",
            "rs",
            "main.rs"
        ));
        assert!(quick_prefilter(b"#[clap(env = \"PORT\")]", "rs", "cli.rs"));
        assert!(!quick_prefilter(
            b"fn add(a: i32, b: i32) -> i32 { a + b }",
            "rs",
            "math.rs"
        ));
    }

    #[test]
    fn test_prefilter_js_ts() {
        assert!(quick_prefilter(
            b"const x = process.env.API_KEY;",
            "ts",
            "api.ts"
        ));
        assert!(quick_prefilter(
            b"const b = Bun.env.PORT;",
            "js",
            "server.js"
        ));
        assert!(!quick_prefilter(
            b"export function calculate() { return 42; }",
            "ts",
            "calc.ts"
        ));
    }

    #[test]
    fn test_prefilter_python() {
        assert!(quick_prefilter(
            b"import os\nx = os.getenv('KEY')",
            "py",
            "app.py"
        ));
        assert!(quick_prefilter(
            b"class Config(BaseSettings): pass",
            "py",
            "config.py"
        ));
        assert!(!quick_prefilter(
            b"def greet(name):\n    return f'Hello {name}'",
            "py",
            "utils.py"
        ));
    }

    #[test]
    fn test_prefilter_go() {
        assert!(quick_prefilter(
            b"val := os.Getenv(\"PORT\")",
            "go",
            "main.go"
        ));
        assert!(quick_prefilter(
            b"port := viper.GetString(\"PORT\")",
            "go",
            "config.go"
        ));
        assert!(!quick_prefilter(
            b"package main\nfunc Add(a, b int) int { return a + b }",
            "go",
            "add.go"
        ));
    }

    #[test]
    fn test_prefilter_infra() {
        assert!(quick_prefilter(b"ENV PORT=8080", "", "Dockerfile"));
        assert!(!quick_prefilter(
            b"FROM alpine:latest\nRUN echo hello",
            "",
            "Dockerfile"
        ));
        assert!(quick_prefilter(b"echo $MY_VAR", "sh", "deploy.sh"));
        assert!(!quick_prefilter(b"echo hello world", "sh", "hello.sh"));
    }
}
