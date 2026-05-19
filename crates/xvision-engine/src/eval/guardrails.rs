//! Server-side trade guardrails (F-7).
//!
//! Audit `team/intake/2026-05-19-eval-traces-end-to-end-audit.md` found
//! eval runs where the trader LLM emitted 26 consecutive `long_open`
//! decisions on the same asset (run `01KRZ18JTMZ1S7W1MBKC1PNNSJ`) and a
//! sibling run with 22 consecutive `short_open` plus 12 same-bar long↔short
//! flips (`01KRZKG8A1FHTBE88NPWTVQVYS`). The detection side was already
//! covered by `eval::behavior::derive_behavior_summary` (`direct_flips`,
//! etc.) — this module is the *apply-time* enforcement.
//!
//! ## Vocabulary
//!
//! * **Original action** — what the trader LLM emitted. Always preserved
//!   verbatim in `eval_decisions.action` for audit / replay.
//! * **Applied action** — what the executor actually hands to the broker /
//!   position update. When the guardrail rewrites, this differs from the
//!   original; the `supervisor_notes` row carries both.
//!
//! ## Rules (pure)
//!
//! | Pre-state           | Original     | Guardrail              | Applied | Reason                  |
//! |---------------------|--------------|------------------------|---------|-------------------------|
//! | long open on asset  | `long_open`  | `RewriteTo(Hold)`      | `hold`  | `pyramid blocked`       |
//! | short open on asset | `short_open` | `RewriteTo(Hold)`      | `hold`  | `pyramid blocked`       |
//! | last open was long  | `short_open` | `RewriteTo(Flat)`      | `flat`  | `one-step flip blocked` |
//! | last open was short | `long_open`  | `RewriteTo(Flat)`      | `flat`  | `one-step flip blocked` |
//! | anything else       | (any)        | `Allow`                | =orig.  |                         |
//!
//! Pyramid: there's an open position in the same direction. The trader
//! already has exposure; "open another" would scale up — the v1 sizing
//! path doesn't track multi-leg averaged entries and the live failure
//! mode in run `01KRWZHHSXAWHRZSG1X65CZMCD` was 29 cycles of
//! `long_open` getting `insufficient_funds` rejections. Block at the apply
//! seam and feed the trader a `hold` so equity bookkeeping stays clean.
//!
//! One-step flip: the trader has a long open and emits a `short_open`
//! immediately (or vice versa). The executor closes the prior side
//! (`flat`); a follow-up cycle can re-open the other side from a real
//! flat baseline. This forces a two-bar flip and rules out same-bar
//! whip-saws that the audit caught in `01KRZKG8A1FHTBE88NPWTVQVYS`.
//!
//! ## Why no `RejectWithNote`
//!
//! The contract allowed a third variant that keeps the original action
//! in `eval_decisions` but applies `flat`, with a free-form note. The
//! two-rule design already does this: `RewriteTo(Flat)` is rejection
//! plus a one-line reason. Keeping it to `Allow` / `RewriteTo` keeps the
//! integration call site exhaustive without an open-ended note channel.

/// Trader actions the trader-output schema recognises. We model them as
/// an enum so the rewrite cannot smuggle in a typo — every call site is
/// exhaustive, including future `*_close` variants if those land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    LongOpen,
    ShortOpen,
    Flat,
    Hold,
    /// Anything we don't recognise — passed through `Allow` without
    /// interpretation. The audit only flagged the four canonical actions
    /// above; unknowns stay out of the rewrite path so this module
    /// doesn't fight a schema drift in another track.
    Other,
}

impl Action {
    /// Parse the on-the-wire action string (lowercase, matches the
    /// trader-output schema). Unknown actions become `Other` so the
    /// guardrail is a no-op for them — schema validation happens
    /// upstream in `TraderOutput::parse_response`.
    pub fn parse(s: &str) -> Self {
        match s {
            "long_open" => Action::LongOpen,
            "short_open" => Action::ShortOpen,
            "flat" => Action::Flat,
            "hold" => Action::Hold,
            _ => Action::Other,
        }
    }

    /// Wire-format string. Matches what `DecisionRow.action` carries on
    /// the apply path so a rewritten action serialises identically to a
    /// real `hold` / `flat`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::LongOpen => "long_open",
            Action::ShortOpen => "short_open",
            Action::Flat => "flat",
            Action::Hold => "hold",
            Action::Other => "other",
        }
    }
}

/// Position direction the guardrail sees for the asset under decision.
/// Computed from the executor's current position state (broker.position
/// for paper, the in-memory `position` for backtest). `LastOpen` carries
/// the most recent open *direction* even when position is currently flat
/// — needed to detect a same-bar flip that closes-then-opposite-opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionState {
    /// No live position AND no recent open direction recorded (or the
    /// most recent open was followed by a flat).
    Flat,
    /// Live long position on the asset.
    Long,
    /// Live short position on the asset.
    Short,
}

