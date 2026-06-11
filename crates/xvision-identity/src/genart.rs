//! genart — Bitfields v3 generator. Byte-identical Rust twin of the
//! TypeScript reference implementation:
//! `frontend/web/src/features/marketplace/lib/genartGrid.ts` (engine) and
//! `frontend/web/src/features/marketplace/lib/genart.ts` (SVG + tokenURI).
//!
//! Parity is enforced by golden fixtures at `tests/fixtures/genart_v3.json`.
//! Any change here must be mirrored in the TS files and the fixtures
//! re-frozen — never edit one side alone.

use std::fmt;
use std::fmt::Write as _;

use alloy::primitives::keccak256;

const N: usize = 28;
const LAYERS: u32 = 6;
const STATES: i32 = 6;
const PAL_LEN: i32 = 7;
const DENSITY_FLOOR: f64 = 0.14;

/// Locked roster — order is normative (spec Appendix A). Append-only post-launch.
const PALETTES: [(&str, [&str; 7]); 33] = [
    (
        "risoBlue",
        [
            "#0d1026", "#1c2a6b", "#2f4bb8", "#3f6df2", "#7fa3ff", "#ffd23f", "#fff6e0",
        ],
    ),
    (
        "risoRedTeal",
        [
            "#140a0d", "#5c1a2e", "#c1224f", "#ff5470", "#1ca3a3", "#9fe3d4", "#fff3e8",
        ],
    ),
    (
        "candyArcade",
        [
            "#0d0714", "#2e1245", "#5a1f7d", "#9032a8", "#e84393", "#ffd24f", "#fff5dc",
        ],
    ),
    (
        "circuit",
        [
            "#041013", "#08242b", "#0f5260", "#18a98f", "#a5f3dc", "#ff3b73", "#ffe95e",
        ],
    ),
    (
        "coldSignal",
        [
            "#071019", "#102936", "#23545b", "#44a3a3", "#c5e4dc", "#f44465", "#ffe6a7",
        ],
    ),
    (
        "grapeSoda",
        [
            "#0c0714", "#231140", "#41207a", "#6a39b8", "#9c6be0", "#cda8f0", "#f3e8fd",
        ],
    ),
    (
        "punolit",
        [
            "#11151f", "#1e3442", "#35665f", "#89a36a", "#d5c686", "#df9d8b", "#e8d7cf",
        ],
    ),
    (
        "calmSunset",
        [
            "#2c1534", "#5c2751", "#a94768", "#df8584", "#f3cda9", "#f7e6b0", "#fff7d6",
        ],
    ),
    (
        "lineage",
        [
            "#080916", "#182044", "#263b71", "#426e91", "#75a57d", "#d2bc72", "#f6ead0",
        ],
    ),
    (
        "signalRust",
        [
            "#0c0b0a", "#26211c", "#4f4138", "#8a6450", "#c97f4f", "#f25c3a", "#ffe9d4",
        ],
    ),
    (
        "magmaCore",
        [
            "#0c0508", "#330a12", "#70101c", "#bf1f26", "#f2542d", "#ffa552", "#ffe8c2",
        ],
    ),
    (
        "tidalDusk",
        [
            "#0a1012", "#103035", "#1a5e60", "#2f9a8c", "#e6c36b", "#ef9f63", "#fcefd2",
        ],
    ),
    (
        "ultraviolet",
        [
            "#08051a", "#160d44", "#2a1a80", "#4730c4", "#7a5ef2", "#b49cff", "#e9e2ff",
        ],
    ),
    (
        "voltYellow",
        [
            "#0a0a10", "#1d2433", "#2f4866", "#4a7ab8", "#7fb3e8", "#ffe83f", "#fdf8e2",
        ],
    ),
    (
        "mintMagenta",
        [
            "#070f0d", "#0f2e26", "#1a5c47", "#2f9a73", "#8fe0bb", "#f23fa0", "#fff0f7",
        ],
    ),
    (
        "tealEmber",
        [
            "#06100f", "#0e3331", "#176561", "#2aa39a", "#aee8df", "#ff7733", "#ffeed9",
        ],
    ),
    (
        "indigoCoral",
        [
            "#08081a", "#161a4d", "#2a2f8f", "#4a55d6", "#9aa3f2", "#ff6f61", "#fff1e8",
        ],
    ),
    (
        "limeViolet",
        [
            "#0b0d06", "#222e0d", "#3f5c14", "#6f9a1f", "#b8e040", "#8a3ff2", "#f4eaff",
        ],
    ),
    (
        "roseCyan",
        [
            "#120710", "#3a0f2e", "#73195c", "#b8268f", "#f060c4", "#2ee6e6", "#e8feff",
        ],
    ),
    (
        "amberInk",
        [
            "#0b0a12", "#1f1d33", "#3a3866", "#5c59a8", "#9a97d9", "#ffb347", "#fff3da",
        ],
    ),
    (
        "crimsonMint",
        [
            "#120709", "#3f0d18", "#7c142b", "#c41f44", "#f25c77", "#5ce8b8", "#eafff6",
        ],
    ),
    (
        "cobaltTangerine",
        [
            "#06091a", "#0e1f56", "#1a3b9e", "#2f63e0", "#85aaf2", "#ff9433", "#fff0dc",
        ],
    ),
    (
        "orchidLime",
        [
            "#100818", "#2e1247", "#5a2080", "#9438c4", "#d685f0", "#cfe83f", "#f9ffe0",
        ],
    ),
    (
        "pinkPitch",
        [
            "#0d0c0c", "#1f1d1f", "#3b373b", "#6b6168", "#b3a6ad", "#ff3f8e", "#ffe6f1",
        ],
    ),
    (
        "acidTeal",
        [
            "#0c1206", "#1f330d", "#3f6618", "#6fa826", "#b8e84a", "#1fb8c9", "#e0fbff",
        ],
    ),
    (
        "goldGrape",
        [
            "#0e0814", "#291245", "#4d1f7d", "#7d33b8", "#b370e0", "#ffd23f", "#fff6dc",
        ],
    ),
    (
        "rustTurquoise",
        [
            "#120b08", "#3b1c10", "#73331a", "#b85426", "#e88a4f", "#2ec9b8", "#e8fcf7",
        ],
    ),
    (
        "cherryCola",
        [
            "#100808", "#330f12", "#661a21", "#a82a35", "#e0525c", "#ffc26b", "#fff0d9",
        ],
    ),
    (
        "duskNeon",
        [
            "#0a0814", "#1d1640", "#352a73", "#5444a8", "#8a73d9", "#3fffb8", "#eafff5",
        ],
    ),
    (
        "peachAbyss",
        [
            "#050811", "#0d1c3a", "#173366", "#2a52a3", "#6f8fd9", "#ffb38a", "#fff0e2",
        ],
    ),
    (
        "saffronSea",
        [
            "#071013", "#0f2c38", "#1a5366", "#2f85a3", "#73c2d9", "#ffc63f", "#fff6da",
        ],
    ),
    (
        "furnacePink",
        [
            "#0f070c", "#360d2b", "#6e1452", "#b81f7d", "#f23fb0", "#ffae3f", "#ffeed4",
        ],
    ),
    (
        "glacierPunch",
        [
            "#070b10", "#13283d", "#234a73", "#3f78b3", "#8fc1e8", "#f2543f", "#ffe9e0",
        ],
    ),
];

