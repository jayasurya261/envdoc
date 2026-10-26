use envdoctor::{scan_source, DiagnosticCode, EvidenceKind, Severity};

#[test]
fn test_member_expressions_and_optional_chaining() {
    let source = r#"
        const nodeEnv = process.env.NODE_ENV;
        const apiPort = process.env.PORT;
        const viteApi = import.meta.env.VITE_API_URL;
        const optionalVal = process.env?.OPTIONAL_FLAG;
        const optMeta = import.meta.env?.OPTIONAL_META;
    "#;

    let result = scan_source(source, "src/index.ts");
    assert_eq!(result.diagnostics.len(), 0);
    assert_eq!(result.references.len(), 5);

    assert_eq!(result.references[0].name.as_str(), "NODE_ENV");
    assert_eq!(
        result.references[0].evidence_kind,
        EvidenceKind::DirectMember
    );

    assert_eq!(result.references[1].name.as_str(), "PORT");
    assert_eq!(
        result.references[1].evidence_kind,
        EvidenceKind::DirectMember
    );

    assert_eq!(result.references[2].name.as_str(), "VITE_API_URL");
    assert_eq!(
        result.references[2].evidence_kind,
        EvidenceKind::DirectMember
    );

    assert_eq!(result.references[3].name.as_str(), "OPTIONAL_FLAG");
    assert_eq!(
        result.references[3].evidence_kind,
        EvidenceKind::DirectMember
    );

    assert_eq!(result.references[4].name.as_str(), "OPTIONAL_META");
    assert_eq!(
        result.references[4].evidence_kind,
        EvidenceKind::DirectMember
    );
}

#[test]
fn test_bracket_access_with_literals() {
    let source = r#"
        const a = process.env['SINGLE_QUOTED'];
        const b = process.env["DOUBLE_QUOTED"];
        const c = process.env[`TEMPLATE_LITERAL`];
    "#;

    let result = scan_source(source, "src/config.js");
    assert_eq!(result.diagnostics.len(), 0);
    assert_eq!(result.references.len(), 3);

    assert_eq!(result.references[0].name.as_str(), "SINGLE_QUOTED");
    assert_eq!(
        result.references[0].evidence_kind,
        EvidenceKind::BracketLiteral
    );

    assert_eq!(result.references[1].name.as_str(), "DOUBLE_QUOTED");
    assert_eq!(
        result.references[1].evidence_kind,
        EvidenceKind::BracketLiteral
    );

    assert_eq!(result.references[2].name.as_str(), "TEMPLATE_LITERAL");
    assert_eq!(
        result.references[2].evidence_kind,
        EvidenceKind::BracketLiteral
    );
}

#[test]
fn test_destructuring_and_rest_pattern() {
    let source = r#"
        const { DB_HOST, DB_PORT: port, DB_USER = 'admin' } = process.env;
        const { SECRET_KEY, ...restOfEnv } = process.env;
    "#;

    let result = scan_source(source, "src/db.ts");
    // DB_HOST, DB_PORT, DB_USER, SECRET_KEY = 4 references
    assert_eq!(result.references.len(), 4);
    for r in &result.references {
        assert_eq!(r.evidence_kind, EvidenceKind::Destructured);
    }
    assert_eq!(result.references[0].name.as_str(), "DB_HOST");
    assert_eq!(result.references[1].name.as_str(), "DB_PORT");
    assert_eq!(result.references[2].name.as_str(), "DB_USER");
    assert_eq!(result.references[3].name.as_str(), "SECRET_KEY");

    // Rest pattern triggers ENV012
    let env012: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV012)
        .collect();
    assert_eq!(env012.len(), 1);
    assert_eq!(env012[0].severity, Severity::Note);
}

#[test]
fn test_dynamic_bracket_access_env010() {
    let source = r#"
        const key = 'DYNAMIC_KEY';
        const val1 = process.env[key];
        const val2 = process.env[getDynamicKey()];
        const val3 = process.env[`PREFIX_${key}`];
    "#;

    let result = scan_source(source, "src/dynamic.ts");
    assert_eq!(result.references.len(), 0);

    let env010: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV010)
        .collect();
    assert_eq!(env010.len(), 3);
    for diag in env010 {
        assert_eq!(diag.severity, Severity::Note);
    }
}

#[test]
fn test_whole_object_aliasing_and_escapes_env012() {
    let source = r#"
        const envCopy = process.env;
        const spreadObj = { ...process.env };
        someFunction(process.env);
        function getEnv() {
            return process.env;
        }
    "#;

    let result = scan_source(source, "src/escape.ts");
    assert_eq!(result.references.len(), 0);

    let env012: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV012)
        .collect();
    assert_eq!(env012.len(), 4);
    for diag in env012 {
        assert_eq!(diag.code, DiagnosticCode::ENV012);
        assert_eq!(diag.severity, Severity::Note);
    }
}

#[test]
fn test_string_and_comment_shielding() {
    let source = r#"
        // Comment mentioning process.env.FAKE_VAR_1
        /* Multi-line comment
           process.env.FAKE_VAR_2
        */
        const text1 = "process.env.FAKE_VAR_3";
        const text2 = 'process.env.FAKE_VAR_4';
        const text3 = `process.env.FAKE_VAR_5`;

        // Real reference
        const real = process.env.REAL_VAR;
    "#;

    let result = scan_source(source, "src/shield.ts");
    assert_eq!(result.diagnostics.len(), 0);
    assert_eq!(result.references.len(), 1);
    assert_eq!(result.references[0].name.as_str(), "REAL_VAR");
}

#[test]
fn test_parse_error_recovery_env011() {
    // Contains invalid syntax: should emit ENV011 without crashing
    let source = r#"
        const first = process.env.FIRST_VAR;
        function invalidSyntax( {
        const second = process.env.SECOND_VAR;
    "#;

    let result = scan_source(source, "src/broken.ts");
    // ENV011 should be emitted for the parse problem
    let env011: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV011)
        .collect();
    assert!(!env011.is_empty(), "Expected ENV011 on syntax error");
    assert_eq!(env011[0].severity, Severity::Warning);
    assert_eq!(env011[0].locations[0].path.as_str(), "src/broken.ts");
}
