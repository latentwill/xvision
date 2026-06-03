//! F-8 (`eval-prompt-cache-and-rolling-window`) acceptance tests.
//!
//! Two behaviours pinned here:
//!
//! 1. **Anthropic `cache_control` emission.** With
//!    `XVN_PROMPT_CACHE=1` and a request whose system prompt is
//!    non-empty + first user message carries a >1-entry
//!    `bar_history`, `anthropic_request_body` emits
//!    `cache_control: {"type":"ephemeral"}` on the system block and
//!    on the second-to-last user message block. Without the env, the
//!    body is byte-identical to today's wire shape.
//!
//! 2. **OpenAI-compat skip + debug log.** Same trigger conditions, but
//!    `openai_compat_request_body` MUST NOT emit `cache_control` on
//!    the wire (no key at all, not `null`). The skip-log fires exactly
//!    once per `(provider, model)` pair via the
//!    `OnceLock<Mutex<HashSet>>` dedup.

use std::sync::{Arc, Mutex};

use tracing_subscriber::prelude::*;
use xvision_engine::agent::llm::{
    anthropic_request_body, openai_compat_request_body, CacheControlMode, ContentBlock, LlmRequest, Message,
};

/// Serialises tests that mutate `XVN_PROMPT_CACHE`. Env vars are
/// process-global so concurrent test threads must hold this mutex
/// while they mutate + observe the var. Same pattern as
/// `retention_janitor_spawn.rs::ENV_LOCK`.
static ENV_LOCK: Mutex<()> = Mutex::new(());
const OPENAI_CACHE_SKIP_MESSAGE: &str =
    "prompt cache hint requested but OpenAI-compat has no provider-side equivalent; skipping";

#[derive(Clone, Debug, PartialEq, Eq)]
struct CacheSkipLog {
    provider: String,
    model: String,
    message: String,
}

#[derive(Clone, Default)]
struct CapturedCacheSkipLogs {
    entries: Arc<Mutex<Vec<CacheSkipLog>>>,
}

struct CacheSkipLayer {
    captured: CapturedCacheSkipLogs,
}

impl<S> tracing_subscriber::Layer<S> for CacheSkipLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target() != "xvision::llm" {
            return;
        }

        let mut visitor = CacheSkipVisitor::default();
        event.record(&mut visitor);
        if visitor.message.as_deref() == Some(OPENAI_CACHE_SKIP_MESSAGE) {
            self.captured.entries.lock().unwrap().push(CacheSkipLog {
                provider: visitor.provider.unwrap_or_default(),
                model: visitor.model.unwrap_or_default(),
                message: visitor.message.unwrap_or_default(),
            });
        }
    }
}

#[derive(Default)]
struct CacheSkipVisitor {
    provider: Option<String>,
    model: Option<String>,
    message: Option<String>,
}

impl tracing::field::Visit for CacheSkipVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "provider" => self.provider = Some(value.to_string()),
            "model" => self.model = Some(value.to_string()),
            "message" => self.message = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let value = format!("{value:?}").trim_matches('"').to_string();
        match field.name() {
            "provider" => self.provider = Some(value),
            "model" => self.model = Some(value),
            "message" => self.message = Some(value),
            _ => {}
        }
    }
}

fn eval_prompt_text(n_bars: usize) -> String {
    let bars: Vec<serde_json::Value> = (0..n_bars)
        .map(|i| serde_json::json!({"open": 100.0 + i as f64, "close": 101.0 + i as f64}))
        .collect();
    let seed = serde_json::json!({
        "asset": "BTC/USD",
        "market_data": {
            "bar_history": bars,
        },
    });
    format!(
        "Inputs:\n{}\n\nDecide.",
        serde_json::to_string_pretty(&seed).unwrap()
    )
}

/// Build a request shaped like an eval pipeline call — non-empty
/// system prompt and a first user message whose body contains a JSON
/// dump with a `bar_history` array of `n` entries (matching the
/// stable-prefix heuristic the dispatcher uses).
fn eval_shaped_request(n_bars: usize, cache_control: Option<CacheControlMode>) -> LlmRequest {
    LlmRequest {
        model: "claude-sonnet-4-6".into(),
        system_prompt: "You are a trader.".into(),
        messages: vec![Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: eval_prompt_text(n_bars),
            }],
        }],
        max_tokens: Some(1024),
        tools: vec![],
        temperature: None,
        response_schema: None,
        cache_control,
    }
}