/// Symmetry mode of a Bitfields v3 grid. String forms match the TS engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symmetry {
    Free,
    MirrorX,
    MirrorY,
    Quad,
    Diagonal,
    AntiDiagonal,
    Rot180,
    Rot90,
}

impl Symmetry {
    /// Normative string form (used in tokenURI attributes).
    pub fn as_str(self) -> &'static str {
        match self {
            Symmetry::Free => "free",
            Symmetry::MirrorX => "mirror-x",
            Symmetry::MirrorY => "mirror-y",
            Symmetry::Quad => "quad",
            Symmetry::Diagonal => "diagonal",
            Symmetry::AntiDiagonal => "anti-diagonal",
            Symmetry::Rot180 => "rot180",
            Symmetry::Rot90 => "rot90",
        }
    }
}

impl fmt::Display for Symmetry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

const SYMMETRY_BAG: [Symmetry; 13] = [
    Symmetry::Free,
    Symmetry::Free,
    Symmetry::Free,
    Symmetry::MirrorX,
    Symmetry::MirrorY,
    Symmetry::Quad,
    Symmetry::Quad,
    Symmetry::Quad,
    Symmetry::Diagonal,
    Symmetry::AntiDiagonal,
    Symmetry::Rot180,
    Symmetry::Rot90,
    Symmetry::Rot90,
];

