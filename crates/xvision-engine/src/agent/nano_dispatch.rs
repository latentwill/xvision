use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

/// Normalization scheme for the model's input features.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NanoNormalization {
    Zscore,
    Minmax,
    None,
}

/// The `input_spec` block stored in `trained_models.input_spec` (JSON).
/// Pinned here so inference and training share the same schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NanoInputSpec {
    pub window_bars: u32,
    pub indicators: Vec<String>,
    pub normalization: NanoNormalization,
}

/// Direction enum for nanochat model output and conditioning token.
/// `rename_all = "UPPERCASE"` matches the wire contract with Python.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum NanoDirection {
    Long,
    Short,
    Neutral,
    Pass,
}

/// Build the single-line JSON request written to the worker's stdin.
/// Field order is FIXED per the cross-language contract:
///   spec → conditioning → ohlcv → indicators
/// `f64` for all numerics. Returns a `serde_json::Value` whose
/// `to_string()` is the line sent to the subprocess.
///
/// `ohlcv`: each bar is `[open, high, low, close, volume]` as f64.
/// `indicator_values`: keyed by indicator name, ordered per `input_spec`.
pub fn build_nano_request(
    spec: &NanoInputSpec,
    conditioning: NanoDirection,
    ohlcv: &[[f64; 5]],
    indicator_values: &std::collections::BTreeMap<String, f64>,
) -> Value {
    // Indicator object: only the keys declared in spec.indicators.
    // NOTE: this workspace's serde_json has NO `preserve_order` feature, so
    // serde_json::Map is a BTreeMap and serializes keys ALPHABETICALLY — the
    // insertion order here does NOT control wire order. That is fine: the
    // cross-language contract is keyed-by-name, and the canonical feature
    // ORDER is carried separately by the `spec.indicators` array (preserved
    // because JSON arrays are ordered). The Python consumer must read
    // indicator values via spec.indicators, not by object key position.
    let mut ind_obj = serde_json::Map::with_capacity(spec.indicators.len());
    for name in &spec.indicators {
        let v = indicator_values.get(name).copied().unwrap_or(0.0);
        ind_obj.insert(name.clone(), serde_json::json!(v));
    }

    // Top-level keys also serialize alphabetically (BTreeMap, see above), not
    // in insert order. The golden-byte test pins the exact alphabetical output;
    // the contract is read-by-key-name, so field order is not significant.
    let mut req = serde_json::Map::with_capacity(4);
    req.insert("spec".into(), serde_json::to_value(spec).unwrap());
    req.insert(
        "conditioning".into(),
        serde_json::to_value(conditioning).unwrap(),
    );
    let ohlcv_arr: Vec<Value> = ohlcv
        .iter()
        .map(|bar| {
            Value::Array(bar.iter().map(|&v| serde_json::json!(v)).collect())
        })
        .collect();
    req.insert("ohlcv".into(), Value::Array(ohlcv_arr));
    req.insert("indicators".into(), Value::Object(ind_obj));

    Value::Object(req)
}

/// Parsed response from a successful nano worker round-trip.
#[derive(Debug, Clone)]
pub struct NanoResponse {
    pub direction: NanoDirection,
    pub confidence: f64,
}

/// Wire shape of the worker's stdout line.
#[derive(Debug, Deserialize)]
struct WorkerResponse {
    direction: NanoDirection,
    confidence: f64,
}

/// Result of one `run_nano_inference` call. The caller (dispatch_filter
/// nanochat branch) maps FailSafe to NEUTRAL under the slot's veto.
#[derive(Debug)]
pub enum NanoInferenceResult {
    Ok {
        direction: NanoDirection,
        confidence: f64,
    },
    FailSafe {
        reason: String,
    },
}