/// Outcome of the guardrail decision. `Allow` keeps the trader's action;
/// `RewriteTo(action)` swaps the applied action while preserving the
/// original in `eval_decisions.action` for audit.
///
/// The variant is consumed at the apply seam in
/// `paper::run_inner` / `backtest::run_inner`; supervisor-note formatting
/// uses `GuardrailOutcome::supervisor_note_content` so the wire shape is
/// pinned by a single function across both executors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardrailDecision {
    Allow,
    RewriteTo {
        action: Action,
        reason: GuardrailReason,
    },
}

/// Reason tag for the supervisor note. Tags are stable strings the
/// review / dashboard can group on. Free-form notes proved unnecessary
/// once `RejectWithNote` was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardrailReason {
    PyramidBlocked,
    OneStepFlipBlocked,
}

impl GuardrailReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            GuardrailReason::PyramidBlocked => "pyramid blocked",
            GuardrailReason::OneStepFlipBlocked => "one-step flip blocked",
        }
    }
}

/// Pure guardrail decision function. Inputs are the original action,
/// the current position state on the asset, and the most-recent open
/// direction (for flip detection from a momentarily-flat position
/// between same-bar close + opposite-open — the executor's `position`
/// snapshot may not catch that within a single decision tick).
///
/// In the v1 single-asset executors, `position_state` carries enough
/// information for both rules: if the executor has a `Long` position
/// open and the trader emits `short_open`, that's a flip; if it has
/// `Flat` AND no prior-open hint, anything is allowed.
///
/// `last_open_direction` is `Some(Action::LongOpen | Action::ShortOpen)`
/// when the most recent emitted action on this asset was an open and
/// no closing `flat` has been recorded since. The executors maintain
/// this state across decision iterations.
pub fn classify(
    action: Action,
    position_state: PositionState,
    last_open_direction: Option<Action>,
) -> GuardrailDecision {
    // Pyramid block: same-side open while a same-direction position is live.
    match (action, position_state) {
        (Action::LongOpen, PositionState::Long) => {
            return GuardrailDecision::RewriteTo {
                action: Action::Hold,
                reason: GuardrailReason::PyramidBlocked,
            };
        }
        (Action::ShortOpen, PositionState::Short) => {
            return GuardrailDecision::RewriteTo {
                action: Action::Hold,
                reason: GuardrailReason::PyramidBlocked,
            };
        }
        _ => {}
    }

    // One-step flip block: opposite-direction open while a live position
    // is in the other direction.
    match (action, position_state) {
        (Action::ShortOpen, PositionState::Long) | (Action::LongOpen, PositionState::Short) => {
            return GuardrailDecision::RewriteTo {
                action: Action::Flat,
                reason: GuardrailReason::OneStepFlipBlocked,
            };
        }
        _ => {}
    }

    // Same-bar flip detection from a flat position: the last *recorded*
    // open was opposite the new one. This only fires when the executor
    // has lost the live position to a same-bar close (rare in v1 but
    // covered for safety) — when the executor sees a live position the
    // first match above already wins.
    if matches!(position_state, PositionState::Flat) {
        match (action, last_open_direction) {
            (Action::ShortOpen, Some(Action::LongOpen))
            | (Action::LongOpen, Some(Action::ShortOpen)) => {
                return GuardrailDecision::RewriteTo {
                    action: Action::Flat,
                    reason: GuardrailReason::OneStepFlipBlocked,
                };
            }
            _ => {}
        }
    }

    GuardrailDecision::Allow
}

/// Format the `supervisor_notes.content` row for a non-`Allow` decision.
/// Format pinned by the contract:
///
/// ```text
/// <reason>: original=<action> applied=<action> asset=<asset> decision_index=<i>
/// ```
///
/// The format is stable enough that downstream review tools (the
/// dashboard supervisor-notes panel, when it lands) can split on
/// `=` per field.
pub fn supervisor_note_content(
    reason: GuardrailReason,
    original: Action,
    applied: Action,
    asset: &str,
    decision_index: u32,
) -> String {
    format!(
        "{reason}: original={orig} applied={app} asset={asset} decision_index={idx}",
        reason = reason.as_str(),
        orig = original.as_str(),
        app = applied.as_str(),
        asset = asset,
        idx = decision_index,
    )
}

