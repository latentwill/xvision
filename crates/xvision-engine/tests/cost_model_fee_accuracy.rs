//! V2E eval-cost-model-per-bar-and-volume-share — wire-shape tests.

use xvision_engine::eval::scenario::{FeeSource, SlippageModel};

/// Verify fee_bps enum round-trips through serde.
#[test]
fn fee_source_serde_round_trip() {
    for src in [
        FeeSource::Default,
        FeeSource::ScenarioOverride,
        FeeSource::PerAssetOverride,
        FeeSource::PerBarArray,
    ] {
        let s = serde_json::to_string(&src).unwrap();
        let back: FeeSource = serde_json::from_str(&s).unwrap();
        assert_eq!(back, src, "FeeSource {:?} failed round-trip", src);
    }
}

/// Verify SlippageModel::VolumeShare round-trips through serde.
#[test]
fn volume_share_slippage_serde_round_trip() {
    let model = SlippageModel::VolumeShare {
        price_impact: 0.1,
        volume_limit: 0.025,
    };
    let s = serde_json::to_string(&model).unwrap();
    let back: SlippageModel = serde_json::from_str(&s).unwrap();
    assert_eq!(
        back,
        SlippageModel::VolumeShare {
            price_impact: 0.1,
            volume_limit: 0.025
        },
        "VolumeShare failed round-trip"
    );
}

/// Defaults for VolumeShare come through when fields are absent.
#[test]
fn volume_share_defaults_when_fields_absent() {
    let json = r#"{"model":"volume_share"}"#;
    let model: SlippageModel = serde_json::from_str(json).unwrap();
    assert_eq!(
        model,
        SlippageModel::VolumeShare {
            price_impact: 0.1,
            volume_limit: 0.025,
        }
    );
}