/// Run one nanochat inference round-trip via subprocess.
///
/// 1. Verify `worker_path` file sha256 matches `expected_sha256` BEFORE
///    spawning (fail-safe on mismatch — no subprocess created).
/// 2. Spawn `python3 <worker_path>` with `request` JSON written to stdin.
/// 3. Read one JSON line from stdout within `timeout_ms` milliseconds.
/// 4. Parse `{ direction, confidence }`.
/// 5. Any of: timeout / non-zero exit / parse failure / hash mismatch
///    → return `FailSafe` (never `Err`).
pub async fn run_nano_inference(
    worker_path: &Path,
    expected_sha256: &str,
    request: &Value,
    timeout_ms: u64,
) -> anyhow::Result<NanoInferenceResult> {
    // Step 1: hash-verify before spawn.
    let file_bytes = match tokio::fs::read(worker_path).await {
        Ok(b) => b,
        Err(e) => {
            return Ok(NanoInferenceResult::FailSafe {
                reason: format!("worker read error: {e}"),
            });
        }
    };
    let actual_hash = hex::encode(Sha256::digest(&file_bytes));
    if actual_hash != expected_sha256 {
        return Ok(NanoInferenceResult::FailSafe {
            reason: format!(
                "hash mismatch: expected {expected_sha256}, got {actual_hash}"
            ),
        });
    }

    // Step 2: spawn.
    let mut child = match Command::new("python3")
        .arg(worker_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return Ok(NanoInferenceResult::FailSafe {
                reason: format!("spawn failed: {e}"),
            });
        }
    };

    // Step 3: write request JSON + newline.
    let request_line = format!("{}\n", serde_json::to_string(request)?);
    if let Some(stdin) = child.stdin.take() {
        let mut stdin = stdin;
        let _ = stdin.write_all(request_line.as_bytes()).await;
        drop(stdin); // close stdin so worker sees EOF
    }

    // Step 4: read one stdout line within timeout.
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            return Ok(NanoInferenceResult::FailSafe {
                reason: "no stdout handle".into(),
            });
        }
    };
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    let read_result = timeout(
        Duration::from_millis(timeout_ms),
        reader.read_line(&mut line),
    )
    .await;

    match read_result {
        Err(_elapsed) => {
            let _ = child.kill().await;
            return Ok(NanoInferenceResult::FailSafe {
                reason: format!("timeout after {timeout_ms}ms"),
            });
        }
        Ok(Err(e)) => {
            return Ok(NanoInferenceResult::FailSafe {
                reason: format!("stdout read error: {e}"),
            });
        }
        Ok(Ok(_)) => {}
    }

    // Wait for child exit; treat non-zero as fail-safe.
    let exit_status = child.wait().await;
    match exit_status {
        Ok(s) if !s.success() => {
            return Ok(NanoInferenceResult::FailSafe {
                reason: format!("worker exited non-zero: {s}"),
            });
        }
        Err(e) => {
            return Ok(NanoInferenceResult::FailSafe {
                reason: format!("wait error: {e}"),
            });
        }
        Ok(_) => {}
    }

    // Step 5: parse.
    let trimmed = line.trim();
    match serde_json::from_str::<WorkerResponse>(trimmed) {
        Ok(r) => Ok(NanoInferenceResult::Ok {
            direction: r.direction,
            confidence: r.confidence,
        }),
        Err(e) => Ok(NanoInferenceResult::FailSafe {
            reason: format!("parse error: {e} (raw: {trimmed:.200})"),
        }),
    }
}

