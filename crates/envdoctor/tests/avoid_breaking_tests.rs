#![allow(clippy::disallowed_methods, clippy::field_reassign_with_default)]

use envdoctor::engine::{run_engine, EngineConfig};
use envdoctor::DiagnosticCode;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_opt_in_languages_prevents_breaking_existing_ci() {
    let dir = tempdir().unwrap();
    let contract_path = dir.path().join(".env.example");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(&contract_path, "PORT=\n").unwrap();

    // TypeScript file using declared PORT
    fs::write(
        src_dir.join("server.ts"),
        "const port = process.env.PORT;\n",
    )
    .unwrap();

    // Python file using an uncontracted var (could break CI if auto-enabled!)
    fs::write(
        src_dir.join("worker.py"),
        "import os\nx = os.environ['UNDECLARED_PYTHON_VAR']\n",
    )
    .unwrap();

    // 1. Default mode: new languages are opt-in, only JS/TS/Rust are scanned
    let mut default_config = EngineConfig::default();
    default_config.contract_path = contract_path.clone();
    default_config.scan_paths = vec![src_dir.clone()];
    default_config.languages = vec!["default".to_string()];
    default_config.use_cache = false;

    let default_report = run_engine(&default_config);
    let default_env001 = default_report
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV001)
        .count();
    // Default mode does NOT break existing CI!
    assert_eq!(default_env001, 0);

    // 2. Opt-in mode: explicitly requesting 'all' or 'python' enables the new scanner
    let mut opt_in_config = EngineConfig::default();
    opt_in_config.contract_path = contract_path;
    opt_in_config.scan_paths = vec![src_dir];
    opt_in_config.languages = vec!["all".to_string()];
    opt_in_config.use_cache = false;

    let opt_in_report = run_engine(&opt_in_config);
    let opt_in_env001 = opt_in_report
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV001)
        .count();
    // Opted-in mode catches the Python variable
    assert_eq!(opt_in_env001, 1);
    assert_eq!(
        opt_in_report.diagnostics[0].key.as_deref().unwrap(),
        "UNDECLARED_PYTHON_VAR"
    );
}

#[test]
fn test_shadow_mode_empty_diff_on_legacy_languages() {
    let dir = tempdir().unwrap();
    let contract_path = dir.path().join(".env.example");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(&contract_path, "PORT=\nDATABASE_URL=\n").unwrap();
    fs::write(
        src_dir.join("main.rs"),
        "fn main() { let _ = std::env::var(\"PORT\"); }\n",
    )
    .unwrap();
    fs::write(
        src_dir.join("index.ts"),
        "const db = process.env.DATABASE_URL;\n",
    )
    .unwrap();

    // Pipeline A: Explicitly pinned to stable languages (js, rust)
    let mut config_a = EngineConfig::default();
    config_a.contract_path = contract_path.clone();
    config_a.scan_paths = vec![src_dir.clone()];
    config_a.languages = vec!["js".into(), "rust".into()];
    config_a.use_cache = false;

    // Pipeline B: Default configuration
    let mut config_b = EngineConfig::default();
    config_b.contract_path = contract_path;
    config_b.scan_paths = vec![src_dir];
    config_b.languages = vec!["default".into()];
    config_b.use_cache = false;

    let report_a = run_engine(&config_a);
    let report_b = run_engine(&config_b);

    // Diff must be strictly empty!
    assert_eq!(report_a.summary, report_b.summary);
    assert_eq!(report_a.diagnostics, report_b.diagnostics);
}

#[test]
fn test_rollback_runtime_flag_disables_language() {
    let dir = tempdir().unwrap();
    let contract_path = dir.path().join(".env.example");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(&contract_path, "PORT=\n").unwrap();
    fs::write(
        src_dir.join("main.rs"),
        "fn main() { let _ = std::env::var(\"PORT\"); }\n",
    )
    .unwrap();
    fs::write(
        src_dir.join("index.ts"),
        "const x = process.env.EXTRA_TS_VAR;\n",
    )
    .unwrap();

    // If JS/TS had an issue, user/CI can roll back by pinning only rust:
    let mut config = EngineConfig::default();
    config.contract_path = contract_path;
    config.scan_paths = vec![src_dir];
    config.languages = vec!["rust".into()];
    config.use_cache = false;

    let report = run_engine(&config);
    // TypeScript findings are completely excluded
    assert_eq!(report.summary.total_warnings, 0);
    assert_eq!(report.summary.total_errors, 0);
    assert_eq!(report.summary.scanned_files, 1);
}
