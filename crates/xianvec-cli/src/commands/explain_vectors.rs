//! `xvn explain-vectors` — pretty-print a vector manifest sidecar.
//!
//! The manifest format is documented in `xianvec-core::substrate::Manifest`
//! and the spike-tolerant superset in `xianvec-inference::substrate::SidecarManifest`.
//! v1 reads the JSON as `serde_json::Value` and surfaces the most useful
//! fields without coupling to either schema directly — this lets us inspect
//! both the spike sidecar and the post-Phase 4.3 production sidecar with
//! the same command.

use std::path::PathBuf;

pub fn run(manifest_path: PathBuf) -> anyhow::Result<()> {
    let bytes = std::fs::read(&manifest_path)?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)?;

    println!("XIANVEC vector manifest — {}", manifest_path.display());
    println!();
    println!("{}", serde_json::to_string_pretty(&v)?);
    println!();

    // Highlights, if present:
    if let Some(model) = v.get("model_id").and_then(|x| x.as_str()) {
        println!("model:         {model}");
    }
    if let Some(quant) = v.get("model_quant").and_then(|x| x.as_str()) {
        println!("model_quant:   {quant}");
    }
    if let Some(layer) = v.get("layer").and_then(|x| x.as_u64()) {
        println!("active layer:  {layer}");
    } else if let Some(layers) = v.get("layers").and_then(|x| x.as_array()) {
        let s: Vec<String> = layers
            .iter()
            .filter_map(|x| x.as_u64().map(|n| n.to_string()))
            .collect();
        println!("active layers: [{}]", s.join(", "));
    }
    if let Some(hash) = v.get("contrast_pair_set_hash").and_then(|x| x.as_str()) {
        println!("pair-set hash: {hash}");
    }
    if let Some(alpha) = v.get("alpha_curve_hash").and_then(|x| x.as_str()) {
        println!("alpha hash:    {alpha}");
    }
    if let Some(emb) = v.get("embedder_version").and_then(|x| x.as_str()) {
        println!("embedder:      {emb}");
    }
    if let Some(derived) = v.get("derived_at").and_then(|x| x.as_str()) {
        println!("derived_at:    {derived}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_vectors_handles_minimal_manifest() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            br#"{"model_id":"Qwen/Qwen3-32B","layer":20,"contrast_pair_set_hash":"abc123"}"#,
        )
        .unwrap();
        run(tmp.path().to_path_buf()).expect("must succeed on minimal manifest");
    }

    #[test]
    fn explain_vectors_handles_spike_layers_array() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            br#"{"model_id":"Qwen/Qwen3-32B","layers":[20,32,42,50]}"#,
        )
        .unwrap();
        run(tmp.path().to_path_buf()).expect("must succeed on spike multi-layer manifest");
    }
}
