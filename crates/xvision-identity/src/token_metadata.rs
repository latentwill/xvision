//! tokenURI metadata decoding — the read-side twin of [`crate::genart`]'s
//! `generate_token_uri`.
//!
//! Moved here from `xvision-dashboard/src/marketplace_index.rs` (2026-06-11,
//! marketplace real-loop Phase 1 Task 7) so the dashboard indexer and the
//! `xvn marketplace` CLI verbs share one decoder. The dashboard re-imports
//! [`decode_token_metadata`] / [`TokenMetadata`] from this module.

/// Fields extracted from a genart tokenURI's metadata JSON. All fields default
/// to `""` on any decode failure — callers never drop a listing over bad
/// metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenMetadata {
    pub name: String,
    pub agent_id: String,
    pub symmetry: String,
    pub palette: String,
    /// `Density` attribute value, stringified (`""` if absent).
    pub density: String,
    /// `Layers` attribute value, stringified (`""` if absent).
    pub layers: String,
    /// Raw `image` field (`data:image/svg+xml;base64,…` data URI; `""` if
    /// absent). Decode with [`decode_svg_image`].
    pub image: String,
}

/// Lenient mirror of the genart metadata JSON
/// (`generate_token_uri` output: `{name, image, agent_id, attributes}`).
#[derive(serde::Deserialize)]
struct RawMetadata {
    #[serde(default)]
    name: String,
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    attributes: Vec<RawAttribute>,
}

#[derive(serde::Deserialize)]
struct RawAttribute {
    #[serde(default)]
    trait_type: String,
    /// String OR number on the wire (Density/Layers are numeric).
    #[serde(default)]
    value: serde_json::Value,
}

const DATA_URI_PREFIX: &str = "data:application/json;base64,";
const SVG_DATA_URI_PREFIX: &str = "data:image/svg+xml;base64,";

/// Decodes a `data:application/json;base64,…` tokenURI into [`TokenMetadata`].
///
/// Total function: any failure (wrong prefix, bad base64, non-JSON payload,
/// wrong shape) returns the all-empty default. Never panics, never errors.
pub fn decode_token_metadata(token_uri: &str) -> TokenMetadata {
    let Some(b64) = token_uri.strip_prefix(DATA_URI_PREFIX) else {
        return TokenMetadata::default();
    };
    let Some(bytes) = base64_decode(b64) else {
        return TokenMetadata::default();
    };
    let Ok(raw) = serde_json::from_slice::<RawMetadata>(&bytes) else {
        return TokenMetadata::default();
    };

    let mut symmetry = String::new();
    let mut palette = String::new();
    let mut density = String::new();
    let mut layers = String::new();
    for attr in &raw.attributes {
        let value = stringify_attribute(&attr.value);
        match attr.trait_type.as_str() {
            "Symmetry" => symmetry = value,
            "Palette" => palette = value,
            "Density" => density = value,
            "Layers" => layers = value,
            _ => {}
        }
    }

    TokenMetadata {
        name: raw.name,
        agent_id: raw.agent_id,
        symmetry,
        palette,
        density,
        layers,
        image: raw.image,
    }
}

/// Decodes the metadata's `image` field (`data:image/svg+xml;base64,…`)
/// into raw SVG bytes. `None` on wrong prefix or bad base64.
pub fn decode_svg_image(image_data_uri: &str) -> Option<Vec<u8>> {
    base64_decode(image_data_uri.strip_prefix(SVG_DATA_URI_PREFIX)?)
}

