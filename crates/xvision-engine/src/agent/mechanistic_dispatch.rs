//! Deterministic dispatch for strategies with `decision_mode = "mechanistic"`.
//!
//! The dispatch keeps the same `LlmDispatch` boundary as provider-backed
//! traders, but does not call a provider. It consumes the JSON trade context
//! from the last user turn and emits the canonical trader-output object.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
use serde_json::Value;

use crate::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, StopReason};
use crate::strategies::{
    select_entry_rule, trend_from_filter_context, ClosePolicy, EntryDirection, MechanisticConfig,
};

const EPSILON: f64 = f64::EPSILON;

/// Provider-free dispatch used by a mechanistic strategy.
///
/// A dispatch handle is scoped to one evaluator invocation. The state map is
/// still keyed by asset because a single handle may be reused by a portfolio
/// executor and because the context is the source of truth for the asset.
pub struct MechanisticDispatch {
    config: MechanisticConfig,
    state: Mutex<PositionState>,
}

impl MechanisticDispatch {
    pub fn new(config: MechanisticConfig) -> Self {
        Self {
            config,
            state: Mutex::new(PositionState::default()),
        }
    }

    #[cfg(test)]
    fn state_len(&self) -> usize {
        self.state.lock().expect("mechanistic state lock").positions.len()
    }
}

#[derive(Debug, Default)]
struct PositionState {
    positions: HashMap<String, TrackedPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Long,
    Short,
}

#[derive(Debug, Clone, Copy)]
struct TrackedPosition {
    side: Side,
    entry_price: f64,
    peak_price: f64,
    last_price: f64,
    bars_held: u32,
    size: f64,
}

#[derive(Debug, Default)]
struct TradeContext {
    asset: Option<String>,
    price: Option<f64>,
    position_size: Option<f64>,
    position_side: Option<Side>,
    position_known: bool,
    entry_price: Option<f64>,
    bars_held: Option<u32>,
    equity: Option<f64>,
    timestamp: Option<DateTime<Utc>>,
    trend_long: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseTrigger {
    StopLoss,
    TakeProfit,
    TrailingStop,
    TimeExit,
    TargetPnl,
}

impl CloseTrigger {
    fn action_name(self) -> &'static str {
        // `TraderOutput` intentionally only permits `flat` for an exit. Keep
        // the policy name in the justification so operators can distinguish
        // which close policy won the ordered race.
        "flat"
    }

    fn label(self) -> &'static str {
        match self {
            Self::StopLoss => "stop_loss",
            Self::TakeProfit => "take_profit",
            Self::TrailingStop => "trailing_stop",
            Self::TimeExit => "time_exit",
            Self::TargetPnl => "target_pnl",
        }
    }
}

