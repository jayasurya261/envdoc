//! Integration tests for the compiled `envdoctor` CLI binary.

#![allow(clippy::disallowed_methods)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempWorkspace {
    dir: PathBuf,
}

impl TempWorkspace {
    fn new(name: &str) -> Self {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join("envdoctor_cli_tests")
            .join(format!("{}_{}_{}", name, std::process::id(), counter));
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

fn bin_path() -> PathBuf {
    env!("CARGO_BIN_EXE_envdoctor").into()
}

#[test]
fn test_cli_help() {
    let output = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("envdoctor"));
    assert!(stdout.contains("--contract"));
    assert!(stdout.contains("--format"));
    assert!(stdout.contains("--fail-on"));
}

#[test]
fn test_cli_missing_contract_exit_code_2() {
    let ws = TempWorkspace::new("cli_missing_contract");
    let missing_path = ws.path().join(".env.not_found");

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&missing_path)
        .arg("--path")
        .arg(ws.path())
        .output()
        .expect("Failed to execute command");

    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ENV020"));
    assert!(stdout.contains("contract-missing"));
}

#[test]
fn test_cli_clean_pass_exit_code_0() {
    let ws = TempWorkspace::new("cli_clean_pass");
    let contract = ws.write_file(".env.example", "PORT=3000\n");
    ws.write_file("src/index.ts", "console.log(process.env.PORT);");

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--path")
        .arg(ws.path())
        .arg("--format")
        .arg("human")
        .arg("--no-color")
        .output()
        .expect("Failed to execute command");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Success: No environment contract violations found."));
}

#[test]
fn test_cli_contract_violation_exit_code_1() {
    let ws = TempWorkspace::new("cli_contract_violation");
    let contract = ws.write_file(".env.example", "PORT=3000\n");
    ws.write_file(
        "src/index.ts",
        "console.log(process.env.PORT);\nconsole.log(process.env.SECRET_KEY);",
    );

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--path")
        .arg(ws.path())
        .arg("--fail-on")
        .arg("warning")
        .output()
        .expect("Failed to execute command");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ENV001"));
    assert!(stdout.contains("undocumented-reference"));
}

#[test]
fn test_cli_json_output_conformance() {
    let ws = TempWorkspace::new("cli_json_output");
    let contract = ws.write_file(".env.example", "PORT=3000\n");
    ws.write_file("src/index.ts", "console.log(process.env.PORT);");

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--path")
        .arg(ws.path())
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute command");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Stdout must be valid JSON");

    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["tool_name"], "envdoctor");
    assert_eq!(parsed["summary"]["scanned_files"], 1);
    assert_eq!(parsed["summary"]["complete"], true);
}
