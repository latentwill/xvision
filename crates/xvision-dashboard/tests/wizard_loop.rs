//! Wizard folder-recall honesty regression tests.
//!
//! These tests exercise the three acceptance criteria from the
//! `wizard-folder-recall-honesty` contract (2026-05-21):
//!
//! 1. **Non-empty folder** — when `list_strategies_folder` returns ≥1 entry
//!    the wizard narrative must cite at least one `rel_path` and must not
//!    claim the folder is empty.
//!
//! 2. **Empty folder + named pattern** — when both folder tools return empty
//!    and the operator asks for a named pattern (fibonacci, RSI, etc.) the
//!    wizard must offer prepop (`xvn strategies init`) before jumping to
//!    `create_strategy`.
//!
//! 3. **Empty folder + general request** — when both folder tools return
//!    empty and the operator asks generically, the wizard may offer prepop
//!    OR proceed to `create_strategy` after explicit operator consent.
//!    Documented choice: the wizard offers BOTH options in the same turn
//!    (prepop + blank draft) so the operator decides. Test verifies the
//!    wizard does not silently skip the prepop offer.
//!
//! The tests wrap `MockDispatch::sequence` in a request recorder. The folder
//! tool calls are routed to the real tempdir-backed filesystem, and the
//! assertions verify the follow-up model request contains the real tool
//! results plus the folder-recall policy that should drive the final answer.

use std::sync::{Arc, Mutex};

use tempfile::TempDir;
use tokio::fs;
use xvision_dashboard::wizard_loop::{WizardEvent, WizardLoop};
use xvision_dashboard::AppState;
use xvision_engine::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, MockDispatch, StopReason,
};
use xvision_engine::chat_session::{ChatSessionStore, ContextScope};

// ── helpers ────────────────────────────────────────────────────────────────

async fn boot() -> (AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init AppState");
    (state, tmp)
}

/// Drain all events from the wizard loop and return them.
async fn drain(wl: &mut WizardLoop) -> Vec<WizardEvent> {
    let mut out = vec![];
    while let Some(ev) = wl.next_event().await {
        out.push(ev);
    }
    out
}

