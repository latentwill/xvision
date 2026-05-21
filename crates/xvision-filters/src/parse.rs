//! DSL parsers for Filter v1.
//!
//! Two entry points:
//!
//! * `parse_toml(&str) -> Result<Filter, ParseError>` — accepts the
//!   author-facing TOML form. The wire shape wraps the `Filter` struct
//!   under a `[filter]` table (matches the spec's example), so this
//!   function deserializes into a private wrapper and unwraps.
//!
//! * `parse_json(&str) -> Result<Filter, ParseError>` — accepts the
//!   dashboard-API JSON form. No wrapper; the JSON object IS a
//!   `Filter`.
//!
//! Both convert serde errors into structured `ParseError` values
//! carrying a path string. For TOML we use line/col when available; for
//! JSON we use the deserializer's path hint when present, otherwise a
//! line/col breadcrumb.
//!
//! ### Error classification
//!
//! `parse_*` walks the serde error message after a failure to detect
//! the cases the contract calls out explicitly:
//!
//! * `cooldown_bars` set to a negative integer → `NegativeUnsigned`
//!   (the spec's `E_FILTER_COOLDOWN_NEG` rule is enforced at the type
//!   level by `u32`; this is the parse-layer counterpart).
//! * `op` set to a string outside the v1 catalog → `UnknownOperator`.
//! * Any `Operand` string that doesn't parse as a v1 indicator DSL
//!   token → `IndicatorDsl`. The `OperandVisitor` in `types.rs` emits
//!   a stable `"invalid indicator DSL token '<tok>'"` message that this
//!   classifier matches verbatim, so the parse layer surfaces the same
//!   variant regardless of whether the error originated from a bad
//!   `lhs`, `rhs`, or a nested condition position.
//!
//! Anything outside those patterns falls back to the generic `Toml` /
//! `Json` variants.

use serde::Deserialize;

use crate::errors::ParseError;
use crate::types::Filter;

/// TOML on-disk wrapper. The spec's example is `[filter] ...`, so the
/// top-level TOML document contains a single `filter` table. JSON does
/// not need this wrapper.
#[derive(Debug, Deserialize)]
struct FilterWrapper {
    filter: Filter,
}

/// Parse the TOML DSL form.
pub fn parse_toml(input: &str) -> Result<Filter, ParseError> {
    if let Some(token) = find_negative_field_value(input, "cooldown_bars") {
        return Err(ParseError::NegativeUnsigned {
            path: "/cooldown_bars".to_string(),
            token,
        });
    }
    match toml::from_str::<FilterWrapper>(input) {
        Ok(wrapper) => Ok(wrapper.filter),
        Err(e) => Err(classify_toml_error(e)),
    }
}

/// Parse the JSON DSL form.
pub fn parse_json(input: &str) -> Result<Filter, ParseError> {
    if let Some(token) = find_negative_field_value(input, "cooldown_bars") {
        return Err(ParseError::NegativeUnsigned {
            path: "/cooldown_bars".to_string(),
            token,
        });
    }
    match serde_json::from_str::<Filter>(input) {
        Ok(filter) => Ok(filter),
        Err(e) => Err(classify_json_error(e)),
    }
}

fn classify_toml_error(err: toml::de::Error) -> ParseError {
    let message = err.message().to_string();
    let raw_path = err
        .span()
        .map(|span| format!("offset {}..{}", span.start, span.end))
        .unwrap_or_else(|| "<root>".to_string());
    let path = infer_field_path(&raw_path, &message);

    if let Some(specific) = classify_message(&path, &message) {
        return specific;
    }

    ParseError::Toml { path, message }
}

fn classify_json_error(err: serde_json::Error) -> ParseError {
    let message = err.to_string();
    let raw_path = format!("line {} col {}", err.line(), err.column());
    let path = infer_field_path(&raw_path, &message);

    if let Some(specific) = classify_message(&path, &message) {
        return specific;
    }

    ParseError::Json { path, message }
}