#[async_trait]
impl LlmDispatch for MechanisticDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let text = last_user_text(&req)
            .ok_or_else(|| anyhow!("mechanistic dispatch requires a user trade context"))?;
        let context = parse_trade_context(&text)?;
        let asset = context
            .asset
            .as_deref()
            .filter(|asset| !asset.trim().is_empty())
            .ok_or_else(|| anyhow!("mechanistic trade context is missing asset"))?
            .trim()
            .to_string();

        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("mechanistic position state lock poisoned"))?;
        let mut tracked = state.positions.remove(&asset);

        // When the executor supplies position fields, synchronise our state
        // with them. Compact contexts may omit those fields; in that case the
        // state created by our previous response is authoritative.
        if context.position_known {
            if context
                .position_size
                .map(|size| size.abs() <= EPSILON)
                .unwrap_or(matches!(context.position_side, Some(Side::Long) | Some(Side::Short)) == false)
            {
                tracked = None;
            } else {
                let side = context
                    .position_side
                    .or_else(|| context.position_size.and_then(side_from_size))
                    .or_else(|| tracked.map(|position| position.side));
                let side =
                    side.ok_or_else(|| anyhow!("mechanistic position context is missing direction"))?;
                let entry_price = context
                    .entry_price
                    .filter(|price| price.is_finite() && *price > 0.0)
                    .or_else(|| tracked.map(|position| position.entry_price))
                    .or(context.price)
                    .ok_or_else(|| anyhow!("mechanistic position context is missing entry price"))?;
                let size = context
                    .position_size
                    .map(f64::abs)
                    .filter(|size| *size > EPSILON)
                    .or_else(|| tracked.map(|position| position.size))
                    .unwrap_or(1.0);
                let same_position = tracked
                    .map(|position| {
                        position.side == side && (position.entry_price - entry_price).abs() <= EPSILON
                    })
                    .unwrap_or(false);
                tracked = Some(TrackedPosition {
                    side,
                    entry_price,
                    peak_price: if same_position {
                        tracked.map(|position| position.peak_price).unwrap_or(entry_price)
                    } else {
                        entry_price
                    },
                    bars_held: context
                        .bars_held
                        .or_else(|| tracked.map(|position| position.bars_held))
                        .unwrap_or(0),
                    last_price: context
                        .price
                        .or_else(|| tracked.map(|position| position.last_price))
                        .unwrap_or(entry_price),
                    size,
                });
            }
        }

        let price = context
            .price
            .filter(|price| price.is_finite() && *price > 0.0)
            .or_else(|| tracked.map(|position| position.last_price));

        let Some(mut position) = tracked else {
            let Some(price) = price else {
                return Ok(response("hold", 0.5, "mechanistic: missing current price"));
            };
            if !entry_session_open(self.config.entry_session_utc.as_deref(), context.timestamp) {
                return Ok(response("hold", 0.5, "mechanistic: outside entry session"));
            }
            let Some(entry_rule) = select_entry_rule(&self.config, context.trend_long) else {
                return Ok(response("hold", 0.5, "mechanistic: no entry rule"));
            };
            let side = match entry_rule.direction {
                EntryDirection::Long => Side::Long,
                EntryDirection::Short => Side::Short,
            };
            tracing::debug!(
                target = "xvision.mechanistic",
                timestamp = ?context.timestamp,
                price,
                trend_long = ?context.trend_long,
                direction = ?entry_rule.direction,
                side = ?side,
                "mechanistic dispatch entry"
            );
            state.positions.insert(
                asset,
                TrackedPosition {
                    side,
                    entry_price: price,
                    peak_price: price,
                    last_price: price,
                    bars_held: 0,
                    size: 1.0,
                },
            );
            let action = match side {
                Side::Long => "long_open",
                Side::Short => "short_open",
            };
            return Ok(response(action, 1.0, "mechanistic entry"));
        };

        let Some(price) = price else {
            state.positions.insert(asset, position);
            return Ok(response("hold", 0.5, "mechanistic: missing current price"));
        };

        if !context.position_known {
            position.bars_held = position.bars_held.saturating_add(1);
        }
        position.peak_price = match position.side {
            Side::Long => position.peak_price.max(price),
            Side::Short => position.peak_price.min(price),
        };
        position.last_price = price;

        if let Some(trigger) =
            first_close_trigger(&self.config.close_policies, &position, price, context.equity)
        {
            return Ok(response(
                trigger.action_name(),
                1.0,
                &format!("mechanistic {}", trigger.label()),
            ));
        }

        state.positions.insert(asset, position);
        Ok(response("hold", 0.5, "mechanistic hold"))
    }
}

fn entry_session_open(spec: Option<&str>, timestamp: Option<DateTime<Utc>>) -> bool {
    let (Some(spec), Some(timestamp)) = (spec, timestamp) else {
        return true;
    };
    let mut parts = spec.split('-');
    let (Ok(start), Ok(end)) = (
        parts.next().unwrap_or_default().trim().parse::<u32>(),
        parts.next().unwrap_or_default().trim().parse::<u32>(),
    ) else {
        return true;
    };
    if parts.next().is_some() || start > 24 || end > 24 {
        return true;
    }
    if start == end {
        return true;
    }
    let hour = timestamp.hour();
    if start < end {
        hour >= start && hour < end
    } else {
        hour >= start || hour < end
    }
}

fn response(action: &str, conviction: f64, justification: &str) -> LlmResponse {
    let body = serde_json::json!({
        "action": action,
        "conviction": conviction,
        "justification": justification,
    });
    LlmResponse {
        content: vec![ContentBlock::Text {
            text: body.to_string(),
        }],
        stop_reason: StopReason::EndTurn,
        input_tokens: 0,
        output_tokens: 0,
    }
}

