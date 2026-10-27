//! Comprehensive integration tests for Phase 5:
//! File Discovery, Bounds & Limits, Orchestration Engine, Renderers, and CLI.

#![allow(clippy::disallowed_methods)]

use envdoctor::diagnostics::code::DiagnosticCode;
use envdoctor::diagnostics::severity::Severity;
use envdoctor::discover::{discover_files, DiscoveryOptions};
use envdoctor::engine::{run_engine, EngineConfig};
use envdoctor::render::human::{advice_for_diagnostic, render_to_string};
use envdoctor::render::json::render as render_json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempWorkspace {
    dir: PathBuf,
}

impl TempWorkspace {
    fn new(name: &str) -> Self {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join("envdoctor_tests").join(format!(
            "{}_{}_{}",
            name,
            std::process::id(),
            counter
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn write_file(&self, rel_path: &str, content: &str) -> PathBuf {
        let full_path = self.dir.join(rel_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
        full_path
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn test_discover_standard_filters_and_extensions() {
    let ws = TempWorkspace::new("discover_filters");

    // Standard source files
    ws.write_file("src/index.ts", "console.log(process.env.APP_PORT);");
    ws.write_file("src/utils.js", "const x = process.env.API_KEY;");
    ws.write_file("src/components/App.tsx", "export const App = () => null;");

    // Ignored folders by default
    ws.write_file("node_modules/pkg/index.js", "process.env.INTERNAL = 1;");
    ws.write_file("target/debug/build.js", "process.env.BUILD = 1;");
    ws.write_file("dist/bundle.js", "process.env.DIST = 1;");
    ws.write_file(".next/server.js", "process.env.NEXT = 1;");

    // Non-source extensions
    ws.write_file("README.md", "# Readme");
    ws.write_file(".env.example", "APP_PORT=3000\nAPI_KEY=abc");

    // 1. Default discovery
    let opts = DiscoveryOptions {
        paths: vec![ws.path().to_path_buf()],
        include_ignored: false,
        follow_symlinks: false,
        max_files: 100,
        max_depth: 10,
        languages: vec!["all".to_string()],
    };
    let discovered = discover_files(&opts);
    assert!(!discovered.file_count_limit_exceeded);
    assert!(!discovered.depth_limit_exceeded);

    let filenames: Vec<String> = discovered
        .files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(filenames.contains(&"index.ts".to_string()));
    assert!(filenames.contains(&"utils.js".to_string()));
    assert!(filenames.contains(&"App.tsx".to_string()));
    assert_eq!(filenames.len(), 3);

    // 2. Override with include_ignored: true
    let opts_ignored = DiscoveryOptions {
        paths: vec![ws.path().to_path_buf()],
        include_ignored: true,
        follow_symlinks: false,
        max_files: 100,
        max_depth: 10,
        languages: vec!["all".to_string()],
    };
    let discovered_all = discover_files(&opts_ignored);
    assert!(discovered_all.files.len() > 3);
}

#[test]
fn test_discover_max_files_limit() {
    let ws = TempWorkspace::new("discover_limit");
    for i in 0..5 {
        ws.write_file(&format!("src/file_{}.ts", i), "console.log(1);");
    }

    let opts = DiscoveryOptions {
        paths: vec![ws.path().to_path_buf()],
        include_ignored: false,
        follow_symlinks: false,
        max_files: 2,
        max_depth: 10,
        languages: vec!["all".to_string()],
    };
    let discovered = discover_files(&opts);
    assert!(discovered.file_count_limit_exceeded);
    assert_eq!(discovered.files.len(), 2);
}

#[test]
fn test_engine_missing_contract_env020() {
    let ws = TempWorkspace::new("missing_contract");
    ws.write_file("src/main.ts", "const port = process.env.PORT;");

    let non_existent = ws.path().join(".env.missing");
    let config = EngineConfig {
        contract_path: non_existent,
        env_path: None,
        scan_paths: vec![ws.path().to_path_buf()],
        ..Default::default()
    };

    let report = run_engine(&config);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == DiagnosticCode::ENV020));
    assert_eq!(report.exit_code(Severity::Warning), 2); // Usage error
}

#[test]
fn test_engine_empty_contract_env021() {
    let ws = TempWorkspace::new("empty_contract");
    let contract = ws.write_file(".env.example", "# Only comments\n\n# Another comment\n");
    ws.write_file("src/main.ts", "console.log('hello');");

    let config = EngineConfig {
        contract_path: contract,
        env_path: None,
        scan_paths: vec![ws.path().to_path_buf()],
        ..Default::default()
    };

    let report = run_engine(&config);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == DiagnosticCode::ENV021));
    assert_eq!(report.exit_code(Severity::Warning), 1);
    assert_eq!(report.exit_code(Severity::Error), 0);
}

#[test]
fn test_engine_oversized_file_io010() {
    let ws = TempWorkspace::new("oversized_file");
    let contract = ws.write_file(".env.example", "BIG_DATA=123");
    // Write a source file that is 50 bytes long
    let content = "console.log(process.env.BIG_DATA); // padded text 1234567890";
    ws.write_file("src/big.ts", content);

    let config = EngineConfig {
        contract_path: contract,
        env_path: None,
        scan_paths: vec![ws.path().to_path_buf()],
        max_file_size: 20, // 20 bytes threshold
        ..Default::default()
    };

    let report = run_engine(&config);
    assert!(!report.summary.complete);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == DiagnosticCode::IO010));
    assert_eq!(report.summary.skipped_files, 1);
    assert_eq!(report.exit_code(Severity::Warning), 3); // Operational failure
}

