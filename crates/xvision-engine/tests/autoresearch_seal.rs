use chrono::{TimeZone, Utc};
use ed25519_dalek::SigningKey;
use sqlx::sqlite::SqlitePoolOptions;
use xvision_engine::autoresearch::content_hash::ContentHash;
use xvision_engine::autoresearch::seal::{build_and_sign, CycleSeal, OPERATOR_DISPLAY_LABEL};

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE cycle_seals (
            seal_id TEXT PRIMARY KEY,
            cycle_id TEXT NOT NULL,
            merkle_root TEXT NOT NULL,
            operator_signature TEXT NOT NULL,
            sealed_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn test_key() -> SigningKey {
    SigningKey::from_bytes(&[42u8; 32])
}

fn test_merkle() -> ContentHash {
    ContentHash::of_bytes(b"merkle-test")
}

#[tokio::test]
async fn build_and_sign_verify_round_trip() {
    let key = test_key();
    let verifying_key = key.verifying_key();
    let seal = build_and_sign("cycle-1", "session-1", test_merkle(), 5, &key).unwrap();
    assert_eq!(seal.cycle_id, "cycle-1");
    assert_eq!(seal.session_id, "session-1");
    assert_eq!(seal.node_count, 5);
    assert!(
        seal.verify(&verifying_key).is_ok(),
        "verify must succeed with matching pubkey"
    );
}

#[tokio::test]
async fn verify_fails_on_tampered_cycle_id() {
    let key = test_key();
    let verifying_key = key.verifying_key();
    let seal = build_and_sign("cycle-orig", "session-1", test_merkle(), 3, &key).unwrap();
    let tampered = CycleSeal {
        cycle_id: "cycle-tampered".into(),
        ..seal
    };
    assert!(
        tampered.verify(&verifying_key).is_err(),
        "tampered cycle_id must fail verify"
    );
}

#[tokio::test]
async fn verify_fails_on_tampered_merkle_root() {
    let key = test_key();
    let verifying_key = key.verifying_key();
    let seal = build_and_sign("cycle-1", "session-1", test_merkle(), 7, &key).unwrap();
    let tampered = CycleSeal {
        merkle_root: ContentHash::of_bytes(b"different"),
        ..seal
    };
    assert!(
        tampered.verify(&verifying_key).is_err(),
        "tampered merkle_root must fail verify"
    );
}

#[tokio::test]
async fn verify_fails_on_tampered_node_count() {
    let key = test_key();
    let verifying_key = key.verifying_key();
    let seal = build_and_sign("cycle-1", "session-1", test_merkle(), 4, &key).unwrap();
    let tampered = CycleSeal {
        node_count: 999,
        ..seal
    };
    assert!(
        tampered.verify(&verifying_key).is_err(),
        "tampered node_count must fail verify"
    );
}

#[tokio::test]
async fn verify_fails_on_tampered_session_id() {
    let key = test_key();
    let verifying_key = key.verifying_key();
    let seal = build_and_sign("cycle-1", "session-orig", test_merkle(), 2, &key).unwrap();
    let tampered = CycleSeal {
        session_id: "session-tampered".into(),
        ..seal
    };
    assert!(
        tampered.verify(&verifying_key).is_err(),
        "tampered session_id must fail verify"
    );
}

#[tokio::test]
async fn verify_fails_on_tampered_sealed_at() {
    let key = test_key();
    let verifying_key = key.verifying_key();
    let seal = build_and_sign("cycle-1", "session-1", test_merkle(), 6, &key).unwrap();
    let tampered = CycleSeal {
        sealed_at: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        ..seal
    };
    assert!(
        tampered.verify(&verifying_key).is_err(),
        "tampered sealed_at must fail verify"
    );
}

#[tokio::test]
async fn verify_fails_with_wrong_verifying_key() {
    let key = test_key();
    let seal = build_and_sign("cycle-1", "session-1", test_merkle(), 3, &key).unwrap();
    let wrong_key = SigningKey::from_bytes(&[99u8; 32]);
    let wrong_verifying = wrong_key.verifying_key();
    assert!(
        seal.verify(&wrong_verifying).is_err(),
        "wrong verifying key must fail verify"
    );
}

#[tokio::test]
async fn persist_load_round_trip_preserves_all_fields() {
    let pool = fresh_pool().await;
    let key = test_key();
    let verifying_key = key.verifying_key();
    let original = build_and_sign("cycle-rt", "session-rt", test_merkle(), 11, &key).unwrap();
    original.persist(&pool).await.unwrap();

    let loaded = CycleSeal::load(&pool, &original.seal_id.to_string())
        .await
        .unwrap()
        .expect("seal must be present after persist");

    assert_eq!(loaded.seal_id, original.seal_id);
    assert_eq!(loaded.cycle_id, original.cycle_id);
    assert_eq!(loaded.merkle_root, original.merkle_root);
    assert_eq!(loaded.node_count, original.node_count);
    assert_eq!(loaded.operator_signature, original.operator_signature);
    assert_eq!(loaded.session_id, original.session_id);
    // chrono rfc3339 round-trip truncates sub-second precision; compare at second resolution.
    assert_eq!(loaded.sealed_at.timestamp(), original.sealed_at.timestamp());
    assert!(
        loaded.verify(&verifying_key).is_ok(),
        "loaded seal must pass verify"
    );
}

#[tokio::test]
async fn load_returns_none_for_absent_seal() {
    let pool = fresh_pool().await;
    let result = CycleSeal::load(&pool, "01JZZZZZZZZZZZZZZZZZZZZZZ0")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[test]
fn operator_display_label_is_evening_summary() {
    assert_eq!(
        OPERATOR_DISPLAY_LABEL, "Evening summary",
        "terminology lock: CycleSeal must display as 'Evening summary' on operator surfaces"
    );
}
