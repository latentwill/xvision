//! Bitfields v3 golden-parity + property tests.
//!
//! Fixtures frozen from the TypeScript reference implementation
//! (`frontend/web/src/features/marketplace/lib/genart{Grid,}.ts`).
//! Any divergence here means the Rust twin drifted from the normative engine.

use xvision_identity::{derive_traits, generate_svg, generate_token_uri};

#[derive(serde::Deserialize)]
struct Fixture {
    agent_id: String,
    manifest_hash: String,
    traits: FixtureTraits,
    svg: String,
    token_uri: String,
}
#[derive(serde::Deserialize)]
struct FixtureTraits {
    palette: String,
    symmetry: String,
    density: u32,
    layers: u32,
}

fn fixtures() -> Vec<Fixture> {
    serde_json::from_str(include_str!("fixtures/genart_v3.json")).expect("fixture parses")
}

#[test]
fn golden_parity_with_ts() {
    let fs = fixtures();
    assert_eq!(fs.len(), 24, "expected 24 golden fixtures");
    for f in fs {
        let svg = generate_svg(&f.agent_id, &f.manifest_hash).expect("svg");
        assert_eq!(svg, f.svg, "SVG parity failed for {}", f.agent_id);
        let uri = generate_token_uri(&f.agent_id, &f.manifest_hash).expect("uri");
        assert_eq!(uri, f.token_uri, "tokenURI parity failed for {}", f.agent_id);
        let t = derive_traits(&f.agent_id, &f.manifest_hash).expect("traits");
        assert_eq!(t.palette, f.traits.palette, "palette mismatch for {}", f.agent_id);
        assert_eq!(
            t.symmetry.as_str(),
            f.traits.symmetry,
            "symmetry mismatch for {}",
            f.agent_id
        );
        assert_eq!(t.density, f.traits.density, "density mismatch for {}", f.agent_id);
        assert_eq!(t.layers, f.traits.layers, "layers mismatch for {}", f.agent_id);
    }
}

#[test]
fn density_floor_holds_for_1000_seeds() {
    // floor is enforced pre-symmetry; post-symmetry density may differ but these 1000 seeds pin the observed behavior (TS-normative)
    let hash = "c".repeat(64);
    for i in 0..1000 {
        let t = derive_traits(&format!("01HXVNPROP{i}"), &hash).expect("traits");
        assert!(t.density >= 14, "seed {i} density {} below floor", t.density);
    }
}

#[test]
fn token_uri_size_ceiling() {
    let hash = "d".repeat(64);
    for i in 0..300 {
        let uri = generate_token_uri(&format!("01HXVNSZ{i}"), &hash).expect("uri");
        assert!(uri.len() <= 16 * 1024, "seed {i}: {} bytes", uri.len());
    }
}

#[test]
fn invalid_input_fails_loudly() {
    assert!(generate_token_uri("", &"a".repeat(64)).is_err());
    assert!(generate_token_uri("ok", "nothex").is_err());
    assert!(generate_token_uri("bad id!", &"a".repeat(64)).is_err());
    assert!(
        generate_token_uri("ok", &"A".repeat(64)).is_err(),
        "uppercase hex rejected"
    );
}
