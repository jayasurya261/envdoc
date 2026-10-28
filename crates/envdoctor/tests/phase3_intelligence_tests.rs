#![allow(clippy::disallowed_methods, clippy::field_reassign_with_default)]

use envdoctor::diagnostics::code::DiagnosticCode;
use envdoctor::diagnostics::report::Report;
use envdoctor::diagnostics::severity::Severity;
use envdoctor::engine::{run_engine, EngineConfig};
use envdoctor::model::contract::{ContractAnnotations, ContractType};
use envdoctor::model::definition::Definition;
use envdoctor::model::name::EnvName;
use envdoctor::model::path::RelPath;
use envdoctor::model::span::Span;
use envdoctor::model::value::ValueKind;
use envdoctor::render::{render_github, render_junit, render_sarif};
use envdoctor::rules::contract::Contract;
use envdoctor::rules::engine::{evaluate_contract, AuditOptions};
use envdoctor::scan::scan_source;
use envdoctor::suppression::{is_line_suppressed, BaselineEntry, BaselineFile};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_python_scanner_environ_and_pydantic() {
    let py_source = r#"
import os
from pydantic_settings import BaseSettings
from pydantic import Field

def get_keys():
    api_key = os.environ["API_KEY"]
    port = os.getenv("PORT", "8080")
    db_host = os.environ.get("DB_HOST")
    return api_key, port, db_host

class AppSettings(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="APP_", case_sensitive=False)
    database_url: str
    redis_port: int = 6379
    custom_name: str = Field(validation_alias="CUSTOM_ALIAS_VAR")

class V1Settings(BaseSettings):
    class Config:
        env_prefix = "V1_"
    secret_key: str
"#;

    let res = scan_source(py_source, "app/settings.py");
    let names: Vec<_> = res.references.iter().map(|r| r.name.as_str()).collect();

    assert!(names.contains(&"API_KEY"));
    assert!(names.contains(&"PORT"));
    assert!(names.contains(&"DB_HOST"));
    assert!(names.contains(&"APP_DATABASE_URL"));
    assert!(names.contains(&"APP_REDIS_PORT"));
    assert!(names.contains(&"CUSTOM_ALIAS_VAR"));
    assert!(names.contains(&"V1_SECRET_KEY"));
}

#[test]
fn test_go_scanner() {
    let go_source = r#"
package main

import (
    "os"
    "github.com/spf13/viper"
)

func main() {
    port := os.Getenv("APP_PORT")
    dbPass, _ := os.LookupEnv("DB_PASSWORD")
    viper.BindEnv("AUTH_TOKEN")
    viper.BindEnv("db.user", "DB_USER")
    _ = port
    _ = dbPass
}
"#;

    let res = scan_source(go_source, "main.go");
    let names: Vec<_> = res.references.iter().map(|r| r.name.as_str()).collect();

    assert!(names.contains(&"APP_PORT"));
    assert!(names.contains(&"DB_PASSWORD"));
    assert!(names.contains(&"AUTH_TOKEN"));
    assert!(names.contains(&"DB_USER"));
}

#[test]
fn test_infra_scanners() {
    // 1. Dockerfile
    let dockerfile = r#"
FROM node:18
ENV NODE_ENV=production
ARG BUILD_HASH=abc1234
ARG TARGET_PORT
"#;
    let res = scan_source(dockerfile, "Dockerfile");
    let names: Vec<_> = res.references.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"NODE_ENV"));
    assert!(names.contains(&"BUILD_HASH"));
    assert!(names.contains(&"TARGET_PORT"));

    // 2. Shell
    let shell_script = r#"
#!/bin/bash
echo "Connecting to $DB_HOST on ${PORT:-5432}"
export API_SECRET="123"
"#;
    let res = scan_source(shell_script, "deploy.sh");
    let names: Vec<_> = res.references.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"DB_HOST"));
    assert!(names.contains(&"PORT"));

    // 3. Kubernetes / Docker Compose
    let k8s_yaml = r#"
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
      - name: web
        env:
        - name: K8S_SERVICE_KEY
          value: "secure"
"#;
    let res = scan_source(k8s_yaml, "deployment.yaml");
    let names: Vec<_> = res.references.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"K8S_SERVICE_KEY"));

    // 4. GitHub Actions
    let gha_yaml = r#"