/// Derived visual traits for an agent NFT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Traits {
    /// Palette name from the locked roster.
    pub palette: String,
    /// Symmetry mode drawn from the weighted bag.
    pub symmetry: Symmetry,
    /// 0-100, % of filled display cells.
    pub density: u32,
    /// Always 6.
    pub layers: u32,
}

/// Errors from the Bitfields v3 generator. Inputs are validated strictly —
/// there is no fallback rendering for bad input.
#[derive(Debug, thiserror::Error)]
pub enum GenartError {
    /// `agent_id` must match `^[0-9A-Za-z_-]{1,64}$`.
    #[error("invalid agent_id: {0:?}")]
    InvalidAgentId(String),
    /// `manifest_hash` must be exactly 64 chars of lowercase hex.
    #[error("manifest_hash must be 64-char lowercase hex")]
    InvalidManifestHash,
}

// ---------------------------------------------------------------------------
// PRNG — exact twins of the TS fnv1a32 / mulberry32 (ASCII seeds only).
// ---------------------------------------------------------------------------

fn fnv1a32(s: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for &b in s.as_bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

struct Mulberry32 {
    s: u32,
}

impl Mulberry32 {
    fn new(seed: u32) -> Self {
        Self { s: seed }
    }

    fn next(&mut self) -> f64 {
        self.s = self.s.wrapping_add(0x6d2b_79f5);
        let mut t = self.s;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
    }
}

// ---------------------------------------------------------------------------
// Grid engine
// ---------------------------------------------------------------------------

fn raw_grid(seed_str: &str, transparent_states: i32) -> [i8; N * N] {
    let mut r = Mulberry32::new(fnv1a32(seed_str));
    let mut grid = [-1i8; N * N];
    for _layer in 0..LAYERS {
        let op_idx = (r.next() * 3.0).floor() as usize;
        let band = 1 + (r.next() * 7.0).floor() as i32;
        let base = 2 + (r.next() * 9.0).floor() as i32;
        let xo = (r.next() * 64.0).floor() as i32;
        let yo = (r.next() * 64.0).floor() as i32;
        let radial = r.next() > 0.7;
        let invert = r.next() > 0.8;
        // cx = N/2 + xo, so dx = (x + xo) - cx = x - N/2; kept in the TS form.
        let cx = N as f64 / 2.0 + xo as f64;
        let cy = N as f64 / 2.0 + yo as f64;
        for y in 0..N as i32 {
            for x in 0..N as i32 {
                let step = if radial {
                    let dx = (x + xo) as f64 - cx;
                    let dy = (y + yo) as f64 - cy;
                    ((dx * dx + dy * dy).sqrt() / band as f64).floor() as i32
                } else {
                    ((y + yo) as f64 / band as f64).floor() as i32
                };
                let t = base + step;
                let mut v: i32 = match op_idx {
                    0 => (x + y + xo) & (y - x + yo),
                    1 => (x + y + xo) ^ (y - x + yo),
                    _ => (x + y + xo) | (y - x + yo),
                };
                if invert {
                    v = !v;
                }
                v = ((v % t) + t) % t;
                let s = v % (STATES + transparent_states);
                if s < transparent_states {
                    continue;
                }
                grid[(y as usize) * N + x as usize] = ((s - transparent_states) % PAL_LEN) as i8;
                // no-op for s-transparent in [0,5]; kept for line-parity with the TS twin
            }
        }
    }
    grid
}

fn filled_ratio(grid: &[i8; N * N]) -> f64 {
    let filled = grid.iter().filter(|&&v| v >= 0).count();
    filled as f64 / (N * N) as f64
}

fn dense_grid(seed_str: &str) -> [i8; N * N] {
    let mut transparent_states: i32 = 7;
    for attempt in 0..5 {
        let seed = if attempt == 0 {
            seed_str.to_string()
        } else {
            format!("{seed_str}#{attempt}")
        };
        let g = raw_grid(&seed, transparent_states);
        if filled_ratio(&g) >= DENSITY_FLOOR {
            return g;
        }
        transparent_states = (transparent_states - 2).max(2);
    }
    raw_grid(&format!("{seed_str}#final"), 2)
}

fn canon(mode: Symmetry, x: usize, y: usize) -> (usize, usize) {
    let last = N - 1;
    match mode {
        Symmetry::Free => (x, y),
        Symmetry::MirrorX => (x.min(last - x), y),
        Symmetry::MirrorY => (x, y.min(last - y)),
        Symmetry::Quad => (x.min(last - x), y.min(last - y)),
        Symmetry::Diagonal => {
            if x < y {
                (y, x)
            } else {
                (x, y)
            }
        }
        Symmetry::AntiDiagonal => {
            if x + y > last {
                (last - y, last - x)
            } else {
                (x, y)
            }
        }
        Symmetry::Rot180 => {
            if y * N + x <= (last - y) * N + (last - x) {
                (x, y)
            } else {
                (last - x, last - y)
            }
        }
        Symmetry::Rot90 => {
            let (mut bx, mut by, mut bi) = (x, y, y * N + x);
            let (mut cx, mut cy) = (x, y);
            for _ in 0..3 {
                let (nx, ny) = (cy, last - cx);
                cx = nx;
                cy = ny;
                let i = cy * N + cx;
                if i < bi {
                    bi = i;
                    bx = cx;
                    by = cy;
                }
            }
            (bx, by)
        }
    }
}

struct BuiltGrid {
    grid: [i8; N * N],
    palette: &'static [&'static str; 7],
    traits: Traits,
}

