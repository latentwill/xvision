use tempfile::TempDir;
use ulid::Ulid;
use xvision_engine::autoresearch::{
    config::AutoresearchConfig,
    content_hash::ContentHash,
    session::{load_or_generate_key, SessionCommitment},
};

#[test]
fn key_generate_and_load_round_trips() {
    let dir = TempDir::new().unwrap();
    let key_path = dir.path().join("operator.ed25519");
    let key1 = load_or_generate_key(&key_path).unwrap();
    let key2 = load_or_generate_key(&key_path).unwrap();
    assert_eq!(key1.to_bytes(), key2.to_bytes());
}

#[test]
fn new_signed_and_verify_succeed() {
    let dir = TempDir::new().unwrap();
    let key_path = dir.path().join("operator.ed25519");
    let key = load_or_generate_key(&key_path).unwrap();
    let commitment =
        SessionCommitment::new_signed(Ulid::new(), &AutoresearchConfig::default(), vec![], &key)
            .unwrap();
    commitment.verify(&key.verifying_key()).unwrap();
}

#[test]
fn verify_fails_on_tampered_fields() {
    let dir = TempDir::new().unwrap();
    let key_path = dir.path().join("operator.ed25519");
    let key = load_or_generate_key(&key_path).unwrap();
    let mut commitment =
        SessionCommitment::new_signed(Ulid::new(), &AutoresearchConfig::default(), vec![], &key)
            .unwrap();
    commitment.config_hash = ContentHash([0u8; 32]);
    assert!(
        commitment.verify(&key.verifying_key()).is_err(),
        "verification must fail on tampered config_hash"
    );
}

#[test]
fn write_to_and_load_from_round_trip() {
    let dir = TempDir::new().unwrap();
    let key_path = dir.path().join("operator.ed25519");
    let key = load_or_generate_key(&key_path).unwrap();
    let commitment =
        SessionCommitment::new_signed(Ulid::new(), &AutoresearchConfig::default(), vec![], &key)
            .unwrap();
    let path = commitment.write_to(dir.path()).unwrap();
    let loaded = SessionCommitment::load_from(&path).unwrap();
    assert_eq!(commitment, loaded);
}

#[test]
#[cfg(unix)]
fn key_file_permissions_are_0600() {
    use std::os::unix::fs::MetadataExt;
    let dir = TempDir::new().unwrap();
    let key_path = dir.path().join("operator.ed25519");
    load_or_generate_key(&key_path).unwrap();
    let metadata = std::fs::metadata(&key_path).unwrap();
    assert_eq!(metadata.mode() & 0o777, 0o600, "key file must be mode 0600");
}

#[test]
fn verify_fails_with_wrong_key() {
    let dir = TempDir::new().unwrap();
    let key1 = load_or_generate_key(&dir.path().join("key1.ed25519")).unwrap();
    let key2 = load_or_generate_key(&dir.path().join("key2.ed25519")).unwrap();
    let commitment =
        SessionCommitment::new_signed(Ulid::new(), &AutoresearchConfig::default(), vec![], &key1)
            .unwrap();
    assert!(
        commitment.verify(&key2.verifying_key()).is_err(),
        "verification must fail with a different public key"
    );
}

#[test]
fn pub_key_file_created_alongside_secret() {
    let dir = TempDir::new().unwrap();
    let key_path = dir.path().join("operator.ed25519");
    load_or_generate_key(&key_path).unwrap();
    let pub_path = dir.path().join("operator.ed25519.pub");
    assert!(pub_path.exists(), "public key file must exist alongside the secret key");
    let pub_bytes = std::fs::read(&pub_path).unwrap();
    assert_eq!(pub_bytes.len(), 32, "public key file must contain exactly 32 bytes");
}

#[test]
fn new_signed_with_parents_verifies() {
    let dir = TempDir::new().unwrap();
    let key = load_or_generate_key(&dir.path().join("operator.ed25519")).unwrap();
    let parents = vec![ContentHash([1u8; 32]), ContentHash([2u8; 32])];
    let commitment =
        SessionCommitment::new_signed(Ulid::new(), &AutoresearchConfig::default(), parents, &key)
            .unwrap();
    commitment.verify(&key.verifying_key()).unwrap();
    assert_eq!(commitment.parent_strategy_hashes.len(), 2);
}
