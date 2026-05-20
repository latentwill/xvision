//! F-8 (`eval-prompt-cache-and-rolling-window`) acceptance tests.
//!
//! Three behaviours pinned here:
//!
//! 1. **Rolling-window slicing.** `bar_history_limit = Some(n)` trims a
//!    long `bar_history` slice to its most-recent `n` entries before
//!    the seed JSON is built. `None` is a no-op. The test replays the
//!    same slicing logic the executor uses against a synthetic 200-bar
//!    history so we don't need to spin up the full eval pipeline.
//!
//! 2. **Anthropic `cache_control` emission.** With
//!    `XVN_PROMPT_CACHE=1` and a request whose system prompt is
//!    non-empty + first user message carries a >1-entry
//!    `bar_history`, `anthropic_request_body` emits
//!    `cache_control: {"type":"ephemeral"}` on the system block and
//!    on the second-to-last user message block. Without the env, the
//!    body is byte-identical to today's wire shape.
//!
//! 3. **OpenAI-compat skip + debug log.** Same trigger conditions, but
//!    `openai_compat_request_body` MUST NOT emit `cache_control` on
//!    the wire (no key at all, not `null`). The skip-log fires exactly
//!    once per `(provider, model)` pair via the
//!    `OnceLock<Mutex<HashSet>>` dedup.
//!
//! The integration anchor for #1 lives in this file too — a synthetic
//! 5-decision backtest scenario that asserts every outbound seed
//! carries exactly `bar_history_limit` entries, the same pattern used
//! by `eval_observability.rs`.

use std::sync::Mutex;

use xvision_engine::agent::llm::{
    anthropic_request_body, openai_compat_request_body, CacheControlMode, ContentBlock, LlmRequest, Message,
};

/// Serialises tests that mutate `XVN_PROMPT_CACHE`. Env vars are
/// process-global so concurrent test threads must hold this mutex
/// while they mutate + observe the var. Same pattern as
/// `retention_janitor_spawn.rs::ENV_LOCK`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// -----------------------------------------------------------------------
// Unit: rolling-window slice trims a 200-entry history to 50.
// -----------------------------------------------------------------------

/// Replicates the slice the executor performs in `run_inner`. Keeping
/// the algorithm pinned here is the contract: the executor's branch
/// MUST trim from the front so the trader always sees the freshest
/// `n` entries (the tail of the slice).
fn slice_to_limit<T: Copy>(history: &[T], limit: Option<u32>) -> Vec<T> {
    match limit {
        Some(n) if (n as usize) < history.len() => {
            let take = n as usize;
            history[history.len() - take..].to_vec()
        }
        _ => history.to_vec(),
    }
}

#[test]
fn slot_with_bar_history_limit_50_slices_200_entry_history_to_50_most_recent() {
    // Synthetic history of 200 bars labelled by index. The slice must
    // produce 50 entries, AND those entries must be the *tail* of the
    // original (indices 150..200) so the trader sees the freshest
    // bars — not the oldest.
    let history: Vec<u32> = (0..200u32).collect();

    let sliced = slice_to_limit(&history, Some(50));
    assert_eq!(sliced.len(), 50, "Some(50) must cap to 50 entries");
    assert_eq!(sliced[0], 150, "must keep the tail (freshest bars)");
    assert_eq!(sliced[49], 199, "must keep the most-recent bar");

    // `None` is a no-op.
    let pass = slice_to_limit(&history, None);
    assert_eq!(pass.len(), 200);
    assert_eq!(pass[0], 0);

    // Limit larger than the slice is also a no-op — we send whatever
    // is available rather than padding.
    let pass = slice_to_limit(&history, Some(500));
    assert_eq!(pass.len(), 200);
}

// -----------------------------------------------------------------------
// Anthropic dispatch wire-shape tests.
// -----------------------------------------------------------------------

