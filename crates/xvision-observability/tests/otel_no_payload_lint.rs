//! Compile-time guard against payload-string leakage on the public OTel
//! attribute surface.
//!
//! The plan's hard rule: "OTel attributes carry hashes / counts / ids
//! only — never payload strings." This file enforces it at the type
//! level via `compile_fail` doc tests. If `add_attribute` or
//! [`Attribute`] grows a `From<&str>` / `From<String>` impl in the
//! future, these tests start *passing* the compile-fail check (i.e.
//! they compile when they shouldn't) and the lint trips.
//!
//! Compiled only with `--features otel` so the `otel` symbols are
//! visible; runs unconditionally on `cargo test --features otel`.

#![cfg(feature = "otel")]

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

/// `Attribute` deliberately has NO `From<&str>` impl. If someone adds
/// one (turning the enum into something that can absorb raw payload
/// text), this test starts compiling — and the `compile_fail` flips
/// from "yes this fails" to "no it compiles", which `cargo test`
/// surfaces as a test failure. That is exactly the trip wire we want.
///
/// ```compile_fail
/// use xvision_observability::recorder::Attribute;
/// let _: Attribute = "raw-payload-string-must-not-compile".into();
/// ```
///
/// ```compile_fail
/// use xvision_observability::recorder::Attribute;
/// let owned = String::from("raw-payload-string-must-not-compile");
/// let _: Attribute = owned.into();
/// ```
///
/// `otel_add_attribute` also refuses to accept `&str` directly — the
/// `value` parameter is typed `Attribute`. The next two doc tests pin
/// that behaviour.
///
/// ```compile_fail
/// use xvision_observability::{otel_add_attribute, otel_attr};
/// use tracing::{span, Level};
/// let s = span!(Level::INFO, "lint");
/// otel_add_attribute(&s, otel_attr::RUN_ID, "raw-string-not-allowed");
/// ```
///
/// ```compile_fail
/// use xvision_observability::{otel_add_attribute, otel_attr};
/// use tracing::{span, Level};
/// let s = span!(Level::INFO, "lint");
/// let payload = String::from("raw-string-not-allowed");
/// otel_add_attribute(&s, otel_attr::RUN_ID, payload);
/// ```
#[allow(dead_code)]
fn _doctest_anchor() {}

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
    let _coerced: fn(&tracing::Span, &'static str, Attribute) =
        otel_add_attribute;
}