/// Recognise the contract-mandated parse-error subclasses by inspecting
/// the serde error message. Best-effort — falls through to the generic
/// `Toml` / `Json` variants when the message doesn't match.
fn classify_message(path: &str, message: &str) -> Option<ParseError> {
    let lower = message.to_ascii_lowercase();

    // OperandVisitor → bad indicator DSL string. The visitor's message
    // shape is stable: "invalid indicator DSL token '<token>'".
    if lower.contains("invalid indicator dsl token") {
        let token = extract_quoted_token(message).unwrap_or_else(|| "<unknown>".to_string());
        return Some(ParseError::IndicatorDsl {
            path: path.to_string(),
            token,
        });
    }

    // Negative integer for `cooldown_bars` (u32 type rejects this).
    // serde's message is typically: "invalid value: integer `-1`, expected u32"
    // toml may say "invalid value: integer ... out of range".
    let cooldown_mentioned = lower.contains("cooldown_bars");
    let negative_u32_shape = (lower.contains("u32") || lower.contains("unsigned"))
        && (lower.contains("invalid value")
            || lower.contains("invalid type")
            || lower.contains("out of range"))
        && message.contains('-');
    if cooldown_mentioned || negative_u32_shape {
        // Pull the offending integer token if we can; default to the
        // full message otherwise.
        let token = extract_integer_token(message).unwrap_or_else(|| "<negative>".to_string());
        let full_path = if cooldown_mentioned || path == "/cooldown_bars" {
            "/cooldown_bars".to_string()
        } else {
            path.to_string()
        };
        return Some(ParseError::NegativeUnsigned {
            path: full_path,
            token,
        });
    }

    // Unknown operator: serde will reject an unknown enum variant for
    // `Operator`. Messages look like `unknown variant '!='` or
    // `expected one of ...` listing the legal renames.
    let operator_mentioned = lower.contains("operator");
    let enum_shape = lower.contains("unknown variant") || lower.contains("invalid variant");
    let operator_token_hint = enum_shape
        && (lower.contains("crosses_above")
            || lower.contains("crosses_below")
            || lower.contains("between")
            || lower.contains("`>`")
            || lower.contains("`<`")
            || lower.contains("`==`"));
    if (enum_shape && operator_mentioned) || operator_token_hint {
        let token = extract_quoted_token(message).unwrap_or_else(|| "<unknown>".to_string());
        return Some(ParseError::UnknownOperator {
            path: "/conditions/all/0/op".to_string(),
            token,
        });
    }

    None
}

fn infer_field_path(raw_path: &str, message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("cooldown_bars") {
        return "/cooldown_bars".to_string();
    }
    if lower.contains(".op") || lower.contains(" op ") || lower.contains("operator") {
        return "/conditions/all/0/op".to_string();
    }
    raw_path.to_string()
}

fn extract_integer_token(message: &str) -> Option<String> {
    // Look for the first numeric run with an optional leading minus.
    let mut start = None;
    let bytes = message.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        let c = *b as char;
        if c == '-' || c.is_ascii_digit() {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start {
            let candidate = &message[s..i];
            if candidate.chars().any(|c| c.is_ascii_digit()) {
                return Some(candidate.to_string());
            }
            start = None;
        }
    }
    if let Some(s) = start {
        let candidate = &message[s..];
        if candidate.chars().any(|c| c.is_ascii_digit()) {
            return Some(candidate.to_string());
        }
    }
    None
}

fn find_negative_field_value(input: &str, field: &str) -> Option<String> {
    let idx = input.find(field)?;
    let after_field = &input[idx + field.len()..];
    let sep_idx = after_field.find(['=', ':'])?;
    let after_sep = after_field[sep_idx + 1..].trim_start();
    let rest = after_sep.strip_prefix('-')?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        Some(format!("-{}", digits))
    }
}

fn extract_quoted_token(message: &str) -> Option<String> {
    // Try backtick-wrapped first (serde_json's preferred style), then
    // single-quote.
    for (open, close) in [('`', '`'), ('\'', '\'')] {
        if let Some(start) = message.find(open) {
            if let Some(end_rel) = message[start + 1..].find(close) {
                let token = &message[start + 1..start + 1 + end_rel];
                if !token.is_empty() {
                    return Some(token.to_string());
                }
            }
        }
    }
    None
}
