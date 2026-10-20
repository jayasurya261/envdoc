use envdoctor::{
    evaluate_contract, AuditOptions, Contract, Definition, Diagnostic, DiagnosticCode,
    EvidenceKind, Location, Reference, Report, Severity, Span, ValueKind,
};

#[test]
fn test_rule1_undocumented_reference_env001() {
    let contract_def = Definition::new(
        "DATABASE_URL",
        ".env.example",
        Span::new(1, 1, 0, 12),
        ValueKind::NonEmpty,
        false,
    );
    let contract = Contract::new(".env.example", vec![contract_def]);

    let ref_documented = Reference::new(
        "DATABASE_URL",
        "src/db.ts",
        Span::new(10, 5, 100, 112),
        EvidenceKind::DirectMember,
    );
    let ref_undocumented = Reference::new(
        "STRIPE_KEY",
        "src/payment.ts",
        Span::new(25, 12, 350, 360),
        EvidenceKind::Destructured,
    );

    let diags = evaluate_contract(
        Some(&contract),
        None,
        &[ref_documented, ref_undocumented],
        &AuditOptions::default(),
    );

    let env001: Vec<_> = diags
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV001)
        .collect();
    assert_eq!(env001.len(), 1);
    assert_eq!(env001[0].key.as_ref().unwrap().as_str(), "STRIPE_KEY");
    assert_eq!(env001[0].severity, Severity::Warning);
    assert_eq!(env001[0].evidence, Some(EvidenceKind::Destructured));
    assert_eq!(env001[0].locations[0].path.as_str(), "src/payment.ts");
}

#[test]
fn test_rule2_unused_definition_env002() {
    let def_used = Definition::new(
        "PORT",
        ".env.example",
        Span::new(1, 1, 0, 4),
        ValueKind::NonEmpty,
        false,
    );
    let def_unused = Definition::new(
        "DEPRECATED_VAR",
        ".env.example",
        Span::new(2, 1, 15, 29),
        ValueKind::NonEmpty,
        false,
    );
    let contract = Contract::new(".env.example", vec![def_used, def_unused]);

    let reference = Reference::new(
        "PORT",
        "src/main.ts",
        Span::new(5, 1, 40, 44),
        EvidenceKind::DirectMember,
    );

    let diags = evaluate_contract(
        Some(&contract),
        None,
        &[reference],
        &AuditOptions::default(),
    );

    let env002: Vec<_> = diags
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV002)
        .collect();
    assert_eq!(env002.len(), 1);
    assert_eq!(env002[0].key.as_ref().unwrap().as_str(), "DEPRECATED_VAR");
    assert_eq!(env002[0].severity, Severity::Note);
}

#[test]
fn test_rule3_missing_in_env_env003_required_and_optional() {
    let def_required = Definition::new(
        "DB_PASS",
        ".env.example",
        Span::new(1, 1, 0, 7),
        ValueKind::NonEmpty,
        false,
    );
    let def_optional = Definition::new(
        "DEBUG_LOGS",
        ".env.example",
        Span::new(2, 1, 15, 25),
        ValueKind::NonEmpty,
        true, // Optional
    );
    let contract = Contract::new(".env.example", vec![def_required, def_optional]);

    // Active .env has neither variable
    let env_defs: Vec<Definition> = vec![];

    let diags = evaluate_contract(
        Some(&contract),
        Some(&env_defs),
        &[],
        &AuditOptions::default(),
    );

    let env003: Vec<_> = diags
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV003)
        .collect();
    assert_eq!(env003.len(), 2);

    let req_diag = env003
        .iter()
        .find(|d| d.key.as_ref().unwrap().as_str() == "DB_PASS")
        .unwrap();
    assert_eq!(req_diag.severity, Severity::Error); // Required is Error

    let opt_diag = env003
        .iter()
        .find(|d| d.key.as_ref().unwrap().as_str() == "DEBUG_LOGS")
        .unwrap();
    assert_eq!(opt_diag.severity, Severity::Note); // Optional is downgraded to Note
}