fn first_close_trigger(
    policies: &[ClosePolicy],
    position: &TrackedPosition,
    price: f64,
    equity: Option<f64>,
) -> Option<CloseTrigger> {
    for policy in policies {
        let trigger = match policy {
            ClosePolicy::StopLoss { pct } if stop_loss_hit(position, price, *pct) => CloseTrigger::StopLoss,
            ClosePolicy::TakeProfit { pct } if take_profit_hit(position, price, *pct) => {
                CloseTrigger::TakeProfit
            }
            ClosePolicy::TrailingStop { pct } if trailing_stop_hit(position, price, *pct) => {
                CloseTrigger::TrailingStop
            }
            ClosePolicy::TimeExit { bars } if position.bars_held >= *bars => CloseTrigger::TimeExit,
            ClosePolicy::TargetPnl { usd } if target_pnl_hit(position, price, *usd) => {
                CloseTrigger::TargetPnl
            }
            _ => continue,
        };
        // `equity` is parsed intentionally even though TargetPnl is based on
        // the position's realised price PnL. This keeps the context parser
        // compatible with compact renderers that expose equity alongside the
        // position fields and leaves room for notional-aware sizing.
        let _ = equity;
        return Some(trigger);
    }
    None
}

fn stop_loss_hit(position: &TrackedPosition, price: f64, pct: f64) -> bool {
    if pct <= 0.0 || !pct.is_finite() {
        return false;
    }
    let distance = pct / 100.0;
    match position.side {
        Side::Long => price <= position.entry_price * (1.0 - distance),
        Side::Short => price >= position.entry_price * (1.0 + distance),
    }
}

fn take_profit_hit(position: &TrackedPosition, price: f64, pct: f64) -> bool {
    if pct <= 0.0 || !pct.is_finite() {
        return false;
    }
    let distance = pct / 100.0;
    match position.side {
        Side::Long => price >= position.entry_price * (1.0 + distance),
        Side::Short => price <= position.entry_price * (1.0 - distance),
    }
}

fn trailing_stop_hit(position: &TrackedPosition, price: f64, pct: f64) -> bool {
    if pct <= 0.0 || !pct.is_finite() {
        return false;
    }
    let distance = pct / 100.0;
    match position.side {
        Side::Long => price <= position.peak_price * (1.0 - distance),
        Side::Short => price >= position.peak_price * (1.0 + distance),
    }
}

fn target_pnl_hit(position: &TrackedPosition, price: f64, usd: f64) -> bool {
    if !usd.is_finite() {
        return false;
    }
    let pnl = match position.side {
        Side::Long => (price - position.entry_price) * position.size,
        Side::Short => (position.entry_price - price) * position.size,
    };
    pnl >= usd
}

fn side_from_size(size: f64) -> Option<Side> {
    if size > EPSILON {
        Some(Side::Long)
    } else if size < -EPSILON {
        Some(Side::Short)
    } else {
        None
    }
}
fn last_user_text(req: &LlmRequest) -> Option<String> {
    req.messages.iter().rev().find_map(|message| {
        if !message.role.eq_ignore_ascii_case("user") {
            return None;
        }
        let text = message
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        (!text.trim().is_empty()).then_some(text)
    })
}