fn build_grid_from_seed_string(seed_string: &str) -> BuiltGrid {
    let mut r = Mulberry32::new(fnv1a32(seed_string));
    let palette_idx = (r.next() * PALETTES.len() as f64).floor() as usize;
    let (palette_name, palette) = &PALETTES[palette_idx];
    let symmetry = SYMMETRY_BAG[(r.next() * SYMMETRY_BAG.len() as f64).floor() as usize];
    let raw = dense_grid(seed_string);
    let mut grid = [0i8; N * N];
    let mut filled: u32 = 0;
    for y in 0..N {
        for x in 0..N {
            let (sx, sy) = canon(symmetry, x, y);
            let v = raw[sy * N + sx];
            grid[y * N + x] = v;
            if v >= 0 {
                filled += 1;
            }
        }
    }
    let density = ((100.0 * filled as f64) / (N * N) as f64).round() as u32;
    BuiltGrid {
        grid,
        palette,
        traits: Traits {
            palette: (*palette_name).to_string(),
            symmetry,
            density,
            layers: LAYERS,
        },
    }
}

fn valid_agent_id(agent_id: &str) -> bool {
    let b = agent_id.as_bytes();
    (1..=64).contains(&b.len())
        && b.iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn valid_manifest_hash(hash: &str) -> bool {
    let b = hash.as_bytes();
    b.len() == 64
        && b.iter()
            .all(|&c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// Validated entry for mint-path use. Errs on bad input — never falls back.
fn build_grid(agent_id: &str, manifest_hash: &str) -> Result<BuiltGrid, GenartError> {
    if !valid_agent_id(agent_id) {
        return Err(GenartError::InvalidAgentId(agent_id.to_string()));
    }
    if !valid_manifest_hash(manifest_hash) {
        return Err(GenartError::InvalidManifestHash);
    }
    Ok(build_grid_from_seed_string(&format!(
        "{agent_id}:{manifest_hash}"
    )))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Derives the visual traits for `agent_id` + `manifest_hash`.
pub fn derive_traits(agent_id: &str, manifest_hash: &str) -> Result<Traits, GenartError> {
    Ok(build_grid(agent_id, manifest_hash)?.traits)
}

/// Generates SVG from an already-built grid. Private helper for `generate_svg` and `generate_token_uri`.
fn svg_from_built(built: &BuiltGrid) -> String {
    let palette = built.palette;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {N} {N}\" width=\"560\" height=\"560\" shape-rendering=\"crispEdges\"><rect width=\"{N}\" height=\"{N}\" fill=\"{}\"/>",
        palette[0]
    );
    // Per-palette-index run lists in row-major order.
    let mut d_strings: [String; 7] = Default::default();
    for y in 0..N {
        let mut x = 0;
        while x < N {
            let v = built.grid[y * N + x];
            if v < 0 {
                x += 1;
                continue;
            }
            let mut x2 = x + 1;
            while x2 < N && built.grid[y * N + x2] == v {
                x2 += 1;
            }
            write!(d_strings[v as usize], "M{x} {y}.5h{}", x2 - x).expect("string write");
            x = x2;
        }
    }
    // One <path> per palette index with ≥1 run, ascending index order.
    for (i, d) in d_strings.iter().enumerate() {
        if !d.is_empty() {
            write!(
                out,
                "<path stroke=\"{}\" stroke-width=\"1\" d=\"{d}\"/>",
                palette[i]
            )
            .expect("string write");
        }
    }
    out.push_str("</svg>");
    out
}

/// Generates the normative single-line SVG (stroke-path encoding) for the agent.
pub fn generate_svg(agent_id: &str, manifest_hash: &str) -> Result<String, GenartError> {
    let built = build_grid(agent_id, manifest_hash)?;
    Ok(svg_from_built(&built))
}

/// Generates the `data:application/json;base64,…` tokenURI for the agent.
pub fn generate_token_uri(agent_id: &str, manifest_hash: &str) -> Result<String, GenartError> {
    let built = build_grid(agent_id, manifest_hash)?;
    let svg = svg_from_built(&built);
    let svg_b64 = base64_encode(svg.as_bytes());
    let short = &agent_id[..agent_id.len().min(8)];
    // agent_id already validated to [0-9A-Za-z_-]{1,64} — safe to interpolate raw.
    // Field order is normative — the TS twin emits the identical byte string.
    let json = format!(
        "{{\"name\":\"xvn strategy {short}\",\"image\":\"data:image/svg+xml;base64,{svg_b64}\",\"agent_id\":\"{agent_id}\",\"attributes\":[{{\"trait_type\":\"Symmetry\",\"value\":\"{}\"}},{{\"trait_type\":\"Palette\",\"value\":\"{}\"}},{{\"trait_type\":\"Density\",\"value\":{}}},{{\"trait_type\":\"Layers\",\"value\":{}}}]}}",
        built.traits.symmetry.as_str(),
        built.traits.palette,
        built.traits.density,
        built.traits.layers,
    );
    Ok(format!(
        "data:application/json;base64,{}",
        base64_encode(json.as_bytes())
    ))
}

/// Keccak-256 of the canonical-JSON manifest bytes, as 64-char lowercase hex.
pub fn manifest_hash_hex(canonical_json: &str) -> String {
    let digest = keccak256(canonical_json.as_bytes());
    let mut out = String::with_capacity(64);
    for b in digest.0 {
        write!(out, "{b:02x}").expect("string write");
    }
    out
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
