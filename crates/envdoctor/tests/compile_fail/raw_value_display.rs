use envdoctor::RawEnvValue;

fn main() {
    let val = RawEnvValue::new("secret_value", false);
    // Compile error: RawEnvValue does not implement std::fmt::Display
    println!("{}", val);
}