fn parse_trade_context(text: &str) -> anyhow::Result<TradeContext> {
    let value =
        embedded_json(text).ok_or_else(|| anyhow!("mechanistic user message has no JSON trade context"))?;
    let root = value.get("trade_context").unwrap_or(&value);
    let mut context = TradeContext::default();
    context.asset = first_string(root, &["asset", "symbol"])
        .or_else(|| object_string(root, "market_data", &["asset", "symbol"]));
    context.price = first_number(root, &["current_price", "price", "close", "mark_price"])
        .or_else(|| {
            object_number(
                root,
                "market_data",
                &["reference_price_usd", "current_price", "close", "mark_price"],
            )
        })
        .or_else(|| {
            object_number(
                object(root, "market_data").unwrap_or(&Value::Null),
                "current_bar",
                &["close", "price"],
            )
        })
        .or_else(|| object_number(root, "current_bar", &["close", "price"]));
    context.timestamp =
        first_string(root, &["timestamp", "ts"]).and_then(|value| value.parse::<DateTime<Utc>>().ok());
    context.trend_long = infer_trend(root, context.price);
    context.position_size = first_number(root, &["position_size", "position_qty", "quantity"])
        .or_else(|| {
            object_number(
                root,
                "portfolio_state",
                &["position_size", "position_qty", "quantity"],
            )
        })
        .or_else(|| {
            object_number(
                root,
                "position",
                &["position_size", "position_qty", "quantity", "size"],
            )
        });
    context.entry_price = first_number(root, &["entry_price"])
        .or_else(|| object_number(root, "portfolio_state", &["entry_price"]))
        .or_else(|| object_number(root, "position", &["entry_price"]));
    context.bars_held = first_u32(root, &["bars_since_entry", "bars_held"])
        .or_else(|| object_u32(root, "portfolio_state", &["bars_since_entry", "bars_held"]))
        .or_else(|| object_u32(root, "position", &["bars_since_entry", "bars_held"]));
    context.equity = first_number(root, &["equity", "nav"])
        .or_else(|| object_number(root, "portfolio_state", &["equity", "nav"]));
    context.position_side = first_side(
        root,
        &[
            "position_state",
            "position_direction",
            "direction",
            "position",
            "state",
        ],
    )
    .or_else(|| {
        object_side(
            root,
            "portfolio_state",
            &["position_state", "position_direction", "direction", "state"],
        )
    })
    .or_else(|| {
        object_side(
            root,
            "position",
            &["position_state", "position_direction", "direction", "state"],
        )
    })
    .or_else(|| context.position_size.and_then(side_from_size));
    context.position_known = context.position_size.is_some()
        || context.position_side.is_some()
        || has_position_fields(root, "portfolio_state")
        || has_position_fields(root, "position")
        || root
            .as_object()
            .map(|object| {
                object.contains_key("position_size")
                    || object.contains_key("position_state")
                    || object.contains_key("position")
                    || object.contains_key("state")
            })
            .unwrap_or(false);
    Ok(context)
}
fn infer_trend(root: &Value, current_price: Option<f64>) -> Option<bool> {
    if let Some(trend) =
        trend_from_filter_context(root.get("filter_context"), current_price.unwrap_or(f64::NAN))
    {
        return Some(trend);
    }
    let bars = root
        .get("market_data")
        .and_then(|value| value.get("bar_history"))
        .and_then(Value::as_array)?;
    let mut ema = None;
    for bar in bars {
        let close_value = bar.get("close").and_then(number)?;
        ema = Some(match ema {
            None => close_value,
            Some(previous) => previous + (2.0 / 22.0) * (close_value - previous),
        });
    }
    let ema = ema?;
    Some(current_price? > ema)
}

fn has_position_fields(root: &Value, key: &str) -> bool {
    root.get(key)
        .and_then(Value::as_object)
        .map(|object| {
            object.contains_key("position_size")
                || object.contains_key("position_state")
                || object.contains_key("position_direction")
                || object.contains_key("bars_since_entry")
                || object.contains_key("bars_held")
                || object.contains_key("entry_price")
                || object.contains_key("state")
        })
        .unwrap_or(false)
}

fn embedded_json(text: &str) -> Option<Value> {
    let bytes = text.as_bytes();
    for (start, byte) in bytes.iter().enumerate() {
        if *byte != b'{' {
            continue;
        }
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for end in start..bytes.len() {
            let byte = bytes[end];
            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
                continue;
            }
            match byte {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let Ok(value) = serde_json::from_slice::<Value>(&bytes[start..=end]) else {
                            break;
                        };
                        if looks_like_trade_context(&value) {
                            return Some(value);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn looks_like_trade_context(value: &Value) -> bool {
    value.get("asset").is_some()
        || value.get("market_data").is_some()
        || value.get("portfolio_state").is_some()
        || value.get("trade_context").is_some()
        || value.get("position_size").is_some()
}

fn object<'a>(root: &'a Value, key: &str) -> Option<&'a Value> {
    root.get(key)
}

fn first_string(root: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| root.get(*key).and_then(Value::as_str).map(str::to_string))
}

fn object_string(root: &Value, object_key: &str, keys: &[&str]) -> Option<String> {
    root.get(object_key).and_then(|value| first_string(value, keys))
}

fn first_number(root: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| number(root.get(*key)?))
}

fn object_number(root: &Value, object_key: &str, keys: &[&str]) -> Option<f64> {
    root.get(object_key).and_then(|value| first_number(value, keys))
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse::<f64>().ok())
}

fn first_u32(root: &Value, keys: &[&str]) -> Option<u32> {
    keys.iter().find_map(|key| u32_value(root.get(*key)?))
}