fn expected_anthropic_no_cache_body(n_bars: usize) -> serde_json::Value {
    serde_json::json!({
        "model": "claude-sonnet-4-6",
        "max_tokens": 1024,
        "system": "You are a trader.",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": eval_prompt_text(n_bars),
                    }
                ]
            }
        ],
    })
}

fn expected_openai_compat_body(n_bars: usize) -> serde_json::Value {
    serde_json::json!({
        "model": "claude-sonnet-4-6",
        "messages": [
            {
                "role": "system",
                "content": "You are a trader.",
            },
            {
                "role": "user",
                "content": eval_prompt_text(n_bars),
            }
        ],
        "max_tokens": 1024,
    })
}

fn eval_shaped_request_for_model(model: String) -> LlmRequest {
    let mut req = eval_shaped_request(10, None);
    req.model = model;
    req
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
    assert_eq!(
        body,
        expected_anthropic_no_cache_body(10),
        "no-env Anthropic body must match the legacy wire shape exactly"
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
    let req = eval_shaped_request(10, None);

    std::env::remove_var("XVN_PROMPT_CACHE");
    let env_off_body = openai_compat_request_body(&req);

    std::env::set_var("XVN_PROMPT_CACHE", "1");
    let body = openai_compat_request_body(&req);

    assert_eq!(
        serde_json::to_string(&body).unwrap(),
        serde_json::to_string(&env_off_body).unwrap(),
        "OpenAI-compat env-on cache hint path must be byte-identical to env-off"
    );
    assert_eq!(
        body,
        expected_openai_compat_body(10),
        "OpenAI-compat body must match the legacy wire shape exactly"
    );

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

#[test]
fn openai_compat_cache_skip_log_dedups_once_per_provider_model_pair() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("XVN_PROMPT_CACHE", "1");

    let repeated_model = format!("cache-skip-dedup-a-{}-{}", std::process::id(), line!());
    let distinct_model = format!("cache-skip-dedup-b-{}-{}", std::process::id(), line!());
    let repeated_req = eval_shaped_request_for_model(repeated_model.clone());
    let distinct_req = eval_shaped_request_for_model(distinct_model.clone());
    let captured = CapturedCacheSkipLogs::default();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::DEBUG)
        .with(CacheSkipLayer {
            captured: captured.clone(),
        });
    let _subscriber_guard = tracing::subscriber::set_default(subscriber);

    let _ = openai_compat_request_body(&repeated_req);
    let _ = openai_compat_request_body(&repeated_req);
    let _ = openai_compat_request_body(&distinct_req);

    let entries = captured.entries.lock().unwrap().clone();
    assert_eq!(
        entries.len(),
        2,
        "skip log must fire once for the repeated model and once for the distinct model: {entries:?}"
    );
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.provider == "openai-compat" && entry.model == repeated_model)
            .count(),
        1,
        "repeated provider/model pair must log exactly once"
    );
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.provider == "openai-compat" && entry.model == distinct_model)
            .count(),
        1,
        "distinct provider/model pair must log once independently"
    );

    std::env::remove_var("XVN_PROMPT_CACHE");
}

#[test]
fn no_regression_when_limit_none_and_env_unset_byte_identical_today_wire() {
    // When env is off, the Anthropic body's `system` field stays a plain
    // string and no `cache_control` keys appear anywhere. The OpenAI-compat
    // body matches today's shape exactly.
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::remove_var("XVN_PROMPT_CACHE");
    let req = eval_shaped_request(10, None);

    let anth = anthropic_request_body(&req);
    assert_eq!(
        anth,
        expected_anthropic_no_cache_body(10),
        "no-env Anthropic body must match the legacy wire shape exactly"
    );

    let oai = openai_compat_request_body(&req);
    assert_eq!(
        oai,
        expected_openai_compat_body(10),
        "OpenAI-compat body must match the legacy wire shape exactly"
    );
}
