//! Integration tests for `POST /api/live/deploy/degen-arena`.
//!
//! Contract:
//!   POST  /api/live/deploy/degen-arena
//!   body: { apiKey: string(0x+64hex), accountAddress: string(0x+40hex),
//!            network: "testnet"|"mainnet" }
//!   200 { ok: true }  on valid input
//!   400              on invalid format
//!   Key NEVER appears in any response body.

use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

// A valid 64-hex private key (all-zero is used only in tests — never a real key).
const VALID_API_KEY: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";
// A valid 40-hex address.
const VALID_ADDR: &str = "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

async fn boot() -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

/// Helper: POST the deploy body and return the response.
async fn post_deploy(
    server: &TestServer,
    api_key: &str,
    account_address: &str,
    network: &str,
) -> axum_test::TestResponse {
    server
        .post("/api/live/deploy/degen-arena")
        .json(&serde_json::json!({
            "apiKey": api_key,
            "accountAddress": account_address,
            "network": network,
        }))
        .await
}

// ── Happy path ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn valid_testnet_body_returns_200_ok_true() {
    let (server, _tmp) = boot().await;
    let resp = post_deploy(&server, VALID_API_KEY, VALID_ADDR, "testnet").await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["ok"], true, "response must carry ok:true");
    // Key must NEVER appear in the response.
    let serialized = body.to_string();
    assert!(
        !serialized.contains(VALID_API_KEY),
        "api key must never appear in the response body"
    );
    assert!(
        !serialized.contains(&VALID_API_KEY[2..]), // bare hex without 0x prefix
        "api key hex must never appear in the response body"
    );
}

#[tokio::test]
async fn valid_mainnet_body_returns_200() {
    let (server, _tmp) = boot().await;
    let resp = post_deploy(&server, VALID_API_KEY, VALID_ADDR, "mainnet").await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["ok"], true);
    assert_eq!(body["network"], "mainnet");
}

#[tokio::test]
async fn response_carries_redacted_suffix_not_full_key() {
    let (server, _tmp) = boot().await;
    // Key ending in "0001" → suffix must be "0001".
    let key = "0x0000000000000000000000000000000000000000000000000000000000000001";
    let resp = post_deploy(&server, key, VALID_ADDR, "testnet").await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(
        body["stored_key_suffix"], "0001",
        "only the last-4 suffix is surfaced"
    );
    // Full key must not appear.
    assert!(!body.to_string().contains(key));
}

// ── Validation: apiKey format ─────────────────────────────────────────────────

#[tokio::test]
async fn missing_0x_prefix_is_rejected() {
    let (server, _tmp) = boot().await;
    // 64 hex digits but no 0x prefix.
    let bad_key = "0000000000000000000000000000000000000000000000000000000000000001";
    let resp = post_deploy(&server, bad_key, VALID_ADDR, "testnet").await;
    resp.assert_status_bad_request();
    // Key must not appear in the error response.
    let body_str = resp.text();
    assert!(!body_str.contains(bad_key), "bad key must not echo in error");
}

#[tokio::test]
async fn too_short_api_key_is_rejected() {
    let (server, _tmp) = boot().await;
    let resp = post_deploy(&server, "0x1234abcd", VALID_ADDR, "testnet").await;
    resp.assert_status_bad_request();
}

#[tokio::test]
async fn too_long_api_key_is_rejected() {
    let (server, _tmp) = boot().await;
    // 65 hex chars after 0x → too long.
    let bad = format!("0x{:065x}", 1u128);
    let resp = post_deploy(&server, &bad, VALID_ADDR, "testnet").await;
    resp.assert_status_bad_request();
}

#[tokio::test]
async fn api_key_with_non_hex_chars_is_rejected() {
    let (server, _tmp) = boot().await;
    // 64 chars but with 'z' — not hex.
    let bad = format!("0x{:z<64}", "");
    let resp = post_deploy(&server, &bad, VALID_ADDR, "testnet").await;
    resp.assert_status_bad_request();
}

// ── Validation: accountAddress format ────────────────────────────────────────

#[tokio::test]
async fn bad_account_address_is_rejected() {
    let (server, _tmp) = boot().await;
    // 38 hex chars after 0x → too short.
    let bad_addr = "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbee";
    let resp = post_deploy(&server, VALID_API_KEY, bad_addr, "testnet").await;
    resp.assert_status_bad_request();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn account_address_without_0x_is_rejected() {
    let (server, _tmp) = boot().await;
    let bad_addr = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
    let resp = post_deploy(&server, VALID_API_KEY, bad_addr, "testnet").await;
    resp.assert_status_bad_request();
}

// ── Validation: network field ─────────────────────────────────────────────────

#[tokio::test]
async fn unknown_network_is_rejected() {
    let (server, _tmp) = boot().await;
    let resp = post_deploy(&server, VALID_API_KEY, VALID_ADDR, "arbitrum").await;
    resp.assert_status_bad_request();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn empty_network_is_rejected() {
    let (server, _tmp) = boot().await;
    let resp = post_deploy(&server, VALID_API_KEY, VALID_ADDR, "").await;
    resp.assert_status_bad_request();
}

// ── Security: key never leaks ─────────────────────────────────────────────────

#[tokio::test]
async fn api_key_never_appears_in_validation_error_response() {
    let (server, _tmp) = boot().await;
    // Submit a bad-format key; ensure it doesn't leak in the 400 body.
    let bad_key = "0x0000000000000000000000000000000000000000000000000000000000000bad";
    let resp = post_deploy(&server, bad_key, "0xinvalidaddress", "testnet").await;
    resp.assert_status_bad_request();
    let body_str = resp.text();
    // The hex payload of the bad key must not appear literally.
    assert!(
        !body_str.contains("0000000000000000000000000000000000000000000000000000000000000bad"),
        "api key hex must not echo in validation error: {body_str}"
    );
}
