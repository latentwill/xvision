use std::io::Write;

use tempfile::NamedTempFile;
use xvision_engine::autoresearch::config::AutoresearchConfig;

fn write_temp(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

const VALID_TOML: &str = r#"
min_improvement = 0.05

[baseline_untouched_window]
start = "2025-09-01"
end   = "2025-12-01"

[day_window]
start = "2024-01-01"
end   = "2025-09-01"

[mutator]
provider    = "anthropic"
model       = "claude-haiku-4-5"
max_retries = 3
"#;

#[test]
fn valid_toml_parses() {
    let f = write_temp(VALID_TOML);
    let cfg = AutoresearchConfig::from_path(f.path()).expect("should parse valid TOML");
    assert!((cfg.min_improvement - 0.05).abs() < f64::EPSILON);
    assert_eq!(cfg.mutator.provider, "anthropic");
}

#[test]
fn missing_required_field_returns_error() {
    let toml = r#"
[baseline_untouched_window]
start = "2025-09-01"
end   = "2025-12-01"

[day_window]
start = "2024-01-01"
end   = "2025-09-01"

[mutator]
provider    = "anthropic"
model       = "claude-haiku-4-5"
max_retries = 3
"#;
    let f = write_temp(toml);
    assert!(
        AutoresearchConfig::from_path(f.path()).is_err(),
        "missing min_improvement should return an error",
    );
}

#[test]
fn validate_rejects_zero_min_improvement() {
    let toml = r#"
min_improvement = 0.0

[baseline_untouched_window]
start = "2025-09-01"
end   = "2025-12-01"

[day_window]
start = "2024-01-01"
end   = "2025-09-01"

[mutator]
provider    = "anthropic"
model       = "claude-haiku-4-5"
max_retries = 3
"#;
    let f = write_temp(toml);
    let cfg = AutoresearchConfig::from_path(f.path()).expect("zero min_improvement should parse");
    let err = cfg
        .validate()
        .expect_err("validate should reject min_improvement = 0");
    assert!(
        err.to_string().contains("min_improvement"),
        "error should mention min_improvement, got: {err}",
    );
}

#[test]
fn validate_rejects_backwards_windows() {
    let backwards_baseline = r#"
min_improvement = 0.05

[baseline_untouched_window]
start = "2025-12-01"
end   = "2025-09-01"

[day_window]
start = "2024-01-01"
end   = "2025-09-01"

[mutator]
provider    = "anthropic"
model       = "claude-haiku-4-5"
max_retries = 3
"#;
    let f = write_temp(backwards_baseline);
    let cfg = AutoresearchConfig::from_path(f.path()).expect("should parse");
    assert!(
        cfg.validate().is_err(),
        "backwards baseline_untouched_window should be rejected",
    );

    let backwards_day = r#"
min_improvement = 0.05

[baseline_untouched_window]
start = "2025-09-01"
end   = "2025-12-01"

[day_window]
start = "2025-09-01"
end   = "2024-01-01"

[mutator]
provider    = "anthropic"
model       = "claude-haiku-4-5"
max_retries = 3
"#;
    let f2 = write_temp(backwards_day);
    let cfg2 = AutoresearchConfig::from_path(f2.path()).expect("should parse");
    assert!(
        cfg2.validate().is_err(),
        "backwards day_window should be rejected",
    );
}

#[test]
fn validate_rejects_excessive_retries() {
    let toml = r#"
min_improvement = 0.05

[baseline_untouched_window]
start = "2025-09-01"
end   = "2025-12-01"

[day_window]
start = "2024-01-01"
end   = "2025-09-01"

[mutator]
provider    = "anthropic"
model       = "claude-haiku-4-5"
max_retries = 11
"#;
    let f = write_temp(toml);
    let cfg = AutoresearchConfig::from_path(f.path()).expect("should parse");
    let err = cfg
        .validate()
        .expect_err("validate should reject max_retries > 10");
    assert!(
        err.to_string().contains("max_retries"),
        "error should mention max_retries, got: {err}",
    );
}

#[test]
fn validate_rejects_empty_model_or_provider() {
    let empty_model = r#"
min_improvement = 0.05

[baseline_untouched_window]
start = "2025-09-01"
end   = "2025-12-01"

[day_window]
start = "2024-01-01"
end   = "2025-09-01"

[mutator]
provider    = "anthropic"
model       = ""
max_retries = 3
"#;
    let f = write_temp(empty_model);
    let cfg = AutoresearchConfig::from_path(f.path()).expect("should parse");
    assert!(cfg.validate().is_err(), "empty model should be rejected",);

    let empty_provider = r#"
min_improvement = 0.05

[baseline_untouched_window]
start = "2025-09-01"
end   = "2025-12-01"

[day_window]
start = "2024-01-01"
end   = "2025-09-01"

[mutator]
provider    = ""
model       = "claude-haiku-4-5"
max_retries = 3
"#;
    let f2 = write_temp(empty_provider);
    let cfg2 = AutoresearchConfig::from_path(f2.path()).expect("should parse");
    assert!(cfg2.validate().is_err(), "empty provider should be rejected",);
}

#[test]
fn example_file_is_valid() {
    const EXAMPLE: &str = include_str!("../../../config/autoresearch.toml.example");
    let f = write_temp(EXAMPLE);
    let cfg = AutoresearchConfig::from_path(f.path()).expect("example file should parse");
    cfg.validate().expect("example file should pass validation");
}