/// Collect all text tokens emitted during the loop run, concatenated.
fn all_tokens(events: &[WizardEvent]) -> String {
    events
        .iter()
        .filter_map(|ev| match ev {
            WizardEvent::Token { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Return the names of every ToolCall event in the event stream.
fn tool_call_names(events: &[WizardEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|ev| match ev {
            WizardEvent::ToolCall { tool, .. } => Some(tool.clone()),
            _ => None,
        })
        .collect()
}

struct RecordingDispatch {
    inner: MockDispatch,
    requests: Arc<Mutex<Vec<LlmRequest>>>,
}

#[async_trait::async_trait]
impl LlmDispatch for RecordingDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        self.requests.lock().unwrap().push(req.clone());
        self.inner.complete(req).await
    }
}

fn recording_sequence(responses: Vec<LlmResponse>) -> (Arc<dyn LlmDispatch>, Arc<Mutex<Vec<LlmRequest>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let dispatch: Arc<dyn LlmDispatch> = Arc::new(RecordingDispatch {
        inner: MockDispatch::sequence(responses),
        requests: requests.clone(),
    });
    (dispatch, requests)
}

fn request_text(req: &LlmRequest) -> String {
    format!(
        "{}\n{}",
        req.system_prompt,
        serde_json::to_string(&req.messages).expect("messages serialize")
    )
}

fn assert_contains(haystack: &str, needle: &str, context: &str) {
    assert!(
        haystack.contains(needle),
        "{context} must contain {needle:?}; got: {haystack}"
    );
}

// ── test 1: non-empty folder regression ────────────────────────────────────

/// When `list_strategies_folder` returns 3 entries the wizard narrative must
/// reference at least one `rel_path`.  The test also asserts that no
/// "folder is empty" / "folder appears empty" fragment appears in the
/// narrative — catching the 2026-05-21 transcript bug where the wizard
/// narrated emptiness despite the tool returning results.
#[tokio::test]
async fn non_empty_folder_wizard_cites_rel_path() {
    let (state, tmp) = boot().await;

    // Seed the strategies folder with 3 known files.
    let strat_dir = tmp.path().join("strategies");
    fs::create_dir_all(strat_dir.join("notes")).await.unwrap();
    fs::create_dir_all(strat_dir.join("docs")).await.unwrap();
    fs::create_dir_all(strat_dir.join("strategy-files"))
        .await
        .unwrap();

    let entry_a = "notes/macd-notes.md";
    let entry_b = "docs/rsi-reference.txt";
    let entry_c = "strategy-files/btc-breakout.json";

    fs::write(strat_dir.join("notes/macd-notes.md"), b"# MACD notes")
        .await
        .unwrap();
    fs::write(strat_dir.join("docs/rsi-reference.txt"), b"RSI reference")
        .await
        .unwrap();
    fs::write(strat_dir.join("strategy-files/btc-breakout.json"), b"{}")
        .await
        .unwrap();

    // Script the model: (1) call list_strategies_folder, then (2) emit a
    // narrative that cites one of the returned paths. The recorder asserts
    // that the second request contains the real tool result rel_paths, so the
    // canned final text is not the only source of coverage.
    let (mock, requests) = recording_sequence(vec![
        MockDispatch::tool_use("tu_1", "list_strategies_folder", serde_json::json!({})),
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: format!(
                    "Your strategies folder contains 3 files: {entry_a}, {entry_b}, and {entry_c}."
                ),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 10,
            output_tokens: 20,
        },
    ]);

    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let mut wl = WizardLoop::new(
        tmp.path().to_path_buf(),
        mock,
        "claude-sonnet-4-6".into(),
        state.pool.clone(),
        session_id,
        ContextScope::Workspace,
        "what do I have in my strategies folder".into(),
    )
    .await
    .unwrap();

    let events = drain(&mut wl).await;
    let narrative = all_tokens(&events);
    let requests = requests.lock().unwrap();
    assert!(
        requests.len() >= 2,
        "tool-use loop must dispatch again after list_strategies_folder"
    );
    let second_request = request_text(&requests[1]);
    assert_contains(
        &second_request,
        entry_a,
        "second wizard request after non-empty folder result",
    );
    assert_contains(
        &second_request,
        entry_b,
        "second wizard request after non-empty folder result",
    );
    assert_contains(
        &second_request,
        entry_c,
        "second wizard request after non-empty folder result",
    );
    assert_contains(
        &second_request,
        "cite the returned entries by their `rel_path`",
        "wizard system prompt",
    );

    // Must cite at least one of the returned rel_paths.
    let cites_a_path =
        narrative.contains(entry_a) || narrative.contains(entry_b) || narrative.contains(entry_c);
    assert!(
        cites_a_path,
        "wizard narrative must cite at least one rel_path from the folder result; \
         got: {narrative:?}"
    );

    // Must NOT claim the folder is empty.
    let empty_phrases = [
        "folder is empty",
        "folder appears empty",
        "folder was empty",
        "no files",
        "nothing in",
    ];
    for phrase in empty_phrases {
        assert!(
            !narrative.to_ascii_lowercase().contains(phrase),
            "wizard narrative must not contain '{phrase}' when folder has entries; \
             got: {narrative:?}"
        );
    }
}

// ── test 2: empty folder + named pattern ───────────────────────────────────

/// When both folder tools return empty AND the operator named a pattern
/// (fibonacci + RSI), the wizard must offer prepop (`xvn strategies init`
/// or "init") before reaching `create_strategy`.
///
/// The test asserts that no `create_strategy` ToolCall appears in the event
/// stream — the wizard is required to surface the prepop offer first.
#[tokio::test]
async fn empty_folder_named_pattern_offers_prepop_not_create_strategy() {
    let (state, tmp) = boot().await;
    // No files under strategies/ — folder missing → list returns [].

    // Script: (1) call list_strategies_folder (returns []), (2) call
    // list_strategy_ideas (returns []), (3) emit prepop offer narrative.
    let (mock, requests) = recording_sequence(vec![
        MockDispatch::tool_use("tu_1", "list_strategies_folder", serde_json::json!({})),
        MockDispatch::tool_use("tu_2", "list_strategy_ideas", serde_json::json!({})),
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: "Your strategies folder is empty. I can run \
                       `xvn strategies init` to seed it with curated examples \
                       including Fibonacci and RSI strategies. Would you like me to do that?"
                    .into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 10,
            output_tokens: 30,
        },
    ]);

    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let mut wl = WizardLoop::new(
        tmp.path().to_path_buf(),
        mock,
        "claude-sonnet-4-6".into(),
        state.pool.clone(),
        session_id,
        ContextScope::Workspace,
        "make me a fibonacci+RSI strategy".into(),
    )
    .await
    .unwrap();

    let events = drain(&mut wl).await;
    let tool_calls = tool_call_names(&events);
    let narrative = all_tokens(&events);
    let requests = requests.lock().unwrap();
    assert!(
        requests.len() >= 3,
        "named-pattern flow must dispatch after both empty folder tool results"
    );
    let final_request = request_text(&requests[2]);
    assert_contains(
        &final_request,
        "make me a fibonacci+RSI strategy",
        "named-pattern final wizard request",
    );
    assert_contains(
        &final_request,
        r#""content":"[]""#,
        "named-pattern final wizard request",
    );
    assert_contains(
        &final_request,
        "Genuinely empty + named pattern",
        "wizard system prompt",
    );
    assert_contains(
        &final_request,
        "explicitly offer `xvn strategies init`",
        "wizard system prompt",
    );
    assert_contains(&final_request, "Do NOT jump straight", "wizard system prompt");

    // Must NOT have called create_strategy before offering prepop.
    assert!(
        !tool_calls.contains(&"create_strategy".to_string()),
        "wizard must not jump to create_strategy when folder is empty and \
         operator named a pattern; tool_calls: {tool_calls:?}"
    );

    // The narrative must mention the empty-folder state.
    let mentions_empty = narrative.to_ascii_lowercase().contains("empty")
        || narrative.to_ascii_lowercase().contains("no files")
        || narrative.to_ascii_lowercase().contains("nothing");
    assert!(
        mentions_empty,
        "wizard must acknowledge the empty folder; got: {narrative:?}"
    );

    // The narrative must offer prepop (init).
    let offers_init = narrative.contains("init")
        || narrative.to_ascii_lowercase().contains("seed")
        || narrative.to_ascii_lowercase().contains("prepop");
    assert!(
        offers_init,
        "wizard must offer prepop (xvn strategies init) for empty folder + named pattern; \
         got: {narrative:?}"
    );
}

