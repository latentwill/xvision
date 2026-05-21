//! V2E eval-cost-model-per-bar-and-volume-share — slippage model wire tests.

use xvision_engine::eval::scenario::SlippageModel;

#[test]
fn volume_share_serde_tag() {
    let model = SlippageModel::VolumeShare {
        price_impact: 0.1,
        volume_limit: 0.025,
    };
    let json = serde_json::to_string(&model).unwrap();
    assert!(
        json.contains("\"model\":\"volume_share\""),
        "expected snake_case tag; got {json}"
    );
}

#[test]
fn linear_serde_tag() {
    let model = SlippageModel::Linear { bps: 10 };
    let json = serde_json::to_string(&model).unwrap();
    assert!(
        json.contains("\"model\":\"linear\""),
        "expected snake_case tag; got {json}"
    );
}

#[test]
fn none_serde_tag() {
    let model = SlippageModel::None;
    let json = serde_json::to_string(&model).unwrap();
    assert!(
        json.contains("\"model\":\"none\""),
        "expected snake_case tag; got {json}"
    );
}
