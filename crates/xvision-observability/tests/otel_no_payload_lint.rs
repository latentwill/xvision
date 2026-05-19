//! Compile-time guard against payload-string leakage on the public OTel
//! attribute surface.
//!
//! The plan's hard rule: "OTel attributes carry hashes / counts / ids
//! only — never payload strings." This file enforces it at the type
//! level via executable compile-fail tests. If `add_attribute` or
//! [`Attribute`] grows a `From<&str>` / `From<String>` impl in the
//! future, these tests start compiling when they shouldn't and the
//! lint trips.
//!
//! Compiled only with `--features otel` so the `otel` symbols are
//! visible; runs unconditionally on `cargo test --features otel`.

#![cfg(feature = "otel")]

use std::fs;
use std::path::Path;
use std::process::Command;
use xvision_observability::recorder::Attribute;
use xvision_observability::{otel_add_attribute, otel_attr};

/// Positive control: the legitimate path through the API must work.
/// This ensures the `compile_fail` tests below are checking the *right*
/// constraint (i.e. the API actually accepts the valid forms; the only
/// thing rejected is the payload-string path).
#[test]
fn legitimate_attribute_calls_compile_and_run() {
    use tracing::{span, Level};
    let s = span!(Level::INFO, "lint_positive_control");
    otel_add_attribute(&s, otel_attr::RUN_ID, Attribute::id("run_abc"));
    otel_add_attribute(&s, otel_attr::MODEL_PROMPT_HASH, Attribute::hash("sha256:00"));
    otel_add_attribute(&s, otel_attr::MODEL_INPUT_TOKENS, Attribute::count(42));
    otel_add_attribute(&s, otel_attr::TOOL_REQUIRES_APPROVAL, Attribute::flag(false));
}

/// Runtime reflection: confirm every variant of `Attribute` matches one
/// of the four allowed shapes. If a new variant lands that carries an
/// unrestricted `String` (i.e. a payload field), this match becomes
/// non-exhaustive at compile time and the lint trips that way.
#[test]
fn attribute_enum_only_carries_safe_shapes() {
    let cases = [
        Attribute::Hash("h".into()),
        Attribute::Id("i".into()),
        Attribute::Count(0),
        Attribute::Flag(false),
    ];
    for c in cases {
        match c {
            Attribute::Hash(_) | Attribute::Id(_) | Attribute::Count(_) | Attribute::Flag(_) => {
                // ok — every variant is one of the four allowed shapes
            }
        }
    }
}

const UI_POSITIVE_CONTROL: &str = r#"
use xvision_observability::recorder::Attribute;
use xvision_observability::{otel_add_attribute, otel_attr};

fn main() {
    otel_add_attribute(todo!(), otel_attr::RUN_ID, Attribute::id("run_abc"));
}
"#;

#[test]
fn raw_payload_string_compile_failures_are_executed() {
    let temp = tempfile::tempdir().expect("create temporary UI test crate");
    let project_dir = temp.path();
    fs::create_dir(project_dir.join("src")).expect("create UI test src dir");
    fs::write(
        project_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "xvision-observability-ui-test"
version = "0.0.0"
edition = "2021"

[dependencies]
xvision-observability = {{ path = {}, features = ["otel"] }}
"#,
            toml_path(Path::new(env!("CARGO_MANIFEST_DIR")))
        ),
    )
    .expect("write UI test manifest");

    assert_ui_case(project_dir, "positive_control", UI_POSITIVE_CONTROL, true);

    for (name, source) in [
        ("attribute_from_str", include_str!("ui/attribute_from_str.rs")),
        (
            "attribute_from_string",
            include_str!("ui/attribute_from_string.rs"),
        ),
        (
            "otel_add_attribute_str",
            include_str!("ui/otel_add_attribute_str.rs"),
        ),
        (
            "otel_add_attribute_string",
            include_str!("ui/otel_add_attribute_string.rs"),
        ),
    ] {
        assert_ui_case(project_dir, name, source, false);
    }
}

fn assert_ui_case(project_dir: &Path, name: &str, source: &str, should_compile: bool) {
    fs::write(project_dir.join("src/main.rs"), source)
        .unwrap_or_else(|err| panic!("write UI test fixture {name}: {err}"));

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .arg("check")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(project_dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", project_dir.join("target"))
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .unwrap_or_else(|err| panic!("run cargo check for UI test fixture {name}: {err}"));

    if output.status.success() != should_compile {
        panic!(
            "UI test fixture {name} {} unexpectedly\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            if should_compile {
                "failed to compile"
            } else {
                "compiled"
            },
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

fn toml_path(path: &Path) -> String {
    let path = path
        .canonicalize()
        .unwrap_or_else(|err| panic!("canonicalize {}: {err}", path.display()));
    let escaped = path
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Belt-and-braces: coerce `otel_add_attribute` into a typed
/// `fn(&Span, &'static str, Attribute)` pointer. If the value parameter
/// were ever loosened to accept `&str` / `String` / `impl Into<…>`,
/// this coercion would fail at compile time — `&str` and `Attribute`
/// have different ABIs, so the function-pointer type cannot widen
/// silently.
#[test]
fn add_attribute_signature_takes_attribute_not_str() {
    // The coercion itself is the assertion: if this line stops
    // compiling, someone changed the OTel attribute API to accept a
    // non-`Attribute` value and the lint trips.
    let _coerced: fn(&tracing::Span, &'static str, Attribute) = otel_add_attribute;
}