// ── test 3: empty folder + general request ─────────────────────────────────

/// Documented behavior choice (empty + general request):
/// The wizard offers BOTH options in the same turn — `xvn strategies init`
/// (prepop) AND starting a blank draft — so the operator chooses. The
/// wizard must NOT silently skip the prepop offer.
///
/// This test accepts either of two valid shapes:
///   (a) wizard offers prepop without calling create_strategy — operator
///       consent needed first.
///   (b) wizard offers prepop AND calls create_strategy in the same turn
///       (i.e. offers both in parallel). Both are acceptable as long as
///       the narrative mentions "init" / "seed".
///
/// The test FAILS if create_strategy is called AND the narrative has no
/// mention of init/seed (i.e. the prepop offer was silently skipped).
#[tokio::test]
async fn empty_folder_general_request_offers_prepop() {
    let (state, tmp) = boot().await;
    // No files under strategies/ — folder missing → list returns [].

    // Script: (1) list_strategies_folder (returns []), (2) list_strategy_ideas
    // (returns []), (3) narrative offering both options.
    let (mock, requests) = recording_sequence(vec![
        MockDispatch::tool_use("tu_1", "list_strategies_folder", serde_json::json!({})),
        MockDispatch::tool_use("tu_2", "list_strategy_ideas", serde_json::json!({})),
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: "Your strategies folder is empty. I can either run \
                       `xvn strategies init` to seed it with curated examples, \
                       or start a blank strategy draft right now. Which would you prefer?"
                    .into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 10,
            output_tokens: 30,
        },
    ]);

    let session_id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let mut wl = WizardLoop::new(
        tmp.path().to_path_buf(),
        mock,
        "claude-sonnet-4-6".into(),
        state.pool.clone(),
        session_id,
        ContextScope::Workspace,
        "make me a strategy".into(),
    )
    .await
    .unwrap();

    let events = drain(&mut wl).await;
    let tool_calls = tool_call_names(&events);
    let narrative = all_tokens(&events);
    let requests = requests.lock().unwrap();
    assert!(
        requests.len() >= 3,
        "general empty-folder flow must dispatch after both empty folder tool results"
    );
    let final_request = request_text(&requests[2]);
    assert_contains(
        &final_request,
        "make me a strategy",
        "general empty-folder final wizard request",
    );
    assert_contains(
        &final_request,
        r#""content":"[]""#,
        "general empty-folder final wizard request",
    );
    assert_contains(
        &final_request,
        "Genuinely empty + general request",
        "wizard system prompt",
    );
    assert_contains(
        &final_request,
        "offer `xvn strategies init` as an option",
        "wizard system prompt",
    );

    // The prepop offer must be visible in the emitted narrative. The
    // list_strategy_ideas tool call alone is only state gathering; it is not
    // an operator-facing offer.
    let offers_init = narrative.contains("init")
        || narrative.to_ascii_lowercase().contains("seed")
        || narrative.to_ascii_lowercase().contains("prepop");

    // If create_strategy was called the narrative must still mention init/seed.
    if tool_calls.contains(&"create_strategy".to_string()) {
        assert!(
            offers_init,
            "wizard called create_strategy without mentioning prepop option; \
             narrative: {narrative:?}, tool_calls: {tool_calls:?}"
        );
    } else {
        assert!(
            offers_init,
            "wizard must offer prepop (init/seed) for empty folder + general request; \
             narrative: {narrative:?}, tool_calls: {tool_calls:?}"
        );
    }
}
