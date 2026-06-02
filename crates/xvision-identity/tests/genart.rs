use xvision_identity::{generate_svg, generate_token_uri};

const AGENT_A: &str = "01HWTEST0000000000000001AB";
const AGENT_B: &str = "01HWTEST0000000000000002CD";
const HASH_A: &str  = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

fn b64_decode(s: &str) -> Vec<u8> {
    fn val(c: u8) -> u32 {
        match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    }
    let raw: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
    let n = raw.len();
    let full = n / 4;
    let tail = n % 4;
    let cap = full * 3 + if tail > 0 { tail - 1 } else { 0 };
    let mut out = Vec::with_capacity(cap);
    for i in 0..full {
        let v = (val(raw[i*4]) << 18) | (val(raw[i*4+1]) << 12)
              | (val(raw[i*4+2]) << 6) | val(raw[i*4+3]);
        out.push((v >> 16) as u8);
        out.push((v >> 8) as u8);
        out.push(v as u8);
    }
    if tail >= 2 {
        let a = val(raw[full*4]);
        let b = val(raw[full*4+1]);
        let combined = (a << 18) | (b << 12);
        out.push((combined >> 16) as u8);
        if tail == 3 {
            let c = val(raw[full*4+2]);
            out.push(((combined | (c << 6)) >> 8) as u8);
        }
    }
    out
}

#[test]
fn genart_deterministic() {
    let svg1 = generate_svg(AGENT_A, HASH_A);
    let svg2 = generate_svg(AGENT_A, HASH_A);
    assert_eq!(svg1, svg2);
}

#[test]
fn genart_distinct_per_agent() {
    let hash_b = "b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3";
    let svg_a = generate_svg(AGENT_A, HASH_A);
    let svg_b = generate_svg(AGENT_B, hash_b);
    assert_ne!(svg_a, svg_b);
}

#[test]
fn genart_lineage_coherent() {
    // Two agent_ids differing only in last 4 chars share the same background fill.
    let agent_x = "01HWTEST0000000000000001AB";
    let agent_y = "01HWTEST000000000000ZZZZ";
    let svg_x = generate_svg(agent_x, HASH_A);
    let svg_y = generate_svg(agent_y, HASH_A);
    assert!(svg_x.contains(r##"fill="#0a0a0f""##));
    assert!(svg_y.contains(r##"fill="#0a0a0f""##));
}

#[test]
fn genart_token_uri_is_base64_json() {
    let uri = generate_token_uri(AGENT_A, HASH_A);
    assert!(uri.starts_with("data:application/json;base64,"));
    let b64_part = uri.trim_start_matches("data:application/json;base64,");
    let decoded = b64_decode(b64_part);
    let json_str = std::str::from_utf8(&decoded).expect("valid utf8");
    let v: serde_json::Value = serde_json::from_str(json_str).expect("valid json");
    assert!(v.get("name").is_some(), "missing 'name'");
    assert!(v.get("image").is_some(), "missing 'image'");
    assert!(v.get("agent_id").is_some(), "missing 'agent_id'");
}

#[test]
fn genart_svg_valid_structure() {
    let svg = generate_svg(AGENT_A, HASH_A);
    assert!(svg.contains("<svg"), "missing <svg");
    assert!(svg.contains("</svg>"), "missing </svg>");
    assert!(svg.contains("viewBox"), "missing viewBox");
}