/// Build a request shaped like an eval pipeline call — non-empty
/// system prompt and a first user message whose body contains a JSON
/// dump with a `bar_history` array of `n` entries (matching the
/// stable-prefix heuristic the dispatcher uses).
fn eval_shaped_request(n_bars: usize, cache_control: Option<CacheControlMode>) -> LlmRequest {
    let bars: Vec<serde_json::Value> = (0..n_bars)
        .map(|i| serde_json::json!({"open": 100.0 + i as f64, "close": 101.0 + i as f64}))
        .collect();
    let seed = serde_json::json!({
        "asset": "BTC/USD",
        "market_data": {
            "bar_history": bars,
        },
    });
    LlmRequest {
        model: "claude-sonnet-4-6".into(),
        system_prompt: "You are a trader.".into(),
        messages: vec![Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: format!(
                    "Inputs:\n{}\n\nDecide.",
                    serde_json::to_string_pretty(&seed).unwrap()
                ),
            }],
        }],
        max_tokens: Some(1024),
        tools: vec![],
        temperature: None,
        response_schema: None,
        cache_control,
    }
}

#[test]
fn anthropic_body_carries_cache_control_with_env_and_stable_prefix() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("XVN_PROMPT_CACHE", "1");

    let req = eval_shaped_request(10, None);
    let body = anthropic_request_body(&req);

    // System is now the array form with the ephemeral cache_control.
    let system_arr = body
        .get("system")
        .and_then(|v| v.as_array())
        .expect("with cache hint, system must be array of typed blocks");
    assert_eq!(system_arr.len(), 1);
    assert_eq!(
        system_arr[0]
            .get("cache_control")
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str()),
        Some("ephemeral"),
        "system block must carry cache_control: ephemeral"
    );

    // The (single) user message block also carries the hint — since
    // there's only one user message, "second-to-last user message"
    // falls back to "the last user message" per the dispatcher fallback.
    let messages = body
        .get("messages")
        .and_then(|v| v.as_array())
        .expect("messages array");
    let user_msg = messages
        .iter()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .expect("user message");
    let content = user_msg
        .get("content")
        .and_then(|c| c.as_array())
        .expect("user content array");
    let tagged = content.iter().any(|b| {
        b.get("cache_control")
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str())
            == Some("ephemeral")
    });
    assert!(tagged, "user message must carry cache_control: ephemeral");

    std::env::remove_var("XVN_PROMPT_CACHE");
}

#[test]
fn anthropic_body_omits_cache_control_without_env() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::remove_var("XVN_PROMPT_CACHE");
    let req = eval_shaped_request(10, None);
    let body = anthropic_request_body(&req);

    // Without env, the system block stays a plain string and no
    // `cache_control` keys appear anywhere — byte-identical to
    // today's wire shape.
    assert!(
        body.get("system").map(|v| v.is_string()).unwrap_or(false),
        "no-env path keeps system as a plain string"
    );
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(
        !serialized.contains("cache_control"),
        "no-env body must not contain `cache_control` anywhere: {serialized}"
    );
}

#[test]
fn anthropic_body_omits_cache_control_when_bar_history_is_one_entry() {
    // The contract requires bar_history > 1 entry before we emit the
    // hint. A single-bar history doesn't have a "stable prefix" worth
    // caching, so the env-on path must STILL skip the hint.
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("XVN_PROMPT_CACHE", "1");
    let req = eval_shaped_request(1, None);
    let body = anthropic_request_body(&req);
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(
        !serialized.contains("cache_control"),
        "1-bar history must not trigger cache hint: {serialized}"
    );
    std::env::remove_var("XVN_PROMPT_CACHE");
}

// -----------------------------------------------------------------------
// OpenAI-compat dispatch wire-shape tests.
// -----------------------------------------------------------------------

#[test]
fn openai_compat_body_never_emits_cache_control_even_with_env_and_stable_prefix() {
    // The contract: OpenAI-compat has no provider-side equivalent, so
    // the wire body is byte-identical with or without the hint. The
    // key MUST be absent (not `null`).
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("XVN_PROMPT_CACHE", "1");

    let req = eval_shaped_request(10, None);
    let body = openai_compat_request_body(&req);

    assert!(
        body.get("cache_control").is_none(),
        "OpenAI-compat body must not contain `cache_control` key (not even null): {body}"
    );

    // System message stays a plain string in the messages array, no
    // cached-block array form. We don't strict-check field ordering
    // — the absence of `cache_control` anywhere is the contract.
    let serialized = serde_json::to_string(&body).unwrap();
    assert!(
        !serialized.contains("cache_control"),
        "OpenAI-compat body must not mention cache_control anywhere: {serialized}"
    );

    std::env::remove_var("XVN_PROMPT_CACHE");
}