fn object_u32(root: &Value, object_key: &str, keys: &[&str]) -> Option<u32> {
    root.get(object_key).and_then(|value| first_u32(value, keys))
}

fn u32_value(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .or_else(|| value.as_str()?.parse::<u32>().ok())
}

fn first_side(root: &Value, keys: &[&str]) -> Option<Side> {
    keys.iter().find_map(|key| side(root.get(*key)?))
}

fn object_side(root: &Value, object_key: &str, keys: &[&str]) -> Option<Side> {
    root.get(object_key).and_then(|value| first_side(value, keys))
}

fn side(value: &Value) -> Option<Side> {
    match value.as_str()?.to_ascii_lowercase().as_str() {
        "long" | "long_open" | "buy" => Some(Side::Long),
        "short" | "short_open" | "sell" => Some(Side::Short),
        "flat" | "none" | "" => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::{Message, ResponseSchema};

    fn config(policy: ClosePolicy) -> MechanisticConfig {
        MechanisticConfig {
            entry_rules: vec![crate::strategies::EntryRule {
                signal_name: "gate".into(),
                direction: EntryDirection::Long,
            }],
            entry_session_utc: None,
            close_policies: vec![policy],
        }
    }

    fn request(asset: &str, price: f64, position: Option<(f64, f64, u32)>) -> LlmRequest {
        let (size, entry, bars) = position.unwrap_or((0.0, 0.0, 0));
        let text = serde_json::json!({
            "asset": asset,
            "market_data": {"reference_price_usd": price, "current_bar": {"close": price}},
            "portfolio_state": {"position_size": size, "entry_price": entry, "bars_held": bars},
        });
        LlmRequest {
            model: "mechanistic".into(),
            system_prompt: String::new(),
            messages: vec![Message::user_text(format!("Inputs:\n{text}\n\nFollow."))],
            max_tokens: None,
            tools: Vec::new(),
            temperature: None,
            response_schema: Some(ResponseSchema::trader_output()),
            cache_control: None,
            force_json: false,
        }
    }

    async fn action(dispatch: &MechanisticDispatch, req: LlmRequest) -> String {
        let response = dispatch.complete(req).await.unwrap();
        let ContentBlock::Text { text } = &response.content[0] else {
            panic!("text response")
        };
        serde_json::from_str::<Value>(text).unwrap()["action"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn flat_emits_configured_entry_direction() {
        let dispatch = MechanisticDispatch::new(MechanisticConfig {
            entry_rules: vec![crate::strategies::EntryRule {
                signal_name: "gate".into(),
                direction: EntryDirection::Short,
            }],
            entry_session_utc: None,
            close_policies: vec![],
        });
        assert_eq!(
            action(&dispatch, request("BTC/USD", 100.0, None)).await,
            "short_open"
        );
    }

    #[tokio::test]
    async fn stop_loss_closes_long() {
        let dispatch = MechanisticDispatch::new(config(ClosePolicy::StopLoss { pct: 2.0 }));
        assert_eq!(
            action(&dispatch, request("BTC/USD", 100.0, None)).await,
            "long_open"
        );
        assert_eq!(
            action(&dispatch, request("BTC/USD", 98.0, Some((1.0, 100.0, 1)))).await,
            "flat"
        );
    }

    #[tokio::test]
    async fn take_profit_closes_short() {
        let mut cfg = config(ClosePolicy::TakeProfit { pct: 5.0 });
        cfg.entry_rules[0].direction = EntryDirection::Short;
        let dispatch = MechanisticDispatch::new(cfg);
        assert_eq!(
            action(&dispatch, request("BTC/USD", 100.0, None)).await,
            "short_open"
        );
        assert_eq!(
            action(&dispatch, request("BTC/USD", 95.0, Some((-1.0, 100.0, 1)))).await,
            "flat"
        );
    }

    #[tokio::test]
    async fn trailing_stop_uses_favorable_peak() {
        let dispatch = MechanisticDispatch::new(config(ClosePolicy::TrailingStop { pct: 5.0 }));
        assert_eq!(
            action(&dispatch, request("BTC/USD", 100.0, None)).await,
            "long_open"
        );
        assert_eq!(
            action(&dispatch, request("BTC/USD", 120.0, Some((1.0, 100.0, 1)))).await,
            "hold"
        );
        assert_eq!(
            action(&dispatch, request("BTC/USD", 113.9, Some((1.0, 100.0, 2)))).await,
            "flat"
        );
    }

    #[tokio::test]
    async fn time_exit_uses_bars_held() {
        let dispatch = MechanisticDispatch::new(config(ClosePolicy::TimeExit { bars: 3 }));
        assert_eq!(
            action(&dispatch, request("BTC/USD", 100.0, None)).await,
            "long_open"
        );
        assert_eq!(
            action(&dispatch, request("BTC/USD", 100.0, Some((1.0, 100.0, 2)))).await,
            "hold"
        );
        assert_eq!(
            action(&dispatch, request("BTC/USD", 100.0, Some((1.0, 100.0, 3)))).await,
            "flat"
        );
    }
    #[tokio::test]
    async fn dual_entry_rules_follow_filter_trend() {
        let dispatch = MechanisticDispatch::new(MechanisticConfig {
            entry_rules: vec![
                crate::strategies::EntryRule {
                    signal_name: "long_gate".into(),
                    direction: EntryDirection::Long,
                },
                crate::strategies::EntryRule {
                    signal_name: "short_gate".into(),
                    direction: EntryDirection::Short,
                },
            ],
            entry_session_utc: None,
            close_policies: vec![],
        });
        let mut req = request("BTC/USD", 110.0, None);
        req.messages = vec![Message::user_text(
            serde_json::json!({
                "asset": "BTC/USD",
                "current_price": 110.0,
                "position_size": 0.0,
                "filter_context": {"close": 110.0, "ema_21": 100.0}
            })
            .to_string(),
        )];
        assert_eq!(action(&dispatch, req).await, "long_open");

        let mut req = request("ETH/USD", 90.0, None);
        req.messages = vec![Message::user_text(
            serde_json::json!({
                "asset": "ETH/USD",
                "current_price": 90.0,
                "position_size": 0.0,
                "filter_context": {"close": 90.0, "ema_21": 100.0}
            })
            .to_string(),
        )];
        assert_eq!(action(&dispatch, req).await, "short_open");
    }

    #[tokio::test]
    async fn entry_session_blocks_flat_entries_but_not_exits() {
        let mut cfg = config(ClosePolicy::TakeProfit { pct: 2.0 });
        cfg.entry_session_utc = Some("18-24".into());
        let dispatch = MechanisticDispatch::new(cfg);
        let mut req = request("BTC/USD", 100.0, None);
        req.messages = vec![Message::user_text(
            serde_json::json!({
                "asset": "BTC/USD",
                "timestamp": "2025-01-01T17:00:00Z",
                "current_price": 100.0,
                "position_size": 0.0
            })
            .to_string(),
        )];
        assert_eq!(action(&dispatch, req).await, "hold");

        let mut req = request("BTC/USD", 103.0, Some((1.0, 100.0, 1)));
        req.messages = vec![Message::user_text(
            serde_json::json!({
                "asset": "BTC/USD",
                "timestamp": "2025-01-01T17:00:00Z",
                "current_price": 103.0,
                "position_size": 1.0,
                "entry_price": 100.0,
                "bars_held": 1
            })
            .to_string(),
        )];
        assert_eq!(action(&dispatch, req).await, "flat");
    }

    #[tokio::test]
    async fn target_pnl_uses_position_size() {
        let dispatch = MechanisticDispatch::new(config(ClosePolicy::TargetPnl { usd: 10.0 }));
        assert_eq!(
            action(&dispatch, request("BTC/USD", 100.0, None)).await,
            "long_open"
        );
        assert_eq!(
            action(&dispatch, request("BTC/USD", 105.0, Some((2.0, 100.0, 1)))).await,
            "flat"
        );
    }

    #[tokio::test]
    async fn omitted_position_fields_use_internal_state() {
        let dispatch = MechanisticDispatch::new(config(ClosePolicy::TakeProfit { pct: 2.0 }));
        assert_eq!(
            action(&dispatch, request("BTC/USD", 100.0, None)).await,
            "long_open"
        );
        let text = serde_json::json!({"asset":"BTC/USD","close":103.0}).to_string();
        let mut req = request("BTC/USD", 103.0, None);
        req.messages = vec![Message::user_text(text)];
        assert_eq!(action(&dispatch, req).await, "flat");
        assert_eq!(dispatch.state_len(), 0);
    }
}
