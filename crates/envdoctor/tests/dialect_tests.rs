use envdoctor::{parse, DiagnosticCode, Severity, ValueKind};

#[test]
fn test_basic_assignments() {
    let input = r#"
# Standard assignment
DATABASE_URL=postgres://user:pass@localhost:5432/db
PORT=8080

# With export
export SECRET_KEY=my_secret_key
  export   TIMEOUT=30  
"#;

    let output = parse(input, ".env.example");
    assert_eq!(output.diagnostics.len(), 0);
    assert_eq!(output.definitions.len(), 4);

    assert_eq!(output.definitions[0].name.as_str(), "DATABASE_URL");
    assert_eq!(output.definitions[0].value_kind, ValueKind::NonEmpty);
    assert!(!output.definitions[0].is_optional);

    assert_eq!(output.definitions[1].name.as_str(), "PORT");
    assert_eq!(output.definitions[2].name.as_str(), "SECRET_KEY");
    assert_eq!(output.definitions[3].name.as_str(), "TIMEOUT");
}

#[test]
fn test_empty_values() {
    let input = r#"
EMPTY_1=
EMPTY_2=""
EMPTY_3=''
"#;

    let output = parse(input, ".env");
    assert_eq!(output.diagnostics.len(), 0);
    assert_eq!(output.definitions.len(), 3);

    for def in &output.definitions {
        assert_eq!(def.value_kind, ValueKind::Empty);
    }
}

#[test]
fn test_comments_and_hashes() {
    let input = r#"
# Leading whole-line comment
# Another comment line
API_URL=https://api.example.com#anchor # this is an inline comment
QUOTED_HASH="https://example.com#section" # inline comment after quote
SINGLE_HASH='https://example.com#section' # inline comment after quote
"#;

    let output = parse(input, ".env");
    assert_eq!(output.diagnostics.len(), 0);
    assert_eq!(output.definitions.len(), 3);
    assert_eq!(output.definitions[0].name.as_str(), "API_URL");
    assert_eq!(output.definitions[0].value_kind, ValueKind::NonEmpty);
    assert_eq!(output.definitions[1].name.as_str(), "QUOTED_HASH");
    assert_eq!(output.definitions[2].name.as_str(), "SINGLE_HASH");
}

#[test]
fn test_optional_doc_comments() {
    let input = r#"
# Standard required var
DATABASE_URL=postgres://localhost/db

# @optional
DEBUG_LOG=true

#@optional: Used for tracing metrics
METRICS_PORT=9090

# Blank line resets optional
# Just a normal comment
PRODUCTION=true
"#;

    let output = parse(input, ".env.example");
    assert_eq!(output.diagnostics.len(), 0);
    assert_eq!(output.definitions.len(), 4);

    assert!(!output.definitions[0].is_optional);
    assert_eq!(output.definitions[0].name.as_str(), "DATABASE_URL");

    assert!(output.definitions[1].is_optional);
    assert_eq!(output.definitions[1].name.as_str(), "DEBUG_LOG");

    assert!(output.definitions[2].is_optional);
    assert_eq!(output.definitions[2].name.as_str(), "METRICS_PORT");

    assert!(!output.definitions[3].is_optional);
    assert_eq!(output.definitions[3].name.as_str(), "PRODUCTION");
}

#[test]
fn test_multiline_values() {
    let input = r#"
PRIVATE_KEY="-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEA0Y...
-----END RSA PRIVATE KEY-----"

MULTI_SINGLE='First line
Second line
Third line'
"#;

    let output = parse(input, ".env");
    assert_eq!(output.diagnostics.len(), 0);
    assert_eq!(output.definitions.len(), 2);

    assert_eq!(output.definitions[0].name.as_str(), "PRIVATE_KEY");
    assert_eq!(output.definitions[0].value_kind, ValueKind::MultiLine);

    assert_eq!(output.definitions[1].name.as_str(), "MULTI_SINGLE");
    assert_eq!(output.definitions[1].value_kind, ValueKind::MultiLine);
}

#[test]
fn test_unsupported_syntax_detection_env006() {
    let input = r#"
VAR_EXP=Hello $USER
BRACED_EXP=Hello ${USER}
CMD_SUB=Branch is $(git rev-parse --abbrev-ref HEAD)
QUOTED_EXP="Welcome $USER"
QUOTED_CMD="Commit is $(git rev-parse HEAD)"
ESCAPED_DOLLAR="Price is \$100"
SINGLE_QUOTED='Literal $USER and $(cmd)'
"#;

    let output = parse(input, ".env");
    let codes: Vec<DiagnosticCode> = output.diagnostics.iter().map(|d| d.code).collect();

    // The first 5 should trigger ENV006; the last 2 should not
    assert_eq!(
        codes,
        vec![
            DiagnosticCode::ENV006,
            DiagnosticCode::ENV006,
            DiagnosticCode::ENV006,
            DiagnosticCode::ENV006,
            DiagnosticCode::ENV006,
        ]
    );

    for diag in &output.diagnostics {
        assert_eq!(diag.severity, Severity::Warning);
    }

    // All definitions are still recorded despite warnings
    assert_eq!(output.definitions.len(), 7);
}

#[test]
fn test_malformed_syntax_errors_env005() {
    let inputs = [
        "123INVALID_START=true",
        "DASH-NOT-ALLOWED=true",
        "DOT.NOT.ALLOWED=true",
        "=MISSING_KEY",
        "GARBAGE_WITHOUT_EQUALS",
        "UNCLOSED_DOUBLE=\"oops",
        "UNCLOSED_SINGLE='oops",
        "EXTRA_AFTER_QUOTE=\"valid\"extra_chars",
    ];

    for input in inputs {
        let output = parse(input, ".env");
        assert_eq!(
            output.definitions.len(),
            0,
            "Expected 0 definitions for invalid input: {input}"
        );
        assert!(
            output
                .diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::ENV005),
            "Expected ENV005 diagnostic for invalid input: {input}"
        );
        assert_eq!(output.diagnostics[0].severity, Severity::Error);
    }
}

#[test]
fn test_duplicate_keys_env004_last_definition_wins() {
    let input = r#"
PORT=3000
PORT=8080
"#;

    let output = parse(input, ".env");
    assert_eq!(output.diagnostics.len(), 1);
    assert_eq!(output.diagnostics[0].code, DiagnosticCode::ENV004);
    assert_eq!(output.diagnostics[0].severity, Severity::Warning);
    assert_eq!(output.diagnostics[0].key.as_ref().unwrap().as_str(), "PORT");

    // Only 1 definition remains, and it reflects the last occurrence (line 3)
    assert_eq!(output.definitions.len(), 1);
    assert_eq!(output.definitions[0].name.as_str(), "PORT");
    assert_eq!(output.definitions[0].span().line, 3);
}

#[test]
fn test_zero_leakage_in_diagnostics_and_definitions() {
    let input = r#"
SUPER_SECRET_KEY="my-top-secret-token-xyz"
INVALID-SECRET="super-secret-canary-12345"
UNSUPPORTED_EXP="secret-$CANARY_VAR"
"#;

    let output = parse(input, ".env");
    let json_defs = serde_json::to_string(&output.definitions).unwrap();
    let json_diags = serde_json::to_string(&output.diagnostics).unwrap();

    assert!(!json_defs.contains("my-top-secret-token-xyz"));
    assert!(!json_defs.contains("super-secret-canary-12345"));
    assert!(!json_defs.contains("secret-"));

    assert!(!json_diags.contains("my-top-secret-token-xyz"));
    assert!(!json_diags.contains("super-secret-canary-12345"));
}