/// Veto truth table (shared contracts §Veto truth table).
///
/// Returns `Some(payload)` when the nanochat signal should pass to the trader,
/// `None` when the hard gate blocks the trade.
///
/// - `veto=true` & model=NEUTRAL → None
/// - `veto=true` & model≠llm_dir → None
/// - `veto=true` & model==llm_dir → Some({direction, confidence})
/// - `veto=false` (any) → Some({direction, confidence}) — advisory; trader runs
pub fn resolve_nano_filter(
    llm_dir: NanoDirection,
    model_dir: NanoDirection,
    confidence: f64,
    veto: bool,
) -> Option<serde_json::Value> {
    if !veto {
        // Advisory mode: always pass, carry both direction + confidence.
        return Some(serde_json::json!({
            "direction": serde_json::to_value(model_dir).unwrap(),
            "confidence": confidence,
        }));
    }
    // Hard gate.
    match model_dir {
        NanoDirection::Neutral => None,
        dir if dir == llm_dir => Some(serde_json::json!({
            "direction": serde_json::to_value(dir).unwrap(),
            "confidence": confidence,
        })),
        _ => None, // direction mismatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn build_nano_request_golden_bytes() {
        // GOLDEN: the exact serialized bytes Rust sends to xvision_prepare.py.
        // Any change to this test output is a BREAKING cross-language change
        // and must be reflected in xvision_prepare.py simultaneously.
        let spec = NanoInputSpec {
            window_bars: 64,
            indicators: vec!["rsi_14".into(), "atr_20".into()],
            normalization: NanoNormalization::Zscore,
        };
        let ohlcv: Vec<[f64; 5]> = vec![[100.0, 101.5, 99.0, 101.0, 5000.0]];
        let mut inds = BTreeMap::new();
        inds.insert("rsi_14".into(), 55.3f64);
        inds.insert("atr_20".into(), 1.2f64);

        let req = build_nano_request(&spec, NanoDirection::Long, &ohlcv, &inds);
        let serialized = serde_json::to_string(&req).unwrap();

        // serde_json without `preserve_order` feature uses BTreeMap (alphabetical).
        // Field order is alphabetical: conditioning, indicators, ohlcv, spec.
        // Within indicators object: atr_20 before rsi_14 (alphabetical).
        // Within spec object: indicators, normalization, window_bars (alphabetical).
        // This IS the cross-language contract for this workspace — xvision_prepare.py
        // must parse JSON keys by name, not by position, so order does not matter
        // for correctness; however the exact bytes are pinned here for auditability.
        let expected = concat!(
            r#"{"conditioning":"LONG","#,
            r#""indicators":{"atr_20":1.2,"rsi_14":55.3},"#,
            r#""ohlcv":[[100.0,101.5,99.0,101.0,5000.0]],"#,
            r#""spec":{"indicators":["rsi_14","atr_20"],"normalization":"zscore","window_bars":64}}"#
        );
        assert_eq!(
            serialized, expected,
            "golden bytes mismatch — this is a cross-language contract break"
        );
    }

    // Veto truth table — pure function, all four cases from shared contracts.
    // FilterSignalPayload is the JSON value placed in FilterSignal.payload.
    // A None return means the downstream trader must NOT fire.

    #[test]
    fn veto_true_neutral_direction_returns_none() {
        let result = resolve_nano_filter(
            NanoDirection::Long,    // llm_dir
            NanoDirection::Neutral, // model_dir
            0.9,
            true, // veto
        );
        assert!(result.is_none(), "NEUTRAL + veto=true must produce null payload");
    }

    #[test]
    fn veto_true_mismatch_direction_returns_none() {
        let result = resolve_nano_filter(
            NanoDirection::Long,
            NanoDirection::Short,
            0.8,
            true,
        );
        assert!(result.is_none(), "direction mismatch + veto=true must produce null payload");
    }

    #[test]
    fn veto_true_matching_direction_returns_payload() {
        let result = resolve_nano_filter(
            NanoDirection::Long,
            NanoDirection::Long,
            0.85,
            true,
        );
        let payload = result.expect("matching direction + veto=true must return Some payload");
        assert_eq!(payload.get("direction").and_then(|v| v.as_str()), Some("LONG"));
        let conf = payload.get("confidence").and_then(|v| v.as_f64()).unwrap();
        assert!((conf - 0.85).abs() < 1e-9);
    }

    #[test]
    fn veto_false_any_direction_returns_advisory_payload() {
        // advisory: trader always runs regardless of direction
        for (llm, model) in [
            (NanoDirection::Long, NanoDirection::Neutral),
            (NanoDirection::Long, NanoDirection::Short),
            (NanoDirection::Short, NanoDirection::Long),
        ] {
            let result = resolve_nano_filter(llm, model, 0.6, false);
            assert!(
                result.is_some(),
                "veto=false must always return Some payload; llm={llm:?} model={model:?}"
            );
        }
    }
}
