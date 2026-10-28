//! Fast pre-filter byte search to rule out files that do not access environment variables.
//!
//! Most files in a project never touch environment variables. AST parsing, macro expansion,
//! and thread spawning for thousands of files is computationally expensive.
//! By searching for candidate byte sequences (`env`, `getenv`, `viper`, etc.) in
//! a single memory pass with ASCII case-insensitivity, we can rule out 90%+ of files
//! in microseconds without AST allocation and with ZERO false skips.

/// Inspects source bytes and returns `true` if candidate keywords for environment variable
/// access are present for the given file type.
pub fn quick_prefilter(bytes: &[u8], ext: &str, file_name: &str) -> bool {
    let lower_file = file_name.to_ascii_lowercase();

    // Infrastructure files
    if lower_file == "dockerfile"
        || lower_file.starts_with("dockerfile.")
        || lower_file.ends_with(".dockerfile")
    {
        return contains_subslice_ignore_ascii_case(bytes, b"ENV")
            || contains_subslice_ignore_ascii_case(bytes, b"ARG");
    }

    if lower_file == "docker-compose.yml"
        || lower_file == "docker-compose.yaml"
        || lower_file == "compose.yml"
        || lower_file == "compose.yaml"
        || ext == "yml"
        || ext == "yaml"
    {
        return contains_subslice_ignore_ascii_case(bytes, b"env")
            || contains_subslice_ignore_ascii_case(bytes, b"secret");
    }

    if matches!(ext, "sh" | "bash" | "zsh") {
        // Shell scripts access env vars via '$'
        return bytes.contains(&b'$');
    }

    if matches!(ext, "tf" | "tfvars" | "hcl") {
        return contains_subslice_ignore_ascii_case(bytes, b"env")
            || contains_subslice_ignore_ascii_case(bytes, b"tf_var")
            || contains_subslice_ignore_ascii_case(bytes, b"variable ");
    }

    // Language source files
    match ext {
        "rs" => {
            // Rust: std::env::var, env!("..."), #[clap(env = "...")]
            contains_subslice_ignore_ascii_case(bytes, b"env")
        }
        "js" | "ts" | "jsx" | "tsx" | "mjs" | "cjs" | "mts" | "cts" | "vue" | "svelte"
        | "astro" => {
            // JS/TS: process.env, Bun.env, Deno.env, import.meta.env
            contains_subslice_ignore_ascii_case(bytes, b"env")
        }
        "py" | "pyi" | "pyw" => {
            // Python: os.environ, os.getenv, from os import environ as e, BaseSettings
            contains_subslice_ignore_ascii_case(bytes, b"env")
                || contains_subslice_ignore_ascii_case(bytes, b"environ")
                || contains_subslice_ignore_ascii_case(bytes, b"basesettings")
        }
        "go" => {
            // Go: os.Getenv, os.LookupEnv, os.Environ, viper.BindEnv
            contains_subslice_ignore_ascii_case(bytes, b"env")
                || contains_subslice_ignore_ascii_case(bytes, b"viper")
        }
        _ => true,
    }
}

/// Fast case-insensitive ASCII byte substring search.
#[inline]
pub fn contains_subslice_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }

    let first = needle[0];
    let max_search_idx = haystack.len() - needle.len();

    let mut i = 0;
    while i <= max_search_idx {
        if haystack[i].eq_ignore_ascii_case(&first) {
            let mut matches = true;
            for j in 1..needle.len() {
                if !haystack[i + j].eq_ignore_ascii_case(&needle[j]) {
                    matches = false;
                    break;
                }
            }
            if matches {
                return true;
            }
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
        assert!(quick_prefilter(
            b"println!(\"{}\", env!(\"CARGO\"));",
            "rs",
            "lib.rs"
        ));
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
        assert!(quick_prefilter(
            b"const d = Deno.env.get(\"URL\");",
            "ts",
            "deno.ts"
        ));
        assert!(!quick_prefilter(
            b"export function calculate() { return 42; }",
            "ts",
            "calc.ts"
        ));
    }

    #[test]
    fn test_prefilter_python_including_aliases() {
        assert!(quick_prefilter(
            b"import os\nx = os.getenv('KEY')",
            "py",
            "app.py"
        ));
        // Aliased import
        assert!(quick_prefilter(
            b"from os import environ as e\nval = e['DATABASE_URL']",
            "py",
            "app.py"
        ));
        assert!(quick_prefilter(
            b"from os import getenv as g\nval = g('SECRET')",
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
    fn test_prefilter_go_case_variations() {
        // Go Getenv with capital G
        assert!(quick_prefilter(
            b"val := os.Getenv(\"PORT\")",
            "go",
            "main.go"
        ));
        // Go LookupEnv with capital L and capital E
        assert!(quick_prefilter(
            b"val, ok := os.LookupEnv(\"PORT\")",
            "go",
            "main.go"
        ));
        // Go Environ
        assert!(quick_prefilter(
            b"for _, e := range os.Environ() {}",
            "go",
            "main.go"
        ));
        // Viper BindEnv
        assert!(quick_prefilter(
            b"viper.BindEnv(\"port\", \"PORT\")",
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
    fn test_prefilter_infra_and_terraform() {
        assert!(quick_prefilter(b"ENV PORT=8080", "", "Dockerfile"));
        assert!(quick_prefilter(
            b"ARG DEBIAN_FRONTEND=noninteractive",
            "",
            "Dockerfile"
        ));
        assert!(!quick_prefilter(
            b"FROM alpine:latest\nRUN echo hello",
            "",
            "Dockerfile"
        ));
        assert!(quick_prefilter(b"echo $MY_VAR", "sh", "deploy.sh"));
        assert!(!quick_prefilter(b"echo hello world", "sh", "hello.sh"));

        // Terragrunt get_env
        assert!(quick_prefilter(
            b"port = get_env(\"APP_PORT\", \"8080\")",
            "hcl",
            "terragrunt.hcl"
        ));
        // Terraform variable definition
        assert!(quick_prefilter(
            b"variable \"db_password\" {\n  type = string\n}",
            "tf",
            "variables.tf"
        ));
    }
}
