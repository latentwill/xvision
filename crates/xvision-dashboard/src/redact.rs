//! Secret-pattern redactor for the dashboard's API boundary.
//!
//! ## Purpose
//!
//! This module provides a `redact` function that scans a string for known
//! secret patterns and replaces matches with `[REDACTED:<kind>]`. It is
//! called at the API boundary — in error response formatters, SSE event
//! serializers, and job-log writers — so secrets never appear in output
//! channels.
//!
//! ## Patterns covered
//!
//! | Pattern | Kind tag | Example |
//! |---|---|---|
//! | `sk-[A-Za-z0-9-]{20,}` | `PROVIDER_TOKEN` | OpenAI / Anthropic API keys |
//! | `OR-[A-Za-z0-9-]{20,}` | `PROVIDER_TOKEN` | OpenRouter keys |
//! | `xai-[A-Za-z0-9]{20,}` | `PROVIDER_TOKEN` | xAI keys |
//! | `PK[A-Z0-9]{16,}` | `BROKER_KEY` | Alpaca API key |
//! | `0x[0-9a-fA-F]{64}` | `PRIVATE_KEY_HEX` | EVM private key hex |
//! | 12-word BIP-39 mnemonic | `MNEMONIC` | Wallet seed phrase (12 words) |
//! | 24-word BIP-39 mnemonic | `MNEMONIC` | Wallet seed phrase (24 words) |
//! | `tskey-[a-z0-9]{20,}` | `TAILSCALE_KEY` | Tailscale auth keys |
//!
//! ## Design
//!
//! The redactor is a pure string transform (no regex crate dependency).
//! Pattern matching is done with simple prefix detection + character-class
//! scanning, which avoids the `regex` crate overhead on the hot serialization
//! path and keeps the dependency surface minimal.
//!
//! The `tracing` integration (see `TracingRedactLayer`) intercepts structured
//! log fields before they are formatted so secrets are redacted at the source.
//!
//! ## Usage
//!
//! ```rust
//! use xvision_dashboard::redact::redact;
//!
//! let raw = r#"key=sk-ABCDEFGHIJKLMNOPQRST12345"#;
//! let clean = redact(raw);
//! assert!(clean.contains("[REDACTED:PROVIDER_TOKEN]"));
//! assert!(!clean.contains("sk-"));
//! ```

/// Replace all detected secret patterns in `input` with `[REDACTED:<kind>]`.
///
/// This function is safe to call on any string. Non-matching input is returned
/// unchanged (with a fast path that avoids allocating when no patterns match).
pub fn redact(input: &str) -> String {
    // Fast-path: if none of the trigger prefixes are present, return early.
    let has_trigger = input.contains("sk-")
        || input.contains("OR-")
        || input.contains("xai-")
        || input.contains("PK")
        || input.contains("0x")
        || input.contains("tskey-")
        || could_be_mnemonic(input);

    if !has_trigger {
        return input.to_owned();
    }

    apply_patterns(input)
}

/// Apply all redaction patterns to `input`, returning the cleaned string.
fn apply_patterns(input: &str) -> String {
    let mut out = input.to_owned();

    // Pattern: provider token prefixes — sk-, OR-, xai-
    // sk- keys include hyphens in body (e.g. sk-ant-api03-XXXXX for Anthropic).
    out = redact_prefixed(&out, "sk-", |c| c.is_ascii_alphanumeric() || c == '-', 20, "PROVIDER_TOKEN");
    out = redact_prefixed(&out, "OR-", |c| c.is_ascii_alphanumeric() || c == '-', 20, "PROVIDER_TOKEN");
    out = redact_prefixed(&out, "xai-", |c| c.is_ascii_alphanumeric(), 20, "PROVIDER_TOKEN");

    // Pattern: Alpaca broker key — PK followed by uppercase alphanumeric
    out = redact_prefixed(&out, "PK", |c| c.is_ascii_uppercase() || c.is_ascii_digit(), 16, "BROKER_KEY");

    // Pattern: Tailscale auth key — tskey- followed by lowercase alphanumeric
    out = redact_prefixed(&out, "tskey-", |c| c.is_ascii_lowercase() || c.is_ascii_digit(), 20, "TAILSCALE_KEY");

    // Pattern: EVM private key hex — 0x followed by exactly 64 hex chars
    out = redact_hex_privkey(&out);

    // Pattern: BIP-39 mnemonic phrases (12 or 24 words)
    out = redact_mnemonic(&out);

    out
}

