use envdoctor::{EnvName, EvidenceKind, Reference, RelPath, Span};

fn main() {
    let reference = Reference::new(
        EnvName::new("API_KEY"),
        RelPath::new("src/index.ts"),
        Span::new(1, 1, 0, 7),
        EvidenceKind::DirectMember,
    );
    // Compile error: Reference has no field `value` (safe by construction)
    let _ = reference.value;
}