/// Stringifies an attribute `value` that may be a JSON string or number.
fn stringify_attribute(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Minimal standard-alphabet base64 decoder. Lenient about padding: trailing
/// `=`/`==` is stripped and unpadded 2- or 3-char trailing groups decode
/// fine; invalid characters and impossible lengths (trailing group of 1 char)
/// are rejected. Local because this crate has no base64 dep (the genart
/// encoder in `genart.rs` is similarly hand-rolled).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes = s.as_bytes();
    let body = match bytes {
        [head @ .., b'=', b'='] => head,
        [head @ .., b'='] => head,
        _ => bytes,
    };
    // 6n bits must cover whole bytes: trailing group of 1 char is impossible.
    if body.len() % 4 == 1 {
        return None;
    }

    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    for chunk in body.chunks(4) {
        let mut acc: u32 = 0;
        for &c in chunk {
            acc = (acc << 6) | val(c)?;
        }
        // Left-align the 6·len bits in a 24-bit window.
        acc <<= 24 - 6 * chunk.len();
        out.push((acc >> 16) as u8);
        if chunk.len() >= 3 {
            out.push((acc >> 8) as u8);
        }
        if chunk.len() == 4 {
            out.push(acc as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_round_trips_real_generate_token_uri_output() {
        let uri = crate::generate_token_uri("01HXTESTAGENT", &"ab".repeat(32)).expect("generate_token_uri");
        let meta = decode_token_metadata(&uri);

        assert_eq!(meta.agent_id, "01HXTESTAGENT");
        assert_eq!(meta.name, "xvn strategy 01HXTEST");
        assert!(!meta.symmetry.is_empty(), "Symmetry attribute must decode");
        assert!(!meta.palette.is_empty(), "Palette attribute must decode");
        assert!(!meta.density.is_empty(), "Density attribute must decode");
        assert!(!meta.layers.is_empty(), "Layers attribute must decode");

        // The image is a data:image/svg+xml;base64 URI wrapping real SVG.
        let svg = decode_svg_image(&meta.image).expect("svg decode");
        let svg_text = String::from_utf8(svg).expect("svg utf8");
        assert!(svg_text.starts_with("<svg"), "decoded image must be SVG");
        assert!(svg_text.ends_with("</svg>"));
    }

    #[test]
    fn decode_empty_string_is_default() {
        assert_eq!(decode_token_metadata(""), TokenMetadata::default());
    }

    #[test]
    fn decode_non_data_uri_is_default() {
        assert_eq!(decode_token_metadata("https://x"), TokenMetadata::default());
    }

    #[test]
    fn decode_bad_base64_is_default() {
        let uri = format!("{DATA_URI_PREFIX}!!!not-base64!!!");
        assert_eq!(decode_token_metadata(&uri), TokenMetadata::default());
    }

    #[test]
    fn decode_valid_base64_non_json_is_default() {
        // "hello world" — valid base64, not JSON.
        let uri = format!("{DATA_URI_PREFIX}aGVsbG8gd29ybGQ=");
        assert_eq!(decode_token_metadata(&uri), TokenMetadata::default());
    }

    #[test]
    fn decode_numeric_attribute_values_are_stringified() {
        let json = r#"{"name":"n","agent_id":"a","attributes":[
            {"trait_type":"Symmetry","value":42},
            {"trait_type":"Palette","value":"Ember"},
            {"trait_type":"Density","value":0.42},
            {"trait_type":"Layers","value":6}
        ]}"#;
        let uri = format!("{DATA_URI_PREFIX}{}", b64(json.as_bytes()));
        let meta = decode_token_metadata(&uri);
        assert_eq!(meta.symmetry, "42");
        assert_eq!(meta.palette, "Ember");
        assert_eq!(meta.density, "0.42");
        assert_eq!(meta.layers, "6");
    }

    #[test]
    fn decode_svg_image_rejects_wrong_prefix() {
        assert_eq!(decode_svg_image("data:application/json;base64,e30="), None);
        assert_eq!(decode_svg_image(""), None);
    }

    /// Test-only encoder so malformed-payload tests don't depend on the
    /// private encoder in `genart.rs`.
    fn b64(data: &[u8]) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let mut acc: u32 = 0;
            for (i, &b) in chunk.iter().enumerate() {
                acc |= u32::from(b) << (16 - 8 * i);
            }
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(CHARS[((acc >> (18 - 6 * i)) & 0x3f) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    #[test]
    fn base64_decode_round_trip() {
        for input in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            assert_eq!(
                base64_decode(&b64(input)).as_deref(),
                Some(input),
                "round trip failed for {input:?}"
            );
        }
    }
}