/// Helper for the executors: take a position size (signed; +long /
/// -short / 0 flat) and reduce it to the `PositionState` the
/// guardrail expects. Centralised so paper and backtest see identical
/// thresholds on the `0` boundary.
pub fn position_state_from_size(size: f64) -> PositionState {
    if size > 0.0 {
        PositionState::Long
    } else if size < 0.0 {
        PositionState::Short
    } else {
        PositionState::Flat
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_open_into_existing_long_rewrites_to_hold() {
        let out = classify(Action::LongOpen, PositionState::Long, Some(Action::LongOpen));
        assert_eq!(
            out,
            GuardrailDecision::RewriteTo {
                action: Action::Hold,
                reason: GuardrailReason::PyramidBlocked,
            },
        );
    }

    #[test]
    fn short_open_into_existing_short_rewrites_to_hold() {
        let out = classify(
            Action::ShortOpen,
            PositionState::Short,
            Some(Action::ShortOpen),
        );
        assert_eq!(
            out,
            GuardrailDecision::RewriteTo {
                action: Action::Hold,
                reason: GuardrailReason::PyramidBlocked,
            },
        );
    }

    #[test]
    fn short_open_after_long_open_rewrites_to_flat() {
        // Live long position, trader emits short_open — one-step flip.
        let out = classify(Action::ShortOpen, PositionState::Long, Some(Action::LongOpen));
        assert_eq!(
            out,
            GuardrailDecision::RewriteTo {
                action: Action::Flat,
                reason: GuardrailReason::OneStepFlipBlocked,
            },
        );
    }

    #[test]
    fn long_open_after_short_open_rewrites_to_flat() {
        let out = classify(
            Action::LongOpen,
            PositionState::Short,
            Some(Action::ShortOpen),
        );
        assert_eq!(
            out,
            GuardrailDecision::RewriteTo {
                action: Action::Flat,
                reason: GuardrailReason::OneStepFlipBlocked,
            },
        );
    }

    #[test]
    fn long_open_from_flat_with_no_history_is_allowed() {
        let out = classify(Action::LongOpen, PositionState::Flat, None);
        assert_eq!(out, GuardrailDecision::Allow);
    }

    #[test]
    fn flat_when_long_is_allowed() {
        // The trader's own close decision must pass through.
        let out = classify(Action::Flat, PositionState::Long, Some(Action::LongOpen));
        assert_eq!(out, GuardrailDecision::Allow);
    }

    #[test]
    fn hold_is_always_allowed() {
        for ps in [PositionState::Flat, PositionState::Long, PositionState::Short] {
            assert_eq!(classify(Action::Hold, ps, None), GuardrailDecision::Allow);
            assert_eq!(
                classify(Action::Hold, ps, Some(Action::LongOpen)),
                GuardrailDecision::Allow,
            );
        }
    }

    #[test]
    fn long_open_after_flat_after_long_open_is_allowed() {
        // long_open → flat → long_open is a re-entry after close, not
        // a pyramid. Behaviour-side detection (`direct_flips`,
        // `reentries_after_loss`) classifies this; the guardrail does
        // not block.
        let out = classify(Action::LongOpen, PositionState::Flat, None);
        assert_eq!(out, GuardrailDecision::Allow);
    }

    #[test]
    fn same_bar_flip_from_flat_with_last_open_blocks() {
        // Executor lost the live position (same-bar close) but we still
        // know the prior open was long; emitting short_open should be
        // blocked as a flip.
        let out = classify(
            Action::ShortOpen,
            PositionState::Flat,
            Some(Action::LongOpen),
        );
        assert_eq!(
            out,
            GuardrailDecision::RewriteTo {
                action: Action::Flat,
                reason: GuardrailReason::OneStepFlipBlocked,
            },
        );
    }

    #[test]
    fn unknown_action_passes_through() {
        let out = classify(Action::Other, PositionState::Long, Some(Action::LongOpen));
        assert_eq!(out, GuardrailDecision::Allow);
    }

    #[test]
    fn position_state_from_size_boundaries() {
        assert_eq!(position_state_from_size(0.0), PositionState::Flat);
        assert_eq!(position_state_from_size(0.0001), PositionState::Long);
        assert_eq!(position_state_from_size(-0.0001), PositionState::Short);
    }

    #[test]
    fn supervisor_note_content_format_is_stable() {
        let s = supervisor_note_content(
            GuardrailReason::PyramidBlocked,
            Action::LongOpen,
            Action::Hold,
            "BTC/USD",
            7,
        );
        assert_eq!(
            s,
            "pyramid blocked: original=long_open applied=hold asset=BTC/USD decision_index=7"
        );
    }

    #[test]
    fn supervisor_note_content_flip_format() {
        let s = supervisor_note_content(
            GuardrailReason::OneStepFlipBlocked,
            Action::ShortOpen,
            Action::Flat,
            "BTC/USD",
            3,
        );
        assert_eq!(
            s,
            "one-step flip blocked: original=short_open applied=flat asset=BTC/USD decision_index=3"
        );
    }

    #[test]
    fn action_round_trip() {
        for raw in ["long_open", "short_open", "flat", "hold"] {
            let parsed = Action::parse(raw);
            assert_eq!(parsed.as_str(), raw);
        }
        assert!(matches!(Action::parse("weird"), Action::Other));
    }
}
