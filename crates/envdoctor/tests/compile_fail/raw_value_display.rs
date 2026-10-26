use envdoctor::RawEnvValue;

fn require_display<T: std::fmt::Display>(_val: &T) {}

fn main() {
    let val = RawEnvValue::new("secret_value", false);
    // Compile error: RawEnvValue does not implement std::fmt::Display
    require_display(&val);
}