name: CI
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo "${{ env.GHA_DEPLOY_FLAG }}"
      - run: echo "${{ secrets.PROD_API_SECRET }}"
"#;
    let res = scan_source(gha_yaml, ".github/workflows/ci.yml");
    let names: Vec<_> = res.references.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"GHA_DEPLOY_FLAG"));
    assert!(names.contains(&"PROD_API_SECRET"));

    // 5. Terraform & Terragrunt
    let tf_file = r#"
variable "database_password" {
  type = string
}
locals {
  direct_token = "prefix_${TF_VAR_API_TOKEN}"
}
"#;
    let res = scan_source(tf_file, "main.tf");
    let names: Vec<_> = res.references.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"TF_VAR_database_password"));
    assert!(names.contains(&"TF_VAR_API_TOKEN"));

    let terragrunt_file = r#"
inputs = {
  db_host = get_env("DATABASE_HOST", "localhost")
  api_key = get_env("APP_API_KEY")
}
"#;
    let res = scan_source(terragrunt_file, "terragrunt.hcl");
    let names: Vec<_> = res.references.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"DATABASE_HOST"));
    assert!(names.contains(&"APP_API_KEY"));
}

#[test]
fn test_contract_annotations_and_zero_leak_type_validation() {
    let contract_text = r#"
# @type port
APP_PORT=

# @type url
# @secret
DATABASE_URL=

# @enum production|staging|development
APP_ENV=
"#;

    let parsed = envdoctor::dialect::parse(contract_text, ".env.example");
    assert_eq!(parsed.definitions.len(), 3);

    let port_def = parsed
        .definitions
        .iter()
        .find(|d| d.name.as_str() == "APP_PORT")
        .unwrap();
    assert_eq!(port_def.annotations.expected_type, Some(ContractType::Port));

    let db_def = parsed
        .definitions
        .iter()
        .find(|d| d.name.as_str() == "DATABASE_URL")
        .unwrap();
    assert_eq!(db_def.annotations.expected_type, Some(ContractType::Url));
    assert!(db_def.annotations.is_secret);

    let env_def = parsed
        .definitions
        .iter()
        .find(|d| d.name.as_str() == "APP_ENV")
        .unwrap();
    assert_eq!(
        env_def.annotations.enum_values,
        Some(vec![
            "production".into(),
            "staging".into(),
            "development".into()
        ])
    );

    // Test validation
    assert!(port_def.annotations.validate_value("8080").is_ok());
    assert!(port_def.annotations.validate_value("not_a_number").is_err());
    assert!(db_def
        .annotations
        .validate_value("postgres://user:pass@localhost:5432/db")
        .is_ok());
    assert!(db_def
        .annotations
        .validate_value("invalid_url_no_scheme")
        .is_err());
    assert!(env_def.annotations.validate_value("production").is_ok());
    assert!(env_def.annotations.validate_value("testing").is_err());
}

#[test]
fn test_leak_detection_rule() {
    let mut secret_annotations = ContractAnnotations::default();
    secret_annotations.is_secret = true;

    let mut public_secret_annotations = ContractAnnotations::default();
    public_secret_annotations.is_secret = true;
    public_secret_annotations.is_public = true;

    let definitions = vec![
        Definition::with_annotations(
            EnvName::new("NEXT_PUBLIC_DB_PASSWORD"),
            RelPath::new(".env.example"),
            Span::point(1, 1, 0),
            ValueKind::Empty,
            secret_annotations,
        ),
        Definition::with_annotations(
            EnvName::new("API_SECRET_TOKEN"),
            RelPath::new(".env.example"),
            Span::point(2, 1, 0),
            ValueKind::Empty,
            public_secret_annotations,
        ),
    ];

    let contract = Contract::new(RelPath::new(".env.example"), definitions);
    let diagnostics = evaluate_contract(Some(&contract), None, &[], &AuditOptions::default());

    let leak_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV032)
        .collect();
    assert_eq!(leak_diags.len(), 2);
    assert_eq!(leak_diags[0].severity, Severity::Warning);
    assert_eq!(leak_diags[1].severity, Severity::Warning);
}

