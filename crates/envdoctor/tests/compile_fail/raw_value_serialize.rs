use envdoctor::RawEnvValue;

fn main() {
    let val = RawEnvValue::new("secret_value", false);
    // Compile error: RawEnvValue does not implement serde::Serialize
    let _ = serde_json::to_string(&val);
}

