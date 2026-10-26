use envdoctor::{
    Definition, EnvName, EvidenceKind, Location, RawEnvValue, Reference, RelPath, Span, ValueKind,
};

#[test]
fn test_rel_path_normalization() {
    let p1 = RelPath::new("src\\model\\mod.rs");
    let p2 = RelPath::new("src/model/mod.rs");
    let p3 = RelPath::new("./src/model/mod.rs");
    let p4 = RelPath::new(".\\src\\model\\mod.rs");

    assert_eq!(p1, p2);
    assert_eq!(p2, p3);
    assert_eq!(p3, p4);
    assert_eq!(p1.as_str(), "src/model/mod.rs");

    // Serialization produces forward slashes
    let json = serde_json::to_string(&p1).unwrap();
    assert_eq!(json, "\"src/model/mod.rs\"");

    let deserialized: RelPath = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, p1);
}

#[test]
fn test_span_ordering_and_metrics() {
    let s1 = Span::new(1, 5, 4, 10);
    let s2 = Span::new(2, 1, 15, 20);
    let s3 = Span::new(1, 10, 9, 15);

    assert!(s1 < s3);
    assert!(s3 < s2);
    assert_eq!(s1.offset(), 4);
    assert_eq!(s1.len(), 6);
    assert!(!s1.is_empty());

    let point = Span::point(5, 12, 80);
    assert!(point.is_empty());
    assert_eq!(point.len(), 0);
    assert_eq!(point.offset(), 80);

    let loc = Location::new("src/main.rs", s1);
    assert_eq!(loc.path.as_str(), "src/main.rs");
    assert_eq!(loc.to_string(), "src/main.rs:1:5");
}

#[test]
fn test_env_name_validation() {
    let valid_names = ["API_KEY", "PORT", "_PRIVATE", "DB_HOST_2", "a"];
    for name in valid_names {
        assert!(
            EnvName::new(name).is_valid_identifier(),
            "Expected {name} to be valid"
        );
    }

    let invalid_names = ["", "123PORT", "API-KEY", "API KEY", "DB.HOST", "FOO$BAR"];
    for name in invalid_names {
        assert!(
            !EnvName::new(name).is_valid_identifier(),
            "Expected {name} to be invalid"
        );
    }
}

#[test]
fn test_value_kind_properties() {
    assert!(ValueKind::Empty.is_empty());
    assert!(!ValueKind::NonEmpty.is_empty());
    assert!(ValueKind::MultiLine.is_multiline());

    assert_eq!(ValueKind::Empty.as_str(), "empty");
    assert_eq!(ValueKind::NonEmpty.as_str(), "non-empty");
    assert_eq!(ValueKind::MultiLine.as_str(), "multi-line");

    assert_eq!(
        serde_json::to_string(&ValueKind::Empty).unwrap(),
        "\"empty\""
    );
    assert_eq!(
        serde_json::to_string(&ValueKind::NonEmpty).unwrap(),
        "\"non-empty\""
    );
    assert_eq!(
        serde_json::to_string(&ValueKind::MultiLine).unwrap(),
        "\"multi-line\""
    );
}

#[test]
fn test_evidence_kind_variants() {
    let kinds = [
        (EvidenceKind::DirectMember, "direct-member"),
        (EvidenceKind::BracketLiteral, "bracket-literal"),
        (EvidenceKind::Destructured, "destructured"),
        (EvidenceKind::DynamicUnresolved, "dynamic-unresolved"),
        (EvidenceKind::WholeObjectEscape, "whole-object-escape"),
    ];

    for (kind, expected_str) in kinds {
        assert_eq!(kind.as_str(), expected_str);
        assert_eq!(kind.to_string(), expected_str);
        let serialized = serde_json::to_string(&kind).unwrap();
        assert_eq!(serialized, format!("\"{expected_str}\""));
        let deserialized: EvidenceKind = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, kind);
    }
}

#[test]
fn test_definition_and_reference_models() {
    let def = Definition::new(
        "DATABASE_URL",
        ".env.example",
        Span::new(3, 1, 30, 42),
        ValueKind::NonEmpty,
        false,
    );

    assert_eq!(def.name.as_str(), "DATABASE_URL");
    assert_eq!(def.path().as_str(), ".env.example");
    assert_eq!(def.span().line, 3);
    assert_eq!(def.value_kind, ValueKind::NonEmpty);
    assert!(!def.is_optional);

    let reference = Reference::new(
        "DATABASE_URL",
        "src/db/connection.ts",
        Span::new(14, 25, 450, 462),
        EvidenceKind::Destructured,
    );

    assert_eq!(reference.name.as_str(), "DATABASE_URL");
    assert_eq!(reference.path().as_str(), "src/db/connection.ts");
    assert_eq!(reference.span().line, 14);
    assert_eq!(reference.evidence_kind, EvidenceKind::Destructured);

    // Verify JSON serialization does not contain any value field
    let def_json = serde_json::to_string_pretty(&def).unwrap();
    assert!(!def_json.contains("\"value\":"));
    let ref_json = serde_json::to_string_pretty(&reference).unwrap();
    assert!(!ref_json.contains("\"value\":"));
}

#[test]
fn test_raw_env_value_redaction_lifecycle() {
    let raw_empty = RawEnvValue::new("", false);
    assert!(raw_empty.is_empty());
    assert_eq!(raw_empty.into_kind(), ValueKind::Empty);

    let raw_secret = RawEnvValue::new("super_secret_token_12345", false);
    assert_eq!(raw_secret.byte_len(), 24);
    assert_eq!(raw_secret.classify(), ValueKind::NonEmpty);
    assert_eq!(raw_secret.into_kind(), ValueKind::NonEmpty);

    let raw_multiline = RawEnvValue::new("line1\nline2\nline3", true);
    assert_eq!(raw_multiline.into_kind(), ValueKind::MultiLine);
}