#[test]
fn test_typo_did_you_mean_suggestion() {
    let contract_defs = vec![
        Definition::new(
            EnvName::new("DATABASE_URL"),
            RelPath::new(".env.example"),
            Span::point(1, 1, 0),
            ValueKind::Empty,
            false,
        ),
        Definition::new(
            EnvName::new("PORT"),
            RelPath::new(".env.example"),
            Span::point(2, 1, 0),
            ValueKind::Empty,
            false,
        ),
    ];
    let contract = Contract::new(RelPath::new(".env.example"), contract_defs);

    let references = vec![envdoctor::Reference::new(
        EnvName::new("DATABSE_URL"),
        RelPath::new("src/db.ts"),
        Span::point(1, 1, 0),
        envdoctor::EvidenceKind::DirectMember,
    )];

    let mut options = AuditOptions::default();
    options.case_insensitive = false;

    let diags = evaluate_contract(Some(&contract), None, &references, &options);
    let env001 = diags
        .iter()
        .find(|d| d.code == DiagnosticCode::ENV001)
        .unwrap();
    assert!(env001.message.contains("did you mean 'DATABASE_URL'?"));
}

#[test]
fn test_baseline_and_inline_suppressions() {
    let diag = envdoctor::Diagnostic::new(
        DiagnosticCode::ENV001,
        Severity::Warning,
        "Undocumented var",
        Some(EnvName::new("OLD_LEGACY_KEY")),
        vec![envdoctor::Location::new(
            RelPath::new("src/legacy.ts"),
            Span::point(10, 1, 0),
        )],
        None,
    );

    let baseline = BaselineFile {
        schema_version: 1,
        suppressions: vec![BaselineEntry {
            code: "ENV001".into(),
            key: Some("OLD_LEGACY_KEY".into()),
            path: "src/legacy.ts".into(),
        }],
    };

    let filtered = baseline.filter_diagnostics(vec![diag]);
    assert!(filtered.is_empty());

    // Inline suppressions
    assert!(is_line_suppressed("// envdoctor-ignore ENV001", "ENV001"));
    assert!(is_line_suppressed(
        "# envdoctor-ignore ENV001, ENV002",
        "ENV001"
    ));
    assert!(is_line_suppressed("/* envdoctor-ignore-all */", "ENV001"));
    assert!(!is_line_suppressed("// normal comment", "ENV001"));
}

#[test]
fn test_output_formats_rendering() {
    let diag = envdoctor::Diagnostic::new(
        DiagnosticCode::ENV001,
        Severity::Warning,
        "Variable 'FOO' undocumented",
        Some(EnvName::new("FOO")),
        vec![envdoctor::Location::new(
            RelPath::new("src/index.ts"),
            Span::new(5, 10, 50, 53),
        )],
        None,
    );

    let report = Report::new(vec![diag], 1, 0, true);

    // SARIF format
    let sarif = render_sarif(&report);
    assert!(sarif.contains("\"version\": \"2.1.0\""));
    assert!(sarif.contains("ENV001"));

    // JUnit format
    let junit = render_junit(&report);
    assert!(junit.contains("<testsuites"));
    assert!(junit.contains("<failure"));

    // GitHub format
    let github = render_github(&report);
    assert!(github.starts_with("::warning file=src/index.ts,line=5,col=10::ENV001:"));
}

#[test]
fn test_engine_fix_generates_empty_entries_without_leaks() {
    let dir = tempdir().unwrap();
    let contract_path = dir.path().join(".env.example");
    fs::write(&contract_path, "EXISTING_KEY=\n").unwrap();

    let src_dir = dir.path().join("src");
    fs::create_dir(&src_dir).unwrap();
    fs::write(
        src_dir.join("main.py"),
        "import os\nos.environ['NEW_MISSING_VAR']\n",
    )
    .unwrap();

    let config = EngineConfig {
        contract_path: contract_path.clone(),
        scan_paths: vec![src_dir],
        fix: true,
        ..EngineConfig::default()
    };

    let report = run_engine(&config);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == DiagnosticCode::ENV001));

    // Verify file content was appended with empty value
    let updated_contract = fs::read_to_string(&contract_path).unwrap();
    assert!(updated_contract.contains("# Added by envdoctor --fix"));
    assert!(updated_contract.contains("NEW_MISSING_VAR="));
}
