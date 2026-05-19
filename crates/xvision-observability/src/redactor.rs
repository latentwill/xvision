//! Secret-pattern redactor v1.
//!
//! Allowlist-driven regex pass. Designed for the `RetentionMode::Redacted`
//! path: take a prompt / response / tool input string, strip recognizable
//! secrets, return a `RedactedString` plus a list of `RedactionMatch`es so
//! callers can record metrics on what fired (without recording the secret).
//!
//! Patterns covered in v1: AWS access keys, Anthropic / OpenAI API keys,
//! Alpaca / Orderly API keys, JWTs, hex private keys (64-char hex), BIP-39
//! mnemonic phrases. The list is intentionally narrow — this is not a
//! general PII scrubber. Extending it is fine; just keep each pattern
//! independent so a regex that hangs on input doesn't take the others
//! down with it.

use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionMatch {
    pub pattern: &'static str,
    pub span: (usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactedString {
    pub text: String,
    pub matches: Vec<RedactionMatch>,
}

impl RedactedString {
    pub fn untouched(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            matches: vec![],
        }
    }
}

/// Stateless redactor. Construct once (cheap — patterns are lazy-compiled
/// into a static `OnceLock`), call `redact(&str)` per payload.
#[derive(Debug, Default, Clone, Copy)]
pub struct Redactor;

impl Redactor {
    pub fn new() -> Self {
        Self
    }

    pub fn redact(&self, input: &str) -> RedactedString {
        let mut matches: Vec<RedactionMatch> = vec![];
        for pat in patterns() {
            for m in pat.regex.find_iter(input) {
                matches.push(RedactionMatch {
                    pattern: pat.name,
                    span: (m.start(), m.end()),
                });
            }
        }
        if matches.is_empty() {
            return RedactedString::untouched(input);
        }

        // Sort by start ascending so we walk the input once. Overlapping
        // matches are coalesced into the outer one — the outer match wins.
        matches.sort_by_key(|m| m.span.0);
        let mut coalesced: Vec<RedactionMatch> = vec![];
        for m in matches {
            if let Some(last) = coalesced.last_mut() {
                if m.span.0 < last.span.1 {
                    // Overlap. Extend the outer match if needed.
                    if m.span.1 > last.span.1 {
                        last.span.1 = m.span.1;
                    }
                    continue;
                }
            }
            coalesced.push(m);
        }

        let mut out = String::with_capacity(input.len());
        let mut cursor = 0usize;
        for m in &coalesced {
            out.push_str(&input[cursor..m.span.0]);
            out.push_str(&format!("[redacted:{}]", m.pattern));
            cursor = m.span.1;
        }
        out.push_str(&input[cursor..]);

        RedactedString {
            text: out,
            matches: coalesced,
        }
    }
}

struct Pattern {
    name: &'static str,
    regex: Regex,
}

fn patterns() -> &'static [Pattern] {
    static PATTERNS: OnceLock<Vec<Pattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            // AWS access key id — `AKIA` + 16 base32 chars.
            Pattern {
                name: "aws_access_key",
                regex: Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap(),
            },
            // Anthropic API key — `sk-ant-` prefix + base64-ish body.
            Pattern {
                name: "anthropic_api_key",
                regex: Regex::new(r"\bsk-ant-[A-Za-z0-9\-_]{20,}\b").unwrap(),
            },
            // OpenAI API key — `sk-` prefix + 32+ base64-ish chars. Does
            // not collide with sk-ant-* because we order anthropic first
            // in coalescing.
            Pattern {
                name: "openai_api_key",
                regex: Regex::new(r"\bsk-[A-Za-z0-9]{32,}\b").unwrap(),
            },
            // Alpaca API key id — `PK` or `AK` + 16+ alnum (paper vs live).
            Pattern {
                name: "alpaca_api_key",
                regex: Regex::new(r"\b(?:PK|AK)[A-Z0-9]{16,}\b").unwrap(),
            },
            // Orderly API key — `ed25519:` prefix + base58-ish body.
            Pattern {
                name: "orderly_api_key",
                regex: Regex::new(r"\bed25519:[A-Za-z0-9]{30,}\b").unwrap(),
            },
            // JWT — three base64-url segments separated by `.`. Reject
            // common false positives like long version strings by demanding
            // each segment be at least 10 chars (typical JWT header alone
            // is 30+).
            Pattern {
                name: "jwt",
                regex: Regex::new(r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b")
                    .unwrap(),
            },
            // 64-char hex private key (Ethereum / generic secp256k1).
            // Conservative: require the canonical 64-hex shape with word
            // boundaries so we don't redact e.g. random uuids.
            Pattern {
                name: "hex_private_key",
                regex: Regex::new(r"\b(?:0x)?[a-fA-F0-9]{64}\b").unwrap(),
            },
            // BIP-39 mnemonic — 12 or 24 lowercase words separated by
            // single spaces. We don't actually validate against the
            // wordlist (v1 scope); a 12/24-word lowercase pattern catches
            // the format and keeps false-positive rate acceptable.
            Pattern {
                name: "mnemonic_phrase",
                regex: Regex::new(r"\b(?:[a-z]{3,8} ){11,23}[a-z]{3,8}\b").unwrap(),
            },
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untouched_when_no_secrets() {
        let r = Redactor::new();
        let out = r.redact("hello world, nothing interesting here");
        assert_eq!(out.text, "hello world, nothing interesting here");
        assert!(out.matches.is_empty());
    }

    #[test]
    fn redacts_anthropic_key() {
        let r = Redactor::new();
        let input = "key=sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
        let out = r.redact(input);
        assert!(
            out.text.contains("[redacted:anthropic_api_key]"),
            "got: {}",
            out.text
        );
        assert_eq!(out.matches.len(), 1);
        assert_eq!(out.matches[0].pattern, "anthropic_api_key");
    }

    #[test]
    fn redacts_openai_key() {
        let r = Redactor::new();
        let input = "OPENAI_API_KEY=sk-abcdEFGH1234567890ABCDEFGHIJ1234";
        let out = r.redact(input);
        assert!(
            out.text.contains("[redacted:openai_api_key]"),
            "got: {}",
            out.text
        );
    }

    #[test]
    fn redacts_aws_key() {
        let r = Redactor::new();
        let out = r.redact("AKIAIOSFODNN7EXAMPLE in this string");
        assert!(
            out.text.contains("[redacted:aws_access_key]"),
            "got: {}",
            out.text
        );
    }

    #[test]
    fn redacts_alpaca_key() {
        let r = Redactor::new();
        let out = r.redact("APCA-API-KEY-ID: PKABCDEF1234567890XY");
        assert!(
            out.text.contains("[redacted:alpaca_api_key]"),
            "got: {}",
            out.text
        );
    }

    #[test]
    fn redacts_orderly_key() {
        let r = Redactor::new();
        let out = r.redact("orderly_key=ed25519:6KSi7BSqsNuTUyA4LBKj9X8AhVgi6gT9HpsMxNGmH5RR");
        assert!(
            out.text.contains("[redacted:orderly_api_key]"),
            "got: {}",
            out.text
        );
    }

    #[test]
    fn redacts_jwt() {
        let r = Redactor::new();
        // Header.Payload.Signature shape, all base64url.
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let out = r.redact(&format!("token={jwt}"));
        assert!(out.text.contains("[redacted:jwt]"), "got: {}", out.text);
    }

    #[test]
    fn redacts_hex_private_key() {
        let r = Redactor::new();
        let key = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
        let out = r.redact(&format!("private_key=0x{key}"));
        assert!(
            out.text.contains("[redacted:hex_private_key]"),
            "got: {}",
            out.text
        );
    }

    #[test]
    fn redacts_mnemonic() {
        let r = Redactor::new();
        let input = "seed: abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let out = r.redact(input);
        assert!(
            out.text.contains("[redacted:mnemonic_phrase]"),
            "got: {}",
            out.text
        );
    }

    #[test]
    fn multiple_secrets_in_one_string() {
        let r = Redactor::new();
        let input = "ANTHROPIC=sk-ant-abcdefghijklmnopqrstuvwxyz0123456789 AWS=AKIAIOSFODNN7EXAMPLE";
        let out = r.redact(input);
        assert_eq!(out.matches.len(), 2);
        assert!(out.text.contains("[redacted:anthropic_api_key]"));
        assert!(out.text.contains("[redacted:aws_access_key]"));
    }
}
