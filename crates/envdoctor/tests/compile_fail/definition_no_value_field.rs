use envdoctor::{Definition, EnvName, RelPath, Span, ValueKind};

fn main() {
    let def = Definition::new(
        EnvName::new("API_KEY"),
        RelPath::new(".env.example"),
        Span::new(1, 1, 0, 7),
        ValueKind::NonEmpty,
        false,
    );
    // Compile error: Definition has no field `value` (safe by construction)
    let _ = def.value;
}

