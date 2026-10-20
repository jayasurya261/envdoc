//! Conformance Suite (Step 6.2).
//!
//! Executes end-to-end conformance validation against standardized fixture projects:
//! - Pure TypeScript project
//! - Mixed JS/TS project with dynamic access
//! - Missing vs. optional environment variables
//! - Monorepo multi-directory paths

#![allow(clippy::disallowed_methods)]

use envdoctor::Report;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin_path() -> PathBuf {
    env!("CARGO_BIN_EXE_envdoctor").into()
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("conformance")
}

#[test]
fn test_conformance_01_pure_typescript() {
    let dir = fixtures_dir().join("01_pure_typescript");
    let contract = dir.join(".env.example");
    let env_file = dir.join(".env");
    let src_path = dir.join("src");

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--env")
        .arg(&env_file)
        .arg("--path")
        .arg(&src_path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute CLI");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Report = serde_json::from_str(&stdout).expect("Stdout must be valid Report JSON");

    assert_eq!(report.summary.scanned_files, 2);
    assert_eq!(report.summary.skipped_files, 0);
    assert_eq!(report.summary.total_errors, 0);
    assert_eq!(report.summary.total_warnings, 0);
    assert_eq!(report.summary.total_notes, 0);
    assert!(report.summary.complete);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn test_conformance_02_mixed_js_ts_dynamic() {
    let dir = fixtures_dir().join("02_mixed_js_ts_dynamic");
    let contract = dir.join(".env.example");
    let src_path = dir.join("src");

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--path")
        .arg(&src_path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute CLI");

    // Exit code is 0 because default threshold is warning and ENV010/ENV012 are notes
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Report = serde_json::from_str(&stdout).expect("Stdout must be valid Report JSON");

    assert_eq!(report.summary.scanned_files, 2);
    assert_eq!(report.summary.total_errors, 0);
    assert_eq!(report.summary.total_warnings, 0);
    assert_eq!(report.summary.total_notes, 2);
    assert!(report.summary.complete);

    let codes: Vec<_> = report.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert!(codes.contains(&"ENV010"));
    assert!(codes.contains(&"ENV012"));
}

#[test]
fn test_conformance_03_missing_vs_optional() {
    let dir = fixtures_dir().join("03_missing_vs_optional");
    let contract = dir.join(".env.example");
    let env_file = dir.join(".env");
    let src_path = dir.join("src");

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--env")
        .arg(&env_file)
        .arg("--path")
        .arg(&src_path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute CLI");

    // Exit code 1 due to error (ENV003) and warning (ENV007)
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Report = serde_json::from_str(&stdout).expect("Stdout must be valid Report JSON");

    assert_eq!(report.summary.scanned_files, 1);
    assert_eq!(report.summary.total_errors, 1);
    assert_eq!(report.summary.total_warnings, 1);
    assert_eq!(report.summary.total_notes, 1);
    assert!(report.summary.complete);

    // Verify ENV003 for REQUIRED_MISSING
    let required_missing = report
        .diagnostics
        .iter()
        .find(|d| d.key.as_ref().map(|k| k.as_str()) == Some("REQUIRED_MISSING"))
        .expect("Must have diagnostic for REQUIRED_MISSING");
    assert_eq!(required_missing.code.as_str(), "ENV003");
    assert_eq!(required_missing.severity, envdoctor::Severity::Error);

    // Verify ENV007 for REQUIRED_EMPTY
    let required_empty = report
        .diagnostics
        .iter()
        .find(|d| d.key.as_ref().map(|k| k.as_str()) == Some("REQUIRED_EMPTY"))
        .expect("Must have diagnostic for REQUIRED_EMPTY");
    assert_eq!(required_empty.code.as_str(), "ENV007");
    assert_eq!(required_empty.severity, envdoctor::Severity::Warning);

    // Verify OPTIONAL_MISSING was downgraded to Note
    let optional_missing = report
        .diagnostics
        .iter()
        .find(|d| d.key.as_ref().map(|k| k.as_str()) == Some("OPTIONAL_MISSING"))
        .expect("Must have diagnostic for OPTIONAL_MISSING");
    assert_eq!(optional_missing.code.as_str(), "ENV003");
    assert_eq!(optional_missing.severity, envdoctor::Severity::Note);
}

#[test]
fn test_conformance_04_monorepo_manual_dirs() {
    let dir = fixtures_dir().join("04_monorepo_manual_dirs");
    let contract = dir.join(".env.example");
    let frontend_src = dir.join("packages").join("frontend").join("src");
    let backend_src = dir.join("packages").join("backend").join("src");

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--path")
        .arg(&frontend_src)
        .arg(&backend_src)
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute CLI");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Report = serde_json::from_str(&stdout).expect("Stdout must be valid Report JSON");

    assert_eq!(report.summary.scanned_files, 2);
    assert_eq!(report.summary.total_errors, 0);
    assert_eq!(report.summary.total_warnings, 0);
    assert_eq!(report.summary.total_notes, 0);
    assert!(report.summary.complete);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn test_conformance_expected_reports_schema_compliance() {
    let fixtures = [
        "01_pure_typescript",
        "02_mixed_js_ts_dynamic",
        "03_missing_vs_optional",
        "04_monorepo_manual_dirs",
    ];

    for fixture in fixtures {
        let expected_path = fixtures_dir().join(fixture).join("expected_report.json");
        let content = std::fs::read_to_string(&expected_path)
            .unwrap_or_else(|_| panic!("Failed to read {}", expected_path.display()));
        let report: Report = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to deserialize {}: {}", expected_path.display(), e));
        assert_eq!(report.schema_version, 1);
        assert_eq!(report.tool_name, "envdoctor");
    }
}
