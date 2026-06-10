//! Completeness gate: every Orderly symbol in the live snapshot must appear
//! exactly once in config/whitelist.toml as a `venues.orderly` value.

use std::collections::HashSet;

const SNAPSHOT: &str = include_str!("fixtures/orderly_info_snapshot.json");
const WHITELIST_TOML: &str = include_str!("../../../config/whitelist.toml");

#[test]
fn every_orderly_snapshot_symbol_is_in_whitelist() {
    let snapshot: Vec<String> = serde_json::from_str(SNAPSHOT).expect("parse snapshot");

    // Collect all orderly venue values from whitelist.toml
    // Parse as TOML manually: look for lines that set `orderly  = "..."` under [assets.venues]
    let mut whitelist_orderly: HashSet<String> = HashSet::new();
    let mut in_venues = false;
    for line in WHITELIST_TOML.lines() {
        let trimmed = line.trim();
        if trimmed == "[assets.venues]" {
            in_venues = true;
        } else if trimmed.starts_with("[[assets]]") || trimmed.starts_with('[') {
            if !trimmed.starts_with("[assets.venues]") {
                in_venues = false;
            }
        } else if in_venues {
            // Look for: orderly  = "PERP_BTC_USDC"
            if let Some(rest) = trimmed.strip_prefix("orderly") {
                let val = rest.trim_start_matches(|c: char| c == ' ' || c == '=').trim().trim_matches('"');
                if !val.is_empty() {
                    whitelist_orderly.insert(val.to_string());
                }
            }
        }
    }

    let snapshot_set: HashSet<&str> = snapshot.iter().map(|s| s.as_str()).collect();
    let missing: Vec<&&str> = snapshot_set.iter()
        .filter(|&&sym| !whitelist_orderly.contains(sym))
        .collect();

    assert!(
        missing.is_empty(),
        "Snapshot symbols missing from whitelist.toml orderly venue:\n{:?}",
        missing
    );
    assert_eq!(
        whitelist_orderly.len(),
        snapshot.len(),
        "Whitelist orderly count ({}) != snapshot count ({})",
        whitelist_orderly.len(),
        snapshot.len()
    );
}

#[test]
fn orderly_snapshot_count_matches_fixture() {
    let snapshot: Vec<String> = serde_json::from_str(SNAPSHOT).expect("parse snapshot");
    assert_eq!(snapshot.len(), 102, "Expected 102 symbols in snapshot, got {}", snapshot.len());
}
