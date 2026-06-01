use xvision_engine::autooptimizer::content_hash::{
    canonical_json, canonicalize_json, hash_bytes, hash_canonical_json, ContentHash,
};

#[test]
fn same_logical_value_same_hash_regardless_of_key_order() {
    let a = serde_json::json!({"b": 1, "a": 2, "nested": {"y": "two", "x": "one"}});
    let b = serde_json::json!({"a": 2, "b": 1, "nested": {"x": "one", "y": "two"}});
    assert_eq!(ContentHash::of_json(&a), ContentHash::of_json(&b));
}

#[test]
fn different_value_different_hash() {
    let a = serde_json::json!({"x": 1});
    let b = serde_json::json!({"x": 2});
    assert_ne!(ContentHash::of_json(&a), ContentHash::of_json(&b));
}

#[test]
fn hash_string_is_64_hex_chars() {
    let h = ContentHash::of_bytes(b"hello world");
    assert_eq!(h.to_hex().len(), 64);
    assert!(h.to_hex().chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn hex_round_trip_via_fromstr_display() {
    let h = ContentHash::of_bytes(b"x");
    let displayed = format!("{}", h);
    let parsed: ContentHash = displayed.parse().unwrap();
    assert_eq!(h, parsed);
}

#[test]
fn canonicalize_orders_object_keys() {
    let v = serde_json::json!({"z": 1, "a": 2, "m": 3});
    let c = canonicalize_json(&v);
    let s = serde_json::to_string(&c).unwrap();
    assert_eq!(s, r#"{"a":2,"m":3,"z":1}"#);
}

#[test]
fn canonical_json_fn_returns_sorted_string() {
    let v = serde_json::json!({"z": 1, "a": 2});
    assert_eq!(canonical_json(&v), r#"{"a":2,"z":1}"#);
}

#[test]
fn arrays_of_objects_sorted_recursively() {
    let v = serde_json::json!([{"b": 1, "a": 2}, {"d": 3, "c": 4}]);
    let c = canonicalize_json(&v);
    let s = serde_json::to_string(&c).unwrap();
    assert_eq!(s, r#"[{"a":2,"b":1},{"c":4,"d":3}]"#);
}

#[test]
fn known_blake3_vector_empty() {
    const EXPECTED: &str =
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";
    assert_eq!(hash_bytes(b"").to_hex(), EXPECTED);
}

#[test]
fn known_blake3_vector_abc() {
    const EXPECTED: &str =
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85";
    assert_eq!(hash_bytes(b"abc").to_hex(), EXPECTED);
}

#[test]
fn from_hex_rejects_wrong_length() {
    assert!(ContentHash::from_hex("af13").is_err());
    assert!(ContentHash::from_hex("").is_err());
    // 66 hex chars = 33 bytes
    assert!(ContentHash::from_hex(&"af".repeat(33)).is_err());
}

#[test]
fn from_hex_rejects_invalid_hex_chars() {
    let bad = "zz".repeat(32);
    assert!(ContentHash::from_hex(&bad).is_err());
}

#[test]
fn serde_round_trip_as_hex_string() {
    let h = ContentHash::of_bytes(b"serde test");
    let json = serde_json::to_string(&h).unwrap();
    // must be a JSON string, not an array
    assert!(json.starts_with('"') && json.ends_with('"'));
    let inner = &json[1..json.len() - 1];
    assert_eq!(inner.len(), 64);
    let h2: ContentHash = serde_json::from_str(&json).unwrap();
    assert_eq!(h, h2);
}

#[test]
fn hash_canonical_json_matches_of_json() {
    let v = serde_json::json!({"b": 2, "a": 1});
    assert_eq!(hash_canonical_json(&v), ContentHash::of_json(&v));
}

#[test]
fn scalars_pass_through_canonicalize_unchanged() {
    let null = serde_json::Value::Null;
    let bool_t = serde_json::Value::Bool(true);
    let num = serde_json::json!(42);
    let s = serde_json::Value::String("hello".into());
    assert_eq!(canonicalize_json(&null), null);
    assert_eq!(canonicalize_json(&bool_t), bool_t);
    assert_eq!(canonicalize_json(&num), num);
    assert_eq!(canonicalize_json(&s), s);
}