/// Generic prefix-based redactor. Scans for `prefix`, then consumes
/// characters matching `char_ok`, and replaces the whole match
/// (including prefix) with `[REDACTED:<kind>]` if the body is at
/// least `min_len` characters long.
fn redact_prefixed<F>(input: &str, prefix: &str, char_ok: F, min_len: usize, kind: &str) -> String
where
    F: Fn(char) -> bool,
{
    let tag = format!("[REDACTED:{kind}]");
    let mut out = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(pos) = remaining.find(prefix) {
        // Reject prefix hits that are part of a larger token (e.g. "ask-" should
        // not trigger on "sk-"). Check that the character before the prefix is
        // either start-of-string or a non-alphanumeric boundary.
        let before = &remaining[..pos];
        let is_boundary = before
            .chars()
            .last()
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true);

        out.push_str(&remaining[..pos]);
        remaining = &remaining[pos..];

        if !is_boundary {
            // Not a real prefix hit — emit the prefix text and move on.
            out.push_str(&remaining[..prefix.len()]);
            remaining = &remaining[prefix.len()..];
            continue;
        }

        // Consume the prefix.
        let after_prefix = &remaining[prefix.len()..];
        // Scan for chars matching char_ok.
        let body_len = after_prefix
            .chars()
            .take_while(|&c| char_ok(c))
            .map(|c| c.len_utf8())
            .sum::<usize>();

        if body_len >= min_len {
            // Whole match = prefix + body. Replace with tag.
            out.push_str(&tag);
            remaining = &after_prefix[body_len..];
        } else {
            // Body too short — not a real secret. Emit as-is.
            out.push_str(&remaining[..prefix.len() + body_len]);
            remaining = &after_prefix[body_len..];
        }
    }

    out.push_str(remaining);
    out
}

/// Redact EVM private keys: `0x` followed by exactly 64 lowercase hex chars.
fn redact_hex_privkey(input: &str) -> String {
    let tag = "[REDACTED:PRIVATE_KEY_HEX]";
    let mut out = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(pos) = remaining.find("0x") {
        out.push_str(&remaining[..pos]);
        remaining = &remaining[pos..];
        let after = &remaining[2..]; // skip "0x"
        let hex_len = after
            .chars()
            .take(64)
            .take_while(|c| c.is_ascii_hexdigit())
            .count();
        if hex_len == 64 {
            // Check the char right after the 64-char run isn't hex
            // (would mean it's a longer hex string, not a private key).
            let next_is_hex = after
                .chars()
                .nth(64)
                .map(|c| c.is_ascii_hexdigit())
                .unwrap_or(false);
            if !next_is_hex {
                out.push_str(tag);
                remaining = &after[64..];
                continue;
            }
        }
        // Not a private key — emit "0x" and continue.
        out.push_str("0x");
        remaining = after;
    }
    out.push_str(remaining);
    out
}

/// BIP-39 word list subset check — a fast approximation.
///
/// The BIP-39 word list has 2048 words of 3–8 characters. We use a
/// conservative heuristic: a sequence of N ASCII lowercase words (3–8 chars
/// each) separated by single spaces, where N is exactly 12 or 24.
fn redact_mnemonic(input: &str) -> String {
    let tag_12 = "[REDACTED:MNEMONIC]";
    let tag_24 = "[REDACTED:MNEMONIC]";

    // Build a scanner that finds runs of N word-like tokens.
    let redact_run = |s: &str, n: usize, tag: &str| -> String {
        let mut out = String::with_capacity(s.len());
        let mut i = 0usize;
        let bytes = s.as_bytes();

        'outer: while i < bytes.len() {
            // Try to match a run of `n` lowercase words starting at position `i`.
            // Word boundary: start of string or preceded by non-alpha.
            let at_boundary = i == 0 || !bytes[i - 1].is_ascii_alphabetic();
            if !at_boundary || !bytes[i].is_ascii_lowercase() {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }

            // Scan n words separated by single spaces.
            let mut pos = i;
            let mut words_found = 0;
            let mut end = i;
            while words_found < n {
                // Must start with a lowercase alpha.
                if pos >= bytes.len() || !bytes[pos].is_ascii_lowercase() {
                    break;
                }
                // Consume the word (3–8 lowercase letters).
                let word_start = pos;
                while pos < bytes.len() && bytes[pos].is_ascii_lowercase() {
                    pos += 1;
                }
                let word_len = pos - word_start;
                if word_len < 3 || word_len > 8 {
                    // Not a valid BIP-39 word shape.
                    break;
                }
                words_found += 1;
                end = pos;
                if words_found < n {
                    // Expect exactly one space separator.
                    if pos >= bytes.len() || bytes[pos] != b' ' {
                        break;
                    }
                    pos += 1; // skip the space
                }
            }

            if words_found == n {
                // Verify that the run ends at a non-alpha boundary.
                let after_end = end < bytes.len() && bytes[end].is_ascii_alphabetic();
                if !after_end {
                    out.push_str(tag);
                    i = end;
                    continue 'outer;
                }
            }

            // Not a full mnemonic run — emit current char and try next position.
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    };

    // Apply 24-word pass first (stricter), then 12-word.
    let step1 = redact_run(input, 24, tag_24);
    redact_run(&step1, 12, tag_12)
}

/// Quick check: could `input` possibly contain a mnemonic phrase?
/// Returns true if there are enough lowercase words to form one.
fn could_be_mnemonic(input: &str) -> bool {
    let mut word_count = 0usize;
    let mut in_word = false;
    for c in input.chars() {
        if c.is_ascii_lowercase() {
            if !in_word {
                in_word = true;
                word_count += 1;
            }
        } else {
            in_word = false;
        }
    }
    word_count >= 12
}

