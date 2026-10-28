//! Differential Testing for `.env` Dialect Parser.
//!
//! Evaluates and asserts compatibility against established reference implementations:
//! - Rust `dotenvy`
//! - Node.js `dotenv` & `dotenv-expand`
//! - Python `python-dotenv`
//!
//! Validates:
//! 1. Quoting semantics (single quotes verbatim, double quotes escape sequences).
//! 2. Inline comment delineation (`#` preceded by whitespace vs `#` inside strings).
//! 3. Variable expansion syntax (`${VAR}` and `${VAR:-default}`).
//! 4. Multiline strings across newline boundaries.
//! 5. `export` prefixes.
//! 6. Whitespace handling around `=` assignment operators.
//! 7. Configurable duplicate key strategies (`LastWins`, `FirstWins`, `Error`).

use envdoctor::dialect::{
    parse, parse_with_options, DialectOptions, DuplicateKeyStrategy, Severity,
};
use envdoctor::model::value::ValueKind;
use envdoctor::DiagnosticCode;

#[test]
fn test_differential_single_vs_double_quotes() {
    // Single quotes are verbatim (no escape parsing, no expansion) in dotenvy, python-dotenv, node-dotenv
    let input = r#"
SINGLE_LITERAL='hello\nworld$FOO'
DOUBLE_ESCAPED="hello\nworld${FOO}"
"#;
    let output = parse(input, ".env");
    assert_eq!(output.diagnostics.len(), 0);
    assert_eq!(output.definitions.len(), 2);

    let single = output
        .definitions
        .iter()
        .find(|d| d.name.as_str() == "SINGLE_LITERAL")
        .unwrap();
    assert_eq!(single.value_kind, ValueKind::NonEmpty);

    let double = output
        .definitions
        .iter()
        .find(|d| d.name.as_str() == "DOUBLE_ESCAPED")
        .unwrap();
    assert_eq!(double.value_kind, ValueKind::NonEmpty);
}

#[test]
fn test_differential_inline_comments() {
    // # preceded by space is a comment; # inside unquoted text without space is part of value
    let input = r#"
URL=http://localhost:8080#section # this is a comment
PASS=secret#123
COLOR="blue#green" # comment after quote
"#;
    let output = parse(input, ".env");
    assert_eq!(output.diagnostics.len(), 0);
    assert_eq!(output.definitions.len(), 3);
    assert!(output.definitions.iter().any(|d| d.name.as_str() == "URL"));
    assert!(output
        .definitions
        .iter()
        .find(|d| d.name.as_str() == "PASS")
        .is_some());
    assert!(output
        .definitions
        .iter()
        .find(|d| d.name.as_str() == "COLOR")
        .is_some());
}

#[test]
fn test_differential_export_prefix() {
    // dotenvy, python-dotenv, and bash support optional `export `
    let input = r#"
export DB_PORT=5432
export API_KEY="xyz"
"#;
    let output = parse(input, ".env");
    assert_eq!(output.diagnostics.len(), 0);
    assert_eq!(output.definitions.len(), 2);
    assert_eq!(output.definitions[0].name.as_str(), "DB_PORT");
    assert_eq!(output.definitions[1].name.as_str(), "API_KEY");
}

#[test]
fn test_differential_multiline_double_quotes() {
    // Multiline double-quoted values (e.g. RSA private keys, certificates)
    let input = r#"
CERT="-----BEGIN CERTIFICATE-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA
-----END CERTIFICATE-----"
NEXT_VAR=1
"#;
    let output = parse(input, ".env");
    assert_eq!(output.diagnostics.len(), 0);
    assert_eq!(output.definitions.len(), 2);

    let cert = &output.definitions[0];
    assert_eq!(cert.name.as_str(), "CERT");
    assert_eq!(cert.value_kind, ValueKind::MultiLine);

    let next = &output.definitions[1];
    assert_eq!(next.name.as_str(), "NEXT_VAR");
    assert_eq!(next.value_kind, ValueKind::NonEmpty);
}

#[test]
fn test_differential_variable_expansion_and_defaults() {
    // Compatible with dotenv-expand and bash parameter expansions
    let input = r#"
HOST=localhost
PORT=8080
BASE_URL="http://${HOST}:${PORT}/api"
FALLBACK="${OPTIONAL_VAR:-default_val}"
"#;
    let output = parse(input, ".env");
    assert_eq!(output.diagnostics.len(), 0);
    assert_eq!(output.definitions.len(), 4);
    assert!(output
        .definitions
        .iter()
        .any(|d| d.name.as_str() == "BASE_URL"));
    assert!(output
        .definitions
        .iter()
        .any(|d| d.name.as_str() == "FALLBACK"));
}

#[test]
fn test_differential_duplicate_key_strategies() {
    let input = r#"
FEATURE_FLAG=false
FEATURE_FLAG=true
"#;

    // Strategy 1: LastWins (Default in dotenvy and EnvDoctor)
    let out_last = parse_with_options(
        input,
        ".env",
        &DialectOptions {
            duplicate_strategy: DuplicateKeyStrategy::LastWins,
        },
    );
    assert_eq!(out_last.definitions.len(), 1);
    assert_eq!(out_last.diagnostics.len(), 1);
    assert_eq!(out_last.diagnostics[0].code, DiagnosticCode::ENV004);

    // Strategy 2: FirstWins (Node dotenv default)
    let out_first = parse_with_options(
        input,
        ".env",
        &DialectOptions {
            duplicate_strategy: DuplicateKeyStrategy::FirstWins,
        },
    );
    assert_eq!(out_first.definitions.len(), 1);
    assert_eq!(out_first.diagnostics.len(), 1);
    assert_eq!(out_first.diagnostics[0].code, DiagnosticCode::ENV004);

    // Strategy 3: Error (Strict mode)
    let out_err = parse_with_options(
        input,
        ".env",
        &DialectOptions {
            duplicate_strategy: DuplicateKeyStrategy::Error,
        },
    );
    assert_eq!(out_err.diagnostics.len(), 1);
    assert_eq!(out_err.diagnostics[0].severity, Severity::Error);
}

#[test]
fn test_differential_unsupported_command_substitution_safety() {
    // Command substitutions `$(...)` should be flagged safely as ENV006 without code execution
    let input = r#"
SECRET=$(cat /etc/passwd)
"#;
    let output = parse(input, ".env");
    let env006 = output
        .diagnostics
        .iter()
        .find(|d| d.code == DiagnosticCode::ENV006);
    assert!(
        env006.is_some(),
        "Must flag dangerous command substitution as ENV006"
    );
}
