use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use xvision_engine::agent::nano_dispatch::{
    build_nano_request, run_nano_inference, NanoDirection, NanoInferenceResult, NanoInputSpec,
    NanoNormalization,
};

// Process-global mutex that serializes all env-var-manipulating tests.
// cargo runs integration tests in a single binary with parallel threads by
// default; without this lock, STUB_SLEEP_SEC or STUB_EXIT_CODE set by one
// test can bleed into another test's subprocess, causing flaky failures.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn stub_worker_path() -> PathBuf {
    // __file__ is not stable in Rust; locate relative to CARGO_MANIFEST_DIR.
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR must be set by cargo test");
    PathBuf::from(manifest).join("tests/fixtures/stub_nano_worker.py")
}

fn golden_sha256(path: &std::path::Path) -> String {
    // Compute the sha256 of the stub worker itself — used as the
    // "checkpoint hash" for happy-path tests where hash verification passes.
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).expect("stub worker must be readable");
    let hash = Sha256::digest(&bytes);
    hex::encode(hash)
}

fn spec() -> NanoInputSpec {
    NanoInputSpec {
        window_bars: 4,
        indicators: vec!["rsi_14".into()],
        normalization: NanoNormalization::Zscore,
    }
}

fn ohlcv() -> Vec<[f64; 5]> {
    vec![[100.0, 101.0, 99.0, 100.5, 1000.0]; 4]
}

fn inds() -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    m.insert("rsi_14".into(), 55.0f64);
    m
}

#[tokio::test]
async fn happy_path_returns_direction_and_confidence() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let worker = stub_worker_path();
    let hash = golden_sha256(&worker);
    let req = build_nano_request(&spec(), NanoDirection::Long, &ohlcv(), &inds());

    let result = run_nano_inference(
        &worker,
        &hash,
        &req,
        /*timeout_ms=*/ 5_000,
    )
    .await
    .unwrap();

    assert!(matches!(result, NanoInferenceResult::Ok { direction, confidence }
        if direction == NanoDirection::Long && (confidence - 0.9).abs() < 1e-9));
}

#[tokio::test]
async fn timeout_returns_fail_safe() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let worker = stub_worker_path();
    let hash = golden_sha256(&worker);
    let req = build_nano_request(&spec(), NanoDirection::Long, &ohlcv(), &inds());

    // STUB_SLEEP_SEC=5 makes the worker sleep longer than the 200 ms timeout.
    std::env::set_var("STUB_SLEEP_SEC", "5");
    let result = run_nano_inference(&worker, &hash, &req, /*timeout_ms=*/ 200)
        .await
        .unwrap();
    std::env::remove_var("STUB_SLEEP_SEC");

    assert!(
        matches!(result, NanoInferenceResult::FailSafe { .. }),
        "timeout must return FailSafe, got {result:?}"
    );
}

#[tokio::test]
async fn crash_returns_fail_safe() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let worker = stub_worker_path();
    let hash = golden_sha256(&worker);
    let req = build_nano_request(&spec(), NanoDirection::Long, &ohlcv(), &inds());

    std::env::set_var("STUB_EXIT_CODE", "1");
    let result = run_nano_inference(&worker, &hash, &req, /*timeout_ms=*/ 5_000)
        .await
        .unwrap();
    std::env::remove_var("STUB_EXIT_CODE");

    assert!(
        matches!(result, NanoInferenceResult::FailSafe { .. }),
        "non-zero exit must return FailSafe, got {result:?}"
    );
}

#[tokio::test]
async fn hash_mismatch_returns_fail_safe_before_spawn() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let worker = stub_worker_path();
    let bad_hash = "0".repeat(64); // wrong sha256
    let req = build_nano_request(&spec(), NanoDirection::Long, &ohlcv(), &inds());

    let result = run_nano_inference(&worker, &bad_hash, &req, /*timeout_ms=*/ 5_000)
        .await
        .unwrap();
    assert!(
        matches!(result, NanoInferenceResult::FailSafe { ref reason } if reason.contains("hash")),
        "hash mismatch must return FailSafe before spawn, got {result:?}"
    );
}