#[test]
fn test_engine_clean_contract_pass() {
    let ws = TempWorkspace::new("clean_pass");
    let contract = ws.write_file(
        ".env.example",
        "PORT=3000\nDATABASE_URL=postgres://localhost\n",
    );
    ws.write_file(
        "src/server.ts",
        "const port = process.env.PORT;\nconst db = process.env.DATABASE_URL;\n",
    );

    let config = EngineConfig {
        contract_path: contract,
        env_path: None,
        scan_paths: vec![ws.path().to_path_buf()],
        ..Default::default()
    };

    let report = run_engine(&config);
    assert!(report.summary.complete);
    assert_eq!(report.summary.total_errors, 0);
    assert_eq!(report.summary.total_warnings, 0);
    assert_eq!(report.summary.total_notes, 0);
    assert_eq!(report.exit_code(Severity::Warning), 0);
}

#[test]
fn test_engine_rule_violations_env001_env002_env003_env007() {
    let ws = TempWorkspace::new("rule_violations");
    let contract = ws.write_file(
        ".env.example",
        "DECLARED_UNUSED=true\nREQUIRED_VAR=secret\n# @optional\nOPTIONAL_VAR=val\n",
    );
    let env_file = ws.write_file(
        ".env",
        "# Missing REQUIRED_VAR\nDECLARED_UNUSED=true\nOPTIONAL_VAR=\n",
    );
    ws.write_file(
        "src/app.ts",
        "const port = process.env.UNDOCUMENTED_KEY;\nconst unused = process.env.DECLARED_UNUSED;\n",
    );

    let config = EngineConfig {
        contract_path: contract,
        env_path: Some(env_file),
        scan_paths: vec![ws.path().to_path_buf()],
        ..Default::default()
    };

    let report = run_engine(&config);

    // ENV001: UNDOCUMENTED_KEY used in code but absent from contract
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == DiagnosticCode::ENV001));

    // ENV003: REQUIRED_VAR in contract but missing from .env
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == DiagnosticCode::ENV003));

    // Contract has REQUIRED_VAR not referenced in source -> ENV002
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == DiagnosticCode::ENV002));

    // OPTIONAL_VAR is empty in .env, but it's optional so ENV007 should NOT trigger for it
    assert!(!report
        .diagnostics
        .iter()
        .any(|d| d.code == DiagnosticCode::ENV007));

    assert_eq!(report.exit_code(Severity::Warning), 1);
}

#[test]
fn test_human_and_json_renderers() {
    let ws = TempWorkspace::new("renderers");
    let contract = ws.write_file(".env.example", "PORT=3000\n");
    ws.write_file("src/app.ts", "console.log(process.env.PORT);");

    let config = EngineConfig {
        contract_path: contract,
        env_path: None,
        scan_paths: vec![ws.path().to_path_buf()],
        ..Default::default()
    };

    let report = run_engine(&config);

    // Test JSON render
    let json = render_json(&report).expect("JSON render should succeed");
    assert!(json.contains("\"schema_version\": 1"));
    assert!(json.contains("\"tool_name\": \"envdoctor\""));

    // Test Human render
    let human_plain = render_to_string(&report, false);
    assert!(human_plain.contains("Success: No environment contract violations found."));
    assert!(human_plain.contains("Files scanned: 1"));

    let human_color = render_to_string(&report, true);
    assert!(human_color.contains("\x1b["));
}

#[test]
fn test_advice_hints_coverage() {
    use envdoctor::model::span::{Location, Span};
    let diag = envdoctor::Diagnostic::new(
        DiagnosticCode::ENV001,
        Severity::Warning,
        "msg",
        Some("MY_VAR".into()),
        vec![Location::new("src/main.ts", Span::point(1, 1, 0))],
        None,
    );
    let advice = advice_for_diagnostic(&diag).expect("Should have advice");
    assert!(advice.contains("MY_VAR"));
    assert!(advice.contains(".env.example"));
}