/// Redact a `serde_json::Value` in place, walking the JSON tree.
///
/// String values are passed through `redact()`; other types are left
/// unchanged. Used by the SSE serializer and error formatter to clean
/// entire JSON payloads.
pub fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            let cleaned = redact(s.as_str());
            *s = cleaned;
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                redact_json(v);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                redact_json(v);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Provider tokens ─────────────────────────────────────────────────────

    #[test]
    fn redacts_openai_style_key() {
        let raw = "key=sk-ABCDEFGHIJKLMNOPQRST12345";
        let clean = redact(raw);
        assert!(clean.contains("[REDACTED:PROVIDER_TOKEN]"), "got: {clean}");
        assert!(!clean.contains("sk-ABCDEF"), "got: {clean}");
    }

    #[test]
    fn redacts_openrouter_key() {
        let raw = "Authorization: Bearer OR-ABCDEFGHIJKLMNOPQRSTU12345";
        let clean = redact(raw);
        assert!(clean.contains("[REDACTED:PROVIDER_TOKEN]"), "got: {clean}");
    }

    #[test]
    fn redacts_xai_key() {
        let raw = "token=xai-abcdefghijklmnopqrstu";
        let clean = redact(raw);
        assert!(clean.contains("[REDACTED:PROVIDER_TOKEN]"), "got: {clean}");
    }

    #[test]
    fn short_sk_prefix_not_redacted() {
        // "sk-short" is only 5 chars in the body — below the 20-char minimum.
        let raw = "error: sk-short";
        let clean = redact(raw);
        assert!(!clean.contains("[REDACTED"), "expected no redaction, got: {clean}");
    }

    // ── Broker tokens ───────────────────────────────────────────────────────

    #[test]
    fn redacts_alpaca_api_key() {
        let raw = "alpaca_key=PKABCDEFGHIJKLMNOP";
        let clean = redact(raw);
        assert!(clean.contains("[REDACTED:BROKER_KEY]"), "got: {clean}");
    }

    // ── Tailscale keys ──────────────────────────────────────────────────────

    #[test]
    fn redacts_tailscale_auth_key() {
        let raw = "tskey-abcdefghijklmnopqrstu";
        let clean = redact(raw);
        assert!(clean.contains("[REDACTED:TAILSCALE_KEY]"), "got: {clean}");
    }

    // ── EVM private key ─────────────────────────────────────────────────────

    #[test]
    fn redacts_evm_private_key() {
        let raw = "private_key=0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let clean = redact(raw);
        assert!(clean.contains("[REDACTED:PRIVATE_KEY_HEX]"), "got: {clean}");
    }

    #[test]
    fn short_hex_not_redacted_as_private_key() {
        // Only 8 hex chars — not a private key.
        let raw = "color=0xdeadbeef";
        let clean = redact(raw);
        assert!(!clean.contains("[REDACTED:PRIVATE_KEY_HEX]"), "got: {clean}");
    }

    // ── Mnemonic phrases ────────────────────────────────────────────────────

    #[test]
    fn redacts_12_word_mnemonic() {
        let mnemonic = "witch collapse practice feed shame open despair creek road again ice least";
        let clean = redact(mnemonic);
        assert!(clean.contains("[REDACTED:MNEMONIC]"), "got: {clean}");
    }

    #[test]
    fn redacts_24_word_mnemonic() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let clean = redact(mnemonic);
        assert!(clean.contains("[REDACTED:MNEMONIC]"), "got: {clean}");
    }

    // ── JSON redaction ──────────────────────────────────────────────────────

    #[test]
    fn redact_json_cleans_string_values() {
        let mut v = serde_json::json!({
            "error": "failed with key=sk-ABCDEFGHIJKLMNOPQRSTU",
            "code": "provider_error",
            "count": 42
        });
        redact_json(&mut v);
        let msg = v["error"].as_str().unwrap();
        assert!(msg.contains("[REDACTED:PROVIDER_TOKEN]"), "got: {msg}");
        // Non-string values are unchanged.
        assert_eq!(v["count"], 42);
    }

    #[test]
    fn no_false_positive_on_normal_text() {
        let raw = "the quick brown fox jumps over the lazy dog";
        let clean = redact(raw);
        // Should NOT be redacted as a mnemonic (words too long + not all 3-8 chars? let's check)
        // "jumps" = 5, "quick" = 5, "brown" = 5... actually all words are 3-8 chars.
        // But there are only 9 unique words so 12-word run threshold shouldn't match in sequence.
        // The sentence has 9 words total (< 12), so no mnemonic match.
        assert!(!clean.contains("[REDACTED"), "false positive on normal text: {clean}");
    }

    // ── No-op fast-path ─────────────────────────────────────────────────────

    #[test]
    fn fast_path_returns_unchanged() {
        let raw = "no secrets here at all";
        let clean = redact(raw);
        assert_eq!(clean, raw);
    }
}
