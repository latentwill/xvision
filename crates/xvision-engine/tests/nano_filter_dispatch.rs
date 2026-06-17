// Integration test: a dispatch_filter invocation with AgentRef.checkpoint set
// must go through the nanochat path (stub worker) and produce a FilterSignal
// with the expected direction + confidence in payload — without calling the LLM.
//
// `dispatch_filter_with_checkpoint` is a public fn added in Task 3.4 Step 2
// that exercises the nanochat branch directly without the full pipeline harness.

use std::collections::BTreeMap;
use std::sync::Mutex;

use xvision_engine::agent::dispatch_capability::{dispatch_filter_with_checkpoint, FilterSignal};
use xvision_engine::agent::nano_dispatch::{NanoDirection, NanoInputSpec, NanoNormalization};

// Process-global mutex: serializes all env-var-manipulating tests.
// cargo runs integration tests in a single binary with parallel threads by
// default; without this lock, STUB_DIRECTION set by one test can bleed into
// another test's subprocess.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn stub_worker_path() -> std::path::PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo test");
    std::path::PathBuf::from(manifest).join("tests/fixtures/stub_nano_worker.py")
}

fn golden_sha256(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).expect("stub worker must be readable");
    hex::encode(Sha256::digest(bytes))
}

#[tokio::test]
async fn checkpoint_slot_produces_filter_signal_via_stub_worker() {
    // Hold the lock for the full test so STUB_DIRECTION default (LONG) is not
    // clobbered by the other test running in parallel.
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let worker = stub_worker_path();
    let hash = golden_sha256(&worker);

    let spec = NanoInputSpec {
        window_bars: 4,
        indicators: vec!["rsi_14".into()],
        normalization: NanoNormalization::Zscore,
    };
    let ohlcv: Vec<[f64; 5]> = vec![[100.0, 101.0, 99.0, 100.5, 1000.0]; 4];
    let mut inds = BTreeMap::new();
    inds.insert("rsi_14".into(), 55.0f64);

    // Stub returns LONG + 0.9 by default; veto=true + LONG upstream == LONG
    // model direction → payload passes.
    let signal: FilterSignal = dispatch_filter_with_checkpoint(
        "nanochat",
        NanoDirection::Long,
        &spec,
        &ohlcv,
        &inds,
        &worker,
        &hash,
        /*veto=*/ true,
        /*timeout_ms=*/ 5_000,
    )
    .await
    .unwrap();

    assert!(
        !signal.payload.is_null(),
        "matching direction + veto=true must produce non-null payload"
    );
    assert_eq!(
        signal.payload.get("direction").and_then(|v| v.as_str()),
        Some("LONG"),
        "payload direction must be LONG"
    );
}

#[tokio::test]
async fn checkpoint_slot_veto_neutral_produces_null_payload() {
    // Hold the lock for the full test so STUB_DIRECTION mutation is exclusive.
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let worker = stub_worker_path();
    let hash = golden_sha256(&worker);

    let spec = NanoInputSpec {
        window_bars: 4,
        indicators: vec![],
        normalization: NanoNormalization::Zscore,
    };
    let ohlcv: Vec<[f64; 5]> = vec![[100.0, 101.0, 99.0, 100.5, 1000.0]; 4];
    let inds = BTreeMap::new();

    // Stub returns NEUTRAL when STUB_DIRECTION=NEUTRAL.
    // Safety: set env var while holding ENV_LOCK so no other test sees it.
    // SAFETY: single-threaded access guaranteed by ENV_LOCK.
    unsafe { std::env::set_var("STUB_DIRECTION", "NEUTRAL") };
    let signal = dispatch_filter_with_checkpoint(
        "nanochat",
        NanoDirection::Long,
        &spec,
        &ohlcv,
        &inds,
        &worker,
        &hash,
        /*veto=*/ true,
        /*timeout_ms=*/ 5_000,
    )
    .await
    .unwrap();
    unsafe { std::env::remove_var("STUB_DIRECTION") };

    assert!(
        signal.payload.is_null(),
        "NEUTRAL + veto=true must produce null payload, got: {:?}",
        signal.payload
    );
}
