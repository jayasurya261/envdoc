use envdoctor::parse;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn test_parser_never_panics_on_arbitrary_strings(input in "\\PC*") {
        let output = parse(&input, ".env");
        // Verify invariant: spans are well-formed
        for def in &output.definitions {
            assert!(def.name.is_valid_identifier());
            assert!(def.span().start_offset <= def.span().end_offset);
        }
        for diag in &output.diagnostics {
            assert!(diag.span.start_offset <= diag.span.end_offset);
        }
    }

    #[test]
    fn test_parser_never_panics_on_random_bytes(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        // Test valid UTF-8 lossy strings (since dialect parser contract is UTF-8 text in memory)
        let lossy = String::from_utf8_lossy(&bytes);
        let output = parse(&lossy, ".env");
        for def in &output.definitions {
            assert!(def.span().start_offset <= def.span().end_offset);
        }
    }

    #[test]
    fn test_valid_generated_assignments(
        key in "[A-Z_][A-Z0-9_]{1,15}",
        val in "[a-zA-Z0-9_ -]{0,30}"
    ) {
        let content = format!("{key}={val}\n");
        let output = parse(&content, ".env");
        assert_eq!(output.definitions.len(), 1);
        assert_eq!(output.definitions[0].name.as_str(), key);
        assert_eq!(output.diagnostics.len(), 0);
    }
}