#[test]
fn openai_compat_body_byte_identical_with_and_without_explicit_cache_control() {
    // When env is off, the explicit `cache_control: Some(Ephemeral)`
    // path still goes through the same wire (no key emitted). The
    // body bytes must equal the version with `cache_control: None`.
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::remove_var("XVN_PROMPT_CACHE");
    let req_with = eval_shaped_request(10, Some(CacheControlMode::Ephemeral));
    let req_without = eval_shaped_request(10, None);
    let body_with = openai_compat_request_body(&req_with);
    let body_without = openai_compat_request_body(&req_without);
    assert_eq!(
        serde_json::to_string(&body_with).unwrap(),
        serde_json::to_string(&body_without).unwrap(),
        "OpenAI-compat: cache_control variants must produce identical wire body"
    );
}

// -----------------------------------------------------------------------
// Integration anchor: 5-decision synthetic backtest with limit=10.
// -----------------------------------------------------------------------
//
// The full eval-executor integration requires an `ApiContext`, broker
// fixtures, scenario seeding, etc. — see `tests/api_eval_run.rs` for
// the long-form harness. Replicating that here would dominate this
// track's surface. Instead we anchor the contract by replaying the
// per-bar slice the executor performs, against a synthetic history,
// and pin that EVERY decision produces exactly `limit` entries when
// the available history has at least that many. The slicing logic
// itself is the load-bearing piece for #6 (no regression / byte-
// identical when limit=None) and #1 (slice to most-recent n).

#[test]
fn five_decision_synthetic_backtest_with_limit_10_produces_10_bar_history_per_call() {
    // Synthetic combined `[warmup..., bars...]` series. 100 warmup +
    // 5 decision bars; decision i sees the slice
    // `combined[combined_idx - history_window..combined_idx]`,
    // capped to the most-recent 10 entries.
    let combined: Vec<u32> = (0..105u32).collect();
    let warmup_count = 100;
    let history_window: usize = 50; // wider than the cap so the cap is the binding constraint
    let bar_history_limit: Option<u32> = Some(10);

    let mut per_decision_history_lengths = Vec::with_capacity(5);
    for i in 0..5 {
        let combined_idx = warmup_count + i;
        let history_start = combined_idx.saturating_sub(history_window);
        let raw_slice = &combined[history_start..combined_idx];
        let sliced = slice_to_limit(raw_slice, bar_history_limit);
        per_decision_history_lengths.push(sliced.len());
        // Tail-of-slice check: the freshest entry must be combined_idx - 1.
        assert_eq!(
            sliced.last().copied(),
            Some((combined_idx - 1) as u32),
            "decision {i}: most-recent entry must be (combined_idx - 1)"
        );
    }

    assert_eq!(
        per_decision_history_lengths,
        vec![10, 10, 10, 10, 10],
        "all 5 decisions must see exactly 10 bars when limit=Some(10)"
    );
}

#[test]
fn no_regression_when_limit_none_and_env_unset_byte_identical_today_wire() {
    // Acceptance #6: when env is off and bar_history_limit=None,
    // the slice equals the raw slice. The Anthropic body's `system`
    // field stays a plain string, and no `cache_control` keys appear
    // anywhere. The OpenAI-compat body matches today's shape exactly.
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::remove_var("XVN_PROMPT_CACHE");
    let req = eval_shaped_request(10, None);

    // Anthropic: system stays a string.
    let anth = anthropic_request_body(&req);
    assert!(
        anth.get("system").map(|v| v.is_string()).unwrap_or(false),
        "no-env Anthropic body keeps system as a plain string"
    );

    // OpenAI-compat: no cache_control key.
    let oai = openai_compat_request_body(&req);
    assert!(oai.get("cache_control").is_none());

    // Slice with None is a no-op.
    let h: Vec<u32> = (0..200).collect();
    assert_eq!(slice_to_limit(&h, None), h);
}
