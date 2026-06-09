use std::fmt::Write as FmtWrite;

fn fnv_bytes(input: &[u8]) -> [u8; 32] {
    const SEEDS: [u64; 4] = [
        0xcbf2_9ce4_8422_2325,
        0x1465_0fb0_739d_0383,
        0x9368_da02_daeb_19d6,
        0x5ada_82de_37be_73d1,
    ];
    const PRIME: u64 = 0x100000001b3;
    let mut result = [0u8; 32];
    for i in 0..4 {
        let mut h: u64 = SEEDS[i];
        for &b in input {
            h ^= b as u64;
            h = h.wrapping_mul(PRIME);
        }
        h ^= (i as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
        result[i * 8..(i + 1) * 8].copy_from_slice(&h.to_le_bytes());
    }
    result
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex_decode_32(s: &str) -> Option<[u8; 32]> {
    let bytes = s.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let hi = hex_nibble(bytes[i * 2])?;
        let lo = hex_nibble(bytes[i * 2 + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hsl_to_hex(hue: f64, sat: f64, light: f64) -> String {
    let s = sat / 100.0;
    let l = light / 100.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = if hue < 60.0 {
        (c, x, 0.0)
    } else if hue < 120.0 {
        (x, c, 0.0)
    } else if hue < 180.0 {
        (0.0, c, x)
    } else if hue < 240.0 {
        (0.0, x, c)
    } else if hue < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let to_byte = |v: f64| ((v + m).clamp(0.0, 1.0) * 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", to_byte(r1), to_byte(g1), to_byte(b1))
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let n = data.len();
    let full = n / 3;
    let rem = n % 3;
    let mut out = String::with_capacity((full + usize::from(rem != 0)) * 4);
    for i in 0..full {
        let b = ((data[i * 3] as u32) << 16) | ((data[i * 3 + 1] as u32) << 8) | (data[i * 3 + 2] as u32);
        out.push(CHARS[((b >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((b >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((b >> 6) & 0x3f) as usize] as char);
        out.push(CHARS[(b & 0x3f) as usize] as char);
    }
    if rem == 1 {
        let b = (data[full * 3] as u32) << 16;
        out.push(CHARS[((b >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((b >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let b = ((data[full * 3] as u32) << 16) | ((data[full * 3 + 1] as u32) << 8);
        out.push(CHARS[((b >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((b >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((b >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}

/// Generates a deterministic SVG string from `agent_id` and `manifest_hash`.
///
/// `manifest_hash` must be a 64-char lowercase hex string (32-byte hash).
/// If it cannot be decoded, an FNV hash of the raw string is used as fallback.
pub fn generate_svg(agent_id: &str, manifest_hash: &str) -> String {
    let e = hex_decode_32(manifest_hash).unwrap_or_else(|| fnv_bytes(manifest_hash.as_bytes()));
    let p = fnv_bytes(agent_id.as_bytes());
    let hue = (p[0] as f64 / 255.0) * 360.0;
    let sat = 60.0 + (p[1] % 30) as f64;
    let lit = 50.0 + (p[2] % 20) as f64;
    let c1 = hsl_to_hex(hue, sat, lit);
    let c2 = hsl_to_hex((hue + 120.0) % 360.0, sat, lit);
    let c3 = hsl_to_hex((hue + 240.0) % 360.0, sat, lit);
    let label = &agent_id[..agent_id.len().min(8)];
    let mut s = String::with_capacity(950);
    write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 400" width="400" height="400">"#
    )
    .unwrap();
    write!(s, r##"<rect width="400" height="400" fill="#0a0a0f"/>"##).unwrap();
    write!(
        s,
        r#"<circle cx="{}" cy="{}" r="{}" fill="{}" opacity="0.7"/>"#,
        50 + e[3] as u32 * 300 / 255,
        50 + e[4] as u32 * 300 / 255,
        40 + e[5] as u32 * 80 / 255,
        c1
    )
    .unwrap();
    write!(
        s,
        r#"<circle cx="{}" cy="{}" r="{}" fill="{}" opacity="0.6"/>"#,
        50 + e[6] as u32 * 300 / 255,
        50 + e[7] as u32 * 300 / 255,
        30 + e[8] as u32 * 60 / 255,
        c2
    )
    .unwrap();
    write!(
        s,
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" opacity="0.5"/>"#,
        e[9] as u32 * 350 / 255,
        e[10] as u32 * 350 / 255,
        20 + e[11] as u32 * 100 / 255,
        20 + e[12] as u32 * 100 / 255,
        c3
    )
    .unwrap();
    write!(
        s,
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" opacity="0.4"/>"#,
        e[13] as u32 * 350 / 255,
        e[14] as u32 * 350 / 255,
        15 + e[15] as u32 * 80 / 255,
        15 + e[16] as u32 * 80 / 255,
        c1
    )
    .unwrap();
    write!(
        s,
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2" opacity="0.8"/>"#,
        e[17] as u32 * 400 / 255,
        e[18] as u32 * 400 / 255,
        e[19] as u32 * 400 / 255,
        e[20] as u32 * 400 / 255,
        c2
    )
    .unwrap();
    write!(
        s,
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="1.5" opacity="0.7"/>"#,
        e[21] as u32 * 400 / 255,
        e[22] as u32 * 400 / 255,
        e[23] as u32 * 400 / 255,
        e[24] as u32 * 400 / 255,
        c3
    )
    .unwrap();
    write!(
        s,
        r#"<polygon points="{},{} {},{} {},{}" fill="{}" opacity="0.45"/>"#,
        e[25] as u32 * 400 / 255,
        e[26] as u32 * 400 / 255,
        e[27] as u32 * 400 / 255,
        e[28] as u32 * 400 / 255,
        e[29] as u32 * 400 / 255,
        e[30] as u32 * 400 / 255,
        c2
    )
    .unwrap();
    write!(
        s,
        r#"<text x="8" y="392" font-family="monospace" font-size="9" fill="{}" opacity="0.6">{}</text>"#,
        c1, label
    )
    .unwrap();
    write!(s, "</svg>").unwrap();
    s
}

/// Returns a `data:application/json;base64,...` token URI for the given agent.
pub fn generate_token_uri(agent_id: &str, manifest_hash: &str) -> String {
    let svg = generate_svg(agent_id, manifest_hash);
    let svg_b64 = base64_encode(svg.as_bytes());
    let short = &agent_id[..agent_id.len().min(8)];
    let json = format!(
        r#"{{"name":"xvn agent {}","image":"data:image/svg+xml;base64,{}","agent_id":"{}"}}"#,
        short, svg_b64, agent_id
    );
    format!("data:application/json;base64,{}", base64_encode(json.as_bytes()))
}
