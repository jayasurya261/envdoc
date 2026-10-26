#[test]
fn test_compile_fail_leak_prevention() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
