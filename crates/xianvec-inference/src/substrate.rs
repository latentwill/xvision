//! Phase 4.3 vector loader — reads an NPZ file + manifest sidecar, validates
//! the manifest against the runtime's expected `Manifest`, and returns a
//! `VectorBundle` holding the tensor on the engine's device.
//!
//! NPZ format: a ZIP archive where each entry is a `.npy` file. We parse the
//! ZIP local file headers and npy headers ourselves (no additional crate deps)
//! because the only `zip` version in the workspace's transitive dep tree
//! (`zip 7.2.0`) pulls in `lzma-rust2` which currently has a build conflict
//! against `crc 2.x`. Numpy uses STORED (compression=0) for NPZ by default,
//! so the raw-ZIP approach is both correct and dep-free.
//!
//! Spike manifest compatibility: the spike's `.manifest.json` does not include
//! the `alpha_curve_hash` field (it predates that field). When the field is
//! absent it is treated as `"unspecified"` and the mismatch check is skipped
//! for that field. The `derived_at` field is also informational and is never
//! compared.

use std::path::Path;

use candle_core::{Device, Tensor};
use thiserror::Error;
use xianvec_core::{LayerIndex, Manifest};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum SubstrateError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest parse: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error("npz error: {0}")]
    Npz(String),
    #[error("manifest mismatch — field `{field}`: expected `{expected}`, actual `{actual}`")]
    ManifestMismatch {
        field: String,
        expected: String,
        actual: String,
    },
    #[error("layer {0} not found in NPZ")]
    MissingLayer(LayerIndex),
    #[error("tensor error: {0}")]
    Tensor(#[from] candle_core::Error),
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A validated steering vector ready for the engine.
pub struct VectorBundle {
    pub manifest: Manifest,
    /// The steering tensor on the engine's device. Shape: `(hidden_dim,)`.
    pub tensor: Tensor,
}

// ---------------------------------------------------------------------------
// Sidecar manifest — on-disk format (superset of core Manifest)
// ---------------------------------------------------------------------------

/// On-disk sidecar. Tolerates the spike format (missing `alpha_curve_hash`,
/// extra fields such as `axis`, `layers`, `diagnostics`).
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct SidecarManifest {
    model_id: String,
    model_quant: String,
    /// Single layer (post-Phase 4 format).
    layer: Option<u16>,
    /// Multi-layer list (spike format).
    #[serde(default)]
    layers: Vec<u16>,
    contrast_pair_set_hash: String,
    /// Missing in spike manifests — treated as "unspecified".
    #[serde(default = "default_unspecified")]
    alpha_curve_hash: String,
    embedder_version: String,
    derived_at: chrono::DateTime<chrono::Utc>,
}

fn default_unspecified() -> String {
    "unspecified".to_owned()
}

impl SidecarManifest {
    fn into_core_manifest(self, layer: LayerIndex) -> Manifest {
        Manifest {
            model_id: self.model_id,
            model_quant: self.model_quant,
            layer,
            contrast_pair_set_hash: self.contrast_pair_set_hash,
            alpha_curve_hash: self.alpha_curve_hash,
            embedder_version: self.embedder_version,
            derived_at: self.derived_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load a steering vector bundle.
///
/// `path` — path to the `.npz` file.
/// `expected` — the manifest the runtime expects. Field-by-field comparison
/// against the sidecar; `derived_at` is informational and never compared.
/// `alpha_curve_hash` is skipped when either side is `"unspecified"`.
/// `device` — candle device to load the tensor onto.
pub fn load_vector(
    path: &Path,
    expected: &Manifest,
    device: &Device,
) -> Result<VectorBundle, SubstrateError> {
    // 1. Read manifest sidecar.
    let manifest_path = path.with_extension("manifest.json");
    let manifest_bytes = std::fs::read(&manifest_path)?;
    let sidecar: SidecarManifest = serde_json::from_slice(&manifest_bytes)?;

    // 2. Validate fields.
    check_field("model_id", &expected.model_id, &sidecar.model_id)?;
    // model_quant: skip when either side is "unspecified" (spike vs runtime quant).
    if expected.model_quant != "unspecified" && sidecar.model_quant != "unspecified" {
        check_field("model_quant", &expected.model_quant, &sidecar.model_quant)?;
    }
    check_field(
        "contrast_pair_set_hash",
        &expected.contrast_pair_set_hash,
        &sidecar.contrast_pair_set_hash,
    )?;
    check_field(
        "embedder_version",
        &expected.embedder_version,
        &sidecar.embedder_version,
    )?;
    if expected.alpha_curve_hash != "unspecified" && sidecar.alpha_curve_hash != "unspecified" {
        check_field(
            "alpha_curve_hash",
            &expected.alpha_curve_hash,
            &sidecar.alpha_curve_hash,
        )?;
    }

    // 3. Read the array for the requested layer from the NPZ.
    let layer = expected.layer;
    let data = read_npz_layer(path, layer)?;
    let len = data.len();

    // 4. Build core manifest.
    let manifest = sidecar.into_core_manifest(layer);

    // 5. Create tensor on device.
    let tensor = Tensor::from_vec(data, (len,), device)?;

    Ok(VectorBundle { manifest, tensor })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn check_field(field: &str, expected: &str, actual: &str) -> Result<(), SubstrateError> {
    if expected != actual {
        return Err(SubstrateError::ManifestMismatch {
            field: field.into(),
            expected: expected.into(),
            actual: actual.into(),
        });
    }
    Ok(())
}

/// Read a single layer array from an NPZ file using a minimal ZIP/ZIP64 parser.
/// The key format is `L<n>.npy` (e.g. `L20.npy`). Requires STORED (method=0)
/// entries, which is numpy's default.
///
/// Handles ZIP64 extra fields (tag=0x0001) where `comp_size`/`uncomp_size` in
/// the local file header are `0xFFFFFFFF` and the real 8-byte sizes are in the
/// extra field. Numpy uses ZIP64 automatically for arrays larger than 4 GiB OR
/// when there are more than 65535 files — but in practice also for some smaller
/// files depending on the numpy version (the spike NPZ uses ZIP64).
fn read_npz_layer(path: &Path, layer: LayerIndex) -> Result<Vec<f32>, SubstrateError> {
    let bytes = std::fs::read(path)?;
    let target_name = format!("L{}.npy", layer.0);

    // Walk ZIP local file headers.
    // Local file header layout (offsets from start of header):
    //   0: signature (4) = 0x04034b50
    //   4: version needed (2)
    //   6: flags (2)
    //   8: compression method (2)
    //  10: mod_time (2)
    //  12: mod_date (2)
    //  14: crc32 (4)
    //  18: compressed size (4)   — 0xFFFFFFFF if ZIP64
    //  22: uncompressed size (4) — 0xFFFFFFFF if ZIP64
    //  26: filename length (2)
    //  28: extra field length (2)
    //  30: filename
    //  30+fname_len: extra field
    //  30+fname_len+extra_len: file data
    let mut pos: usize = 0;
    loop {
        if pos + 30 > bytes.len() {
            break;
        }
        let sig = u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
        if sig != 0x04034b50 {
            break;
        }

        let method = u16::from_le_bytes([bytes[pos + 8], bytes[pos + 9]]);
        let comp_size_raw = u32::from_le_bytes([
            bytes[pos + 18],
            bytes[pos + 19],
            bytes[pos + 20],
            bytes[pos + 21],
        ]);
        let uncomp_size_raw = u32::from_le_bytes([
            bytes[pos + 22],
            bytes[pos + 23],
            bytes[pos + 24],
            bytes[pos + 25],
        ]);
        let fname_len = u16::from_le_bytes([bytes[pos + 26], bytes[pos + 27]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[pos + 28], bytes[pos + 29]]) as usize;

        let fname_start = pos + 30;
        let extra_start = fname_start + fname_len;
        let data_offset = extra_start + extra_len;

        let fname_bytes = &bytes[fname_start..fname_start + fname_len];
        let fname = std::str::from_utf8(fname_bytes)
            .map_err(|e| SubstrateError::Npz(format!("invalid utf8 in zip entry name: {e}")))?;

        // Resolve actual sizes — handle ZIP64 (sizes == 0xFFFFFFFF).
        let (comp_size, uncomp_size) =
            if comp_size_raw == 0xFFFF_FFFF || uncomp_size_raw == 0xFFFF_FFFF {
                parse_zip64_sizes(&bytes[extra_start..extra_start + extra_len])
                    .ok_or_else(|| SubstrateError::Npz(format!(
                        "entry {fname}: ZIP64 extra field missing or malformed"
                    )))?
            } else {
                (comp_size_raw as usize, uncomp_size_raw as usize)
            };

        if fname == target_name {
            if method != 0 {
                return Err(SubstrateError::Npz(format!(
                    "entry {fname} uses compression method {method}; only STORED (0) is supported"
                )));
            }
            let data_end = data_offset + uncomp_size;
            if data_end > bytes.len() {
                return Err(SubstrateError::Npz(format!(
                    "entry {fname}: data [{}..{}] extends beyond file bounds ({})",
                    data_offset,
                    data_end,
                    bytes.len()
                )));
            }
            return parse_npy_f32(&bytes[data_offset..data_end]);
        }

        pos = data_offset + comp_size;
    }

    Err(SubstrateError::MissingLayer(layer))
}

/// Parse the ZIP64 extended information extra field (tag=0x0001) and return
/// `(comp_size, uncomp_size)` as `usize`. Returns `None` if the field is absent
/// or shorter than expected.
fn parse_zip64_sizes(extra: &[u8]) -> Option<(usize, usize)> {
    let mut epos: usize = 0;
    while epos + 4 <= extra.len() {
        let tag = u16::from_le_bytes([extra[epos], extra[epos + 1]]);
        let sz = u16::from_le_bytes([extra[epos + 2], extra[epos + 3]]) as usize;
        let field_start = epos + 4;

        if tag == 0x0001 && field_start + sz <= extra.len() {
            let field = &extra[field_start..field_start + sz];
            if field.len() >= 16 {
                let uncomp = u64::from_le_bytes(field[0..8].try_into().ok()?) as usize;
                let comp = u64::from_le_bytes(field[8..16].try_into().ok()?) as usize;
                return Some((comp, uncomp));
            }
        }
        epos = field_start + sz;
    }
    None
}

/// Minimal npy parser for 1-D f32 arrays (little-endian `<f4`).
/// NPY magic: `\x93NUMPY` + 2 version bytes + 2 or 4 header-length bytes + header.
fn parse_npy_f32(data: &[u8]) -> Result<Vec<f32>, SubstrateError> {
    if data.len() < 10 || &data[..6] != b"\x93NUMPY" {
        return Err(SubstrateError::Npz("not a valid .npy file".into()));
    }

    let major = data[6];
    let header_len: usize = if major == 1 {
        u16::from_le_bytes([data[8], data[9]]) as usize
    } else {
        if data.len() < 12 {
            return Err(SubstrateError::Npz("npy v2 header too short".into()));
        }
        u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize
    };

    let hdr_start = if major == 1 { 10 } else { 12 };
    let hdr_end = hdr_start + header_len;
    if hdr_end > data.len() {
        return Err(SubstrateError::Npz("npy header extends beyond data".into()));
    }

    let header_str = std::str::from_utf8(&data[hdr_start..hdr_end])
        .map_err(|e| SubstrateError::Npz(format!("npy header utf8: {e}")))?;

    // Validate dtype.
    if !header_str.contains("'<f4'") && !header_str.contains("\"<f4\"") {
        return Err(SubstrateError::Npz(format!(
            "unsupported dtype in npy header: {header_str}"
        )));
    }

    let nelements = parse_npy_shape(header_str)
        .ok_or_else(|| SubstrateError::Npz(format!("cannot parse shape from: {header_str}")))?;

    let raw = &data[hdr_end..];
    if raw.len() < nelements * 4 {
        return Err(SubstrateError::Npz(format!(
            "npy data too short: expected {} bytes, got {}",
            nelements * 4,
            raw.len()
        )));
    }

    let floats: Vec<f32> = raw[..nelements * 4]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    Ok(floats)
}

/// Extract total element count from the npy `'shape'` field.
fn parse_npy_shape(header: &str) -> Option<usize> {
    let shape_pos = header.find("'shape'")?;
    let after = &header[shape_pos + 7..];
    let after = after.trim_start_matches(':').trim_start();
    let paren_start = after.find('(')?;
    let paren_end = after.find(')')?;
    let tuple_str = &after[paren_start + 1..paren_end];

    // Product of all positive dims; a trailing comma in `(5120,)` yields one
    // empty string after split which `.parse::<usize>()` rejects — `.ok()` drops it.
    let product: usize = tuple_str
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .product();

    if product == 0 { None } else { Some(product) }
}

// ---------------------------------------------------------------------------
// Phase 4.3 lexical scorer (ported from tools/extract_vectors/spike/validate.py)
// ---------------------------------------------------------------------------

/// Compute the decisiveness score of `text` in `[-1, +1]`.
/// Positive = more decisive, negative = more hedged.
pub fn score_text_decisiveness(text: &str) -> f32 {
    const DECISIVE: &[&str] = &[
        "definitely",
        "certainly",
        "absolutely",
        "clearly",
        "without",
        "must",
        "is",
        "are",
        "will",
        "buy",
        "sell",
        "long",
        "short",
        "yes",
        "no",
        "now",
        "immediately",
        "decisive",
        "firmly",
    ];
    const HEDGE: &[&str] = &[
        "perhaps",
        "maybe",
        "possibly",
        "potentially",
        "might",
        "could",
        "may",
        "somewhat",
        "tentatively",
        "consider",
        "depending",
        "depends",
        "unless",
        "if",
        "though",
        "but",
        "however",
        "approximately",
        "around",
        "roughly",
        "tend",
        "tendency",
        "seems",
        "appears",
    ];

    let tokens: Vec<String> = text
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| ".,;:!?\"'()[]".contains(c))
                .to_lowercase()
        })
        .collect();

    let decisive: usize = tokens
        .iter()
        .filter(|t| DECISIVE.contains(&t.as_str()))
        .count();
    let hedge: usize = tokens
        .iter()
        .filter(|t| HEDGE.contains(&t.as_str()))
        .count();
    let total = decisive + hedge;

    if total == 0 {
        0.0
    } else {
        (decisive as f32 - hedge as f32) / total as f32
    }
}

/// Compute the directional match rate: fraction of (base, steered) text pairs
/// where `score(steered) >= score(base)`, i.e. positive steering increases
/// decisiveness. This is spike pass criterion #1.
pub fn directional_match_rate(base_texts: &[&str], steered_texts: &[&str]) -> f32 {
    assert_eq!(base_texts.len(), steered_texts.len());
    if base_texts.is_empty() {
        return 0.0;
    }
    let matches = base_texts
        .iter()
        .zip(steered_texts.iter())
        .filter(|(b, s)| score_text_decisiveness(s) >= score_text_decisiveness(b))
        .count();
    matches as f32 / base_texts.len() as f32
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn spike_expected() -> Manifest {
        use chrono::TimeZone;
        Manifest {
            model_id: "Qwen/Qwen3-32B".into(),
            model_quant: "unspecified".into(),
            layer: LayerIndex(20),
            contrast_pair_set_hash: "6e91738f726ff205".into(),
            alpha_curve_hash: "unspecified".into(),
            embedder_version: "mlx-lm".into(),
            derived_at: chrono::Utc
                .timestamp_opt(1_700_000_000, 0)
                .single()
                .unwrap(),
        }
    }

    fn fixture_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("data/vectors/spike_decisive_v1.npz")
    }

    #[test]
    fn load_spike_fixture_returns_correct_shape() {
        let path = fixture_path();
        if !path.exists() {
            eprintln!("SKIP: fixture not found at {}", path.display());
            return;
        }

        let device = Device::Cpu;
        let bundle =
            load_vector(&path, &spike_expected(), &device).expect("load_vector should succeed");

        assert_eq!(bundle.tensor.dims(), &[5120]);
        assert_eq!(bundle.manifest.model_id, "Qwen/Qwen3-32B");
        assert_eq!(bundle.manifest.layer, LayerIndex(20));
    }

    #[test]
    fn load_spike_fixture_missing_alpha_curve_hash_tolerated() {
        let path = fixture_path();
        if !path.exists() {
            return;
        }
        let mut expected = spike_expected();
        expected.alpha_curve_hash = "unspecified".into();
        let device = Device::Cpu;
        assert!(load_vector(&path, &expected, &device).is_ok());
    }

    #[test]
    fn directional_match_rate_basic() {
        let base = ["perhaps this might work possibly"];
        let steered = ["definitely this will work certainly"];
        assert_eq!(directional_match_rate(&base, &steered), 1.0);
    }

    #[test]
    fn directional_match_rate_empty() {
        assert_eq!(directional_match_rate(&[], &[]), 0.0);
    }

    #[test]
    fn score_text_decisiveness_decisive() {
        assert!(score_text_decisiveness("definitely buy now will") > 0.0);
    }

    #[test]
    fn score_text_decisiveness_hedge() {
        assert!(score_text_decisiveness("perhaps maybe possibly might") < 0.0);
    }

    #[test]
    fn parse_npy_shape_1d() {
        let h = "{'descr': '<f4', 'fortran_order': False, 'shape': (5120,), }";
        assert_eq!(parse_npy_shape(h), Some(5120));
    }

    #[test]
    fn parse_npy_shape_2d() {
        let h = "{'descr': '<f4', 'fortran_order': False, 'shape': (4, 5120), }";
        assert_eq!(parse_npy_shape(h), Some(20480));
    }

    /// Phase 4.3 hard gate — run against the production candle path.
    /// Requires the production model. Skip by default; run with `--ignored`.
    ///
    /// ```bash
    /// cargo test -p xianvec-inference validate_directional_match_production -- --ignored
    /// ```
    #[test]
    #[ignore = "requires production model + vectors; run manually with --ignored"]
    fn validate_directional_match_production() {
        // TODO: load Qwen3Engine + spike vectors; run 5 holdout prompts with
        // steering installed; assert directional_match_rate >= 0.75.
        todo!("wire engine + production vectors");
    }
}