#[test]
fn test_rule4_empty_required_value_env007() {
    let def_required = Definition::new(
        "SECRET_KEY",
        ".env.example",
        Span::new(1, 1, 0, 10),
        ValueKind::NonEmpty,
        false,
    );
    let def_optional = Definition::new(
        "OPTIONAL_VAR",
        ".env.example",
        Span::new(2, 1, 15, 27),
        ValueKind::NonEmpty,
        true,
    );
    let contract = Contract::new(".env.example", vec![def_required, def_optional]);

    // .env provides both with Empty value
    let env_defs = vec![
        Definition::new(
            "SECRET_KEY",
            ".env",
            Span::new(1, 1, 0, 11),
            ValueKind::Empty,
            false,
        ),
        Definition::new(
            "OPTIONAL_VAR",
            ".env",
            Span::new(2, 1, 15, 28),
            ValueKind::Empty,
            false,
        ),
    ];

    let diags = evaluate_contract(
        Some(&contract),
        Some(&env_defs),
        &[],
        &AuditOptions::default(),
    );

    let env007: Vec<_> = diags
        .iter()
        .filter(|d| d.code == DiagnosticCode::ENV007)
        .collect();
    // Only SECRET_KEY should trigger ENV007 because it is required
    assert_eq!(env007.len(), 1);
    assert_eq!(env007[0].key.as_ref().unwrap().as_str(), "SECRET_KEY");
    assert_eq!(env007[0].severity, Severity::Warning);
}

#[test]
fn test_rule5_contract_missing_and_empty() {
    // Missing contract -> ENV020
    let diags_missing = evaluate_contract(None, None, &[], &AuditOptions::default());
    assert_eq!(diags_missing.len(), 1);
    assert_eq!(diags_missing[0].code, DiagnosticCode::ENV020);
    assert_eq!(diags_missing[0].severity, Severity::Error);

    // Empty contract -> ENV021
    let empty_contract = Contract::new(".env.example", vec![]);
    let diags_empty =
        evaluate_contract(Some(&empty_contract), None, &[], &AuditOptions::default());
    assert_eq!(diags_empty.len(), 1);
    assert_eq!(diags_empty[0].code, DiagnosticCode::ENV021);
    assert_eq!(diags_empty[0].severity, Severity::Warning);
}

#[test]
fn test_deterministic_sorting() {
    let d1 = Diagnostic::new(
        DiagnosticCode::ENV001,
        Severity::Warning,
        "msg",
        Some("B_KEY".into()),
        vec![Location::new("src/z.ts", Span::new(20, 1, 0, 0))],
        None,
    );
    let d2 = Diagnostic::new(
        DiagnosticCode::ENV001,
        Severity::Warning,
        "msg",
        Some("A_KEY".into()),
        vec![Location::new("src/a.ts", Span::new(10, 5, 0, 0))],
        None,
    );
    let d3 = Diagnostic::new(
        DiagnosticCode::ENV002,
        Severity::Note,
        "msg",
        Some("A_KEY".into()),
        vec![Location::new("src/a.ts", Span::new(10, 1, 0, 0))],
        None,
    );

    let mut list = vec![d1, d2, d3];
    Diagnostic::sort_diagnostics(&mut list);

    // Sorted by path (src/a.ts before src/z.ts), then line, then col
    assert_eq!(list[0].locations[0].path.as_str(), "src/a.ts");
    assert_eq!(list[0].locations[0].span.column, 1);
    assert_eq!(list[1].locations[0].span.column, 5);
    assert_eq!(list[2].locations[0].path.as_str(), "src/z.ts");
}

#[test]
fn test_report_schema_conformance() {
    let diag = Diagnostic::new(
        DiagnosticCode::ENV001,
        Severity::Warning,
        "Missing documentation",
        Some("PORT".into()),
        vec![Location::new("src/index.ts", Span::new(5, 10, 50, 54))],
        Some(EvidenceKind::DirectMember),
    );

    let report = Report::new(vec![diag], 10, 0, true);
    let json_str = serde_json::to_string_pretty(&report).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    // Verify root fields
    assert_eq!(val["schema_version"], 1);
    assert_eq!(val["tool_name"], "envdoctor");
    assert!(val["tool_version"].is_string());

    // Verify summary object
    assert_eq!(val["summary"]["scanned_files"], 10);
    assert_eq!(val["summary"]["skipped_files"], 0);
    assert_eq!(val["summary"]["total_warnings"], 1);
    assert_eq!(val["summary"]["total_errors"], 0);
    assert_eq!(val["summary"]["total_notes"], 0);
    assert_eq!(val["summary"]["complete"], true);

    // Verify diagnostics array
    let diags_arr = val["diagnostics"].as_array().unwrap();
    assert_eq!(diags_arr.len(), 1);
    assert_eq!(diags_arr[0]["code"], "ENV001");
    assert_eq!(diags_arr[0]["severity"], "warning");
    assert_eq!(diags_arr[0]["key"], "PORT");
    assert_eq!(diags_arr[0]["evidence"], "direct-member");
    assert_eq!(diags_arr[0]["locations"][0]["path"], "src/index.ts");
    assert_eq!(diags_arr[0]["locations"][0]["span"]["line"], 5);
    assert_eq!(diags_arr[0]["locations"][0]["span"]["column"], 10);
}
