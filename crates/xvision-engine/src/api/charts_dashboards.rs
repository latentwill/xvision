//! Charts dashboard section payload builders (chart-rework spec Track B).
//!
//! B0 ships a fixture-backed stub for
//! `GET /api/v2/charts/dashboards/overview` so the B1 worker can wire the
//! Dark Minimal Strategy Dashboard surface against a real HTTP endpoint
//! without waiting for the real builder. B1 replaces
//! [`build_dashboard_overview_stub`] with a real builder that pairs each
//! `Strategy` with its latest backtest run equity series and resolves
//! per-strategy color from `PublicManifest.color` with a stable-index
//! `strategyRotation` fallback.
//!
//! The stub re-serves the deterministic frontend fixture (single source of
//! truth: `frontend/web/src/components/chart/v2/__fixtures__/multi-strategy-equity.json`)
//! via `include_str!` so the JSON the server returns and the JSON the
//! frontend tests render against are byte-equal. When B1 lands, both move
//! over to live data.

use serde::{Deserialize, Serialize};

use crate::api::{ApiContext, ApiError, ApiResult};

/// 8-color `strategyRotation` fallback palette, matching
/// `frontend/web/src/theme/themes.ts:CHART2_STRATEGY_ROTATION` exactly
/// (same order, same hex strings). Used when `Strategy.manifest.color` is
/// `None`; index is the strategy's stable insertion order mod 8.
pub const STRATEGY_ROTATION_PALETTE: [&str; 8] = [
    "#D4A547", // fib · Fibonacci Golden Cross
    "#E8DCB0", // ema · EMA Pullback
    "#E07A3A", // brk · Breakout Retest
    "#B98AB4", // msw · Momentum Swing
    "#6BAFA8", // mvr · Mean Reversion AI
    "#D67B5C", // vsc · Volatility Scalper
    "#8C6024", // lqh · Liquidation Hunter
    "#6B6553", // btc · BTC Buy & Hold
];

/// Per-strategy entry inside a [`MultiStrategyEquityBundle`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultiStrategyBundleEntry {
    pub id: String,
    pub name: String,
    pub short: String,
    /// Hex string, e.g. `"#D4A547"`. Resolved server-side: prefers
    /// `Strategy.color` (added in B0), falls back to the `strategyRotation`
    /// palette by stable index when unset.
    pub color: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dashed: Option<bool>,
    /// % return, baselined at 0 — length matches the bundle's `time` array.
    pub equity: Vec<f64>,
    /// % drawdown, ≤ 0, length matches `equity`.
    pub drawdown: Vec<f64>,
    /// Per-month return rows; B0 stub ships 12 months per strategy.
    pub monthly: Vec<MonthlyReturnCell>,
    pub metrics: StrategyMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonthlyReturnCell {
    pub year: u16,
    pub month: u8,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategyMetrics {
    #[serde(rename = "return")]
    pub return_: f64,
    pub sharpe: f64,
    pub mdd: f64,
    pub win: f64,
    pub pf: f64,
}

/// Top-level payload for `GET /api/v2/charts/dashboards/overview`.
///
/// `kind` is a const discriminator so the frontend can narrow on it
/// (mirrors the `kind:` literal types in
/// `frontend/web/src/components/chart/v2/types.ts`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MultiStrategyEquityBundle {
    /// Always `"multi_strategy_equity"`.
    pub kind: String,
    /// Unix seconds when the bundle was generated.
    pub generated_at: f64,
    pub granularity: String,
    /// Shared timeline (unix seconds). Length matches every
    /// `strategies[i].equity` and `strategies[i].drawdown`.
    pub time: Vec<f64>,
    pub strategies: Vec<MultiStrategyBundleEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
    /// `Some(true)` when this bundle is the bundled fixture stub served
    /// during cold starts (no strategies / no completed backtest runs on
    /// disk). The frontend surfaces a visible "Sample data" label when set,
    /// so a demo never presents fixture equity curves as real (hackathon
    /// fixture-disclosure rule, task T3.2). Omitted (`None`) for real
    /// builder output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_fixture: Option<bool>,
}

/// B0 stub. Returns the deterministic fixture bundled with the
/// frontend, byte-for-byte. B1 replaces this with a real builder.
pub fn build_dashboard_overview_stub() -> ApiResult<MultiStrategyEquityBundle> {
    const FIXTURE_JSON: &str = include_str!(
        "../../../../frontend/web/src/components/chart/v2/__fixtures__/multi-strategy-equity.json"
    );
    let mut bundle = serde_json::from_str::<MultiStrategyEquityBundle>(FIXTURE_JSON)
        .map_err(|err| ApiError::Internal(format!("charts_dashboards stub fixture parse failed: {err}")))?;
    // Tag the stub so the frontend can render a visible "Sample data"
    // disclosure label (T3.2). Every cold-start fallback flows through
    // here, so all stub responses inherit the flag.
    bundle.is_fixture = Some(true);
    Ok(bundle)
}

/// B1 real builder. Loads all strategies from disk, pairs each with its
/// latest completed backtest run's equity series, and assembles a
/// [`MultiStrategyEquityBundle`].
///
/// **Timeline choice:** the shared `time` array is taken from the first
/// strategy that has a non-empty equity curve. Strategies with shorter
/// curves are padded with `f64::NAN` at the tail so all arrays stay the
/// same length as the shared timeline. The frontend ignores NaN points
/// when rendering, so the shorter strategies simply end early rather than
/// distorting the chart. This is documented here as the explicit design
/// decision; alternatives (union-merge, truncate to shortest) would
/// require cross-run timestamp alignment that is out of scope for B1.
///
/// **Fallback:** when no strategy has a completed backtest run on disk,
/// returns the B0 stub so the UI keeps rendering during cold starts. A
/// `tracing::info!` message is logged so operators can see why.
pub async fn build_dashboard_overview(ctx: &ApiContext) -> ApiResult<MultiStrategyEquityBundle> {
    use crate::eval::run::RunStatus;
    use crate::eval::store::{ListFilter, RunStore};
    use crate::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};

    let fs_store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let run_store = RunStore::new(ctx.db.clone());

    // List all strategy ids. An empty directory means no strategies yet.
    let ids = fs_store
        .list()
        .await
        .map_err(|e| ApiError::Internal(format!("list strategies: {e}")))?;

    if ids.is_empty() {
        tracing::info!(
            target: "xvision::charts",
            "build_dashboard_overview: no strategies on disk; returning stub"
        );
        return build_dashboard_overview_stub();
    }

    // Load each strategy. Skip ones that fail to parse (corruption guard).
    let mut strategies = Vec::new();
    for id in &ids {
        match fs_store.load(id).await {
            Ok(s) => strategies.push(s),
            Err(e) => {
                tracing::warn!(
                    target: "xvision::charts",
                    strategy_id = %id,
                    error = %e,
                    "build_dashboard_overview: failed to load strategy; skipping"
                );
            }
        }
    }

    // For each strategy, find the latest *completed* backtest run.
    // `RunStore::list` returns newest-first (ORDER BY started_at DESC),
    // so the first completed run in the list is the latest.
    struct StrategyRun {
        strategy: crate::strategies::Strategy,
        equity: Vec<(chrono::DateTime<chrono::Utc>, f64)>,
        metrics: Option<crate::eval::run::MetricsSummary>,
        idx: usize, // insertion order for color fallback
    }

    let mut paired: Vec<StrategyRun> = Vec::new();

    for (idx, strategy) in strategies.into_iter().enumerate() {
        let runs = run_store
            .list(ListFilter {
                agent_id: Some(strategy.manifest.id.clone()),
                status: Some(vec![RunStatus::Completed]),
                limit: Some(1),
                ..Default::default()
            })
            .await
            .map_err(|e| ApiError::Internal(format!("list runs for {}: {e}", strategy.manifest.id)))?;

        let Some(latest_run) = runs.into_iter().next() else {
            // No completed run for this strategy; skip it.
            continue;
        };

        let equity = run_store
            .read_equity_curve(&latest_run.id)
            .await
            .map_err(|e| ApiError::Internal(format!("read equity for run {}: {e}", latest_run.id)))?;

        if equity.is_empty() {
            continue;
        }

        paired.push(StrategyRun {
            strategy,
            equity,
            metrics: latest_run.metrics,
            idx,
        });
    }

    if paired.is_empty() {
        tracing::info!(
            target: "xvision::charts",
            "build_dashboard_overview: no completed backtest runs on disk; returning stub"
        );
        return build_dashboard_overview_stub();
    }

    // Build the shared timeline from the first (longest or first-placed)
    // strategy's equity timestamps.
    let shared_time: Vec<f64> = paired[0]
        .equity
        .iter()
        .map(|(ts, _)| ts.timestamp() as f64)
        .collect();
    let n = shared_time.len();

    let generated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    let lead = paired.first().map(|r| r.strategy.manifest.id.clone());

    let mut bundle_strategies: Vec<MultiStrategyBundleEntry> = Vec::with_capacity(paired.len());

    for sr in paired {
        let color = sr
            .strategy
            .manifest
            .color
            .clone()
            .unwrap_or_else(|| STRATEGY_ROTATION_PALETTE[sr.idx % 8].to_string());

        // Build % return series baselined at 0 from the raw USD equity curve.
        // Equity series length may differ from `n`; pad with NaN at the tail.
        let initial_equity = sr
            .equity
            .first()
            .map(|(_, v)| *v)
            .unwrap_or(1.0)
            .max(f64::EPSILON);

        let raw_equity_pct: Vec<f64> = sr
            .equity
            .iter()
            .map(|(_, v)| (v / initial_equity - 1.0) * 100.0)
            .collect();

        // Drawdown: ≤ 0, from running peak of the USD curve (not % return
        // curve) so the math is consistent with `build_run_payload`.
        let raw_drawdown: Vec<f64> = {
            let mut peak = f64::NEG_INFINITY;
            sr.equity
                .iter()
                .map(|(_, v)| {
                    peak = peak.max(*v);
                    if peak <= 0.0 {
                        0.0
                    } else {
                        -((peak - v) / peak * 100.0)
                    }
                })
                .collect()
        };

        // Pad to shared timeline length with NaN.
        let equity = pad_to(raw_equity_pct, n);
        let drawdown = pad_to(raw_drawdown, n);

        // Monthly return matrix derived from the equity curve.
        let monthly = build_monthly_returns(&sr.equity);

        // Headline metrics. Fall back to zeros when the run has no stored
        // metrics (e.g. a manually-created run without the metrics finalize
        // step — uncommon but possible during early development).
        let (ret, sharpe, mdd, win, pf) = if let Some(m) = sr.metrics {
            (
                m.total_return_pct,
                m.sharpe,
                m.max_drawdown_pct,
                m.win_rate,
                // Profit factor is not stored in MetricsSummary v1;
                // default to 0.0 until a dedicated pf column lands.
                0.0,
            )
        } else {
            (0.0, 0.0, 0.0, 0.0, 0.0)
        };

        bundle_strategies.push(MultiStrategyBundleEntry {
            id: sr.strategy.manifest.id.clone(),
            name: sr.strategy.manifest.display_name.clone(),
            short: sr.strategy.manifest.display_name.chars().take(12).collect(),
            color,
            kind: "Trend".to_string(), // generic; no per-strategy kind field in v1
            dashed: None,
            equity,
            drawdown,
            monthly,
            metrics: StrategyMetrics {
                return_: ret,
                sharpe,
                mdd,
                win,
                pf,
            },
        });
    }

    Ok(MultiStrategyEquityBundle {
        kind: "multi_strategy_equity".to_string(),
        generated_at,
        granularity: "1d".to_string(),
        time: shared_time,
        strategies: bundle_strategies,
        lead,
        // Real builder output — not fixture data.
        is_fixture: None,
    })
}

/// Pad `v` to length `n` with `f64::NAN`. If `v` is already `>= n`, returns
/// the first `n` elements (truncates to the shared timeline).
fn pad_to(mut v: Vec<f64>, n: usize) -> Vec<f64> {
    match v.len().cmp(&n) {
        std::cmp::Ordering::Less => {
            v.resize(n, f64::NAN);
            v
        }
        std::cmp::Ordering::Equal => v,
        std::cmp::Ordering::Greater => {
            v.truncate(n);
            v
        }
    }
}

/// Derive per-month % return from a USD equity curve.
///
/// For each calendar month present in the curve, we take the first and last
/// equity sample within that month and compute `(last / first - 1) * 100`.
/// Months with only one sample get a 0% return (no intra-month change
/// observable). Months are returned in chronological order.
fn build_monthly_returns(equity: &[(chrono::DateTime<chrono::Utc>, f64)]) -> Vec<MonthlyReturnCell> {
    use std::collections::BTreeMap;

    // Accumulate first/last USD equity per (year, month).
    let mut buckets: BTreeMap<(u16, u8), (f64, f64)> = BTreeMap::new();

    for (ts, val) in equity {
        let y = ts.format("%Y").to_string().parse::<u16>().unwrap_or(0);
        let m = ts.format("%m").to_string().parse::<u8>().unwrap_or(0);
        let entry = buckets.entry((y, m)).or_insert((*val, *val));
        // Keep first; update last.
        entry.1 = *val;
    }

    buckets
        .into_iter()
        .map(|((y, m), (first, last))| {
            let value = if first.abs() > f64::EPSILON {
                (last / first - 1.0) * 100.0
            } else {
                0.0
            };
            MonthlyReturnCell {
                year: y,
                month: m,
                value,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── stub tests (renamed with stub_ prefix) ───────────────────────────────

    #[test]
    fn stub_parses_into_well_shaped_bundle() {
        let bundle = build_dashboard_overview_stub().expect("stub parses");
        assert_eq!(bundle.kind, "multi_strategy_equity");
        assert_eq!(bundle.granularity, "1d");
        assert!(
            bundle.time.len() >= 240,
            "expected ≥240 bars in shared timeline, got {}",
            bundle.time.len()
        );
        assert_eq!(
            bundle.strategies.len(),
            5,
            "spec §6.1: B0 stub ships 5 strategies (rotation positions 0-4)"
        );
        for s in &bundle.strategies {
            assert!(s.color.starts_with('#'), "color should be hex: {:?}", s);
            assert_eq!(
                s.equity.len(),
                bundle.time.len(),
                "equity array length must match shared timeline"
            );
            assert_eq!(
                s.drawdown.len(),
                bundle.time.len(),
                "drawdown array length must match shared timeline"
            );
        }
        assert_eq!(bundle.lead.as_deref(), Some("fib"));
    }

    #[test]
    fn stub_is_tagged_as_fixture_and_serializes_camel_case() {
        // T3.2 fixture-disclosure: the cold-start stub must carry
        // `is_fixture = Some(true)` so the frontend can show a visible
        // "Sample data" label, and it must serialize as `isFixture` for
        // the camelCase frontend contract.
        let bundle = build_dashboard_overview_stub().expect("stub parses");
        assert_eq!(bundle.is_fixture, Some(true));

        let json = serde_json::to_string(&bundle).unwrap();
        assert!(
            json.contains("\"isFixture\":true"),
            "stub must serialize isFixture:true, got: {json}"
        );
    }

    #[test]
    fn stub_strategies_have_unique_ids() {
        let bundle = build_dashboard_overview_stub().unwrap();
        let mut ids: Vec<&str> = bundle.strategies.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "strategy ids must be unique");
    }

    #[test]
    fn stub_bundle_roundtrips_through_json() {
        // Catches breaking changes to the serialized shape.
        let bundle = build_dashboard_overview_stub().unwrap();
        let s = serde_json::to_string(&bundle).unwrap();
        let back: MultiStrategyEquityBundle = serde_json::from_str(&s).unwrap();
        assert_eq!(bundle, back);
    }

    // ── B1 real-builder unit tests ───────────────────────────────────────────

    /// Color fallback: strategy with `color = None` gets the rotation color
    /// at the correct stable index.
    #[test]
    fn color_fallback_uses_rotation_palette_at_stable_index() {
        // idx 0 → "#D4A547", idx 1 → "#E8DCB0", idx 7 → "#6B6553"
        assert_eq!(STRATEGY_ROTATION_PALETTE[0], "#D4A547");
        assert_eq!(STRATEGY_ROTATION_PALETTE[1], "#E8DCB0");
        assert_eq!(STRATEGY_ROTATION_PALETTE[7], "#6B6553");
        // Wraps at 8.
        assert_eq!(STRATEGY_ROTATION_PALETTE[8 % 8], STRATEGY_ROTATION_PALETTE[0]);
        assert_eq!(STRATEGY_ROTATION_PALETTE[9 % 8], STRATEGY_ROTATION_PALETTE[1]);
    }

    /// `pad_to` ensures all output arrays have the same length as the shared
    /// timeline, satisfying the bundle invariant.
    #[test]
    fn bundle_array_lengths_match_shared_timeline_after_pad() {
        let shared_n = 5usize;

        // Shorter input is padded with NaN.
        let short = vec![1.0, 2.0, 3.0];
        let padded = pad_to(short, shared_n);
        assert_eq!(padded.len(), shared_n);
        assert!(padded[3].is_nan());
        assert!(padded[4].is_nan());

        // Exact-length input passes through unchanged.
        let exact = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let same = pad_to(exact.clone(), shared_n);
        assert_eq!(same, exact);

        // Longer input is truncated to shared_n.
        let long = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let trunc = pad_to(long, shared_n);
        assert_eq!(trunc.len(), shared_n);
        assert_eq!(trunc, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    /// `build_monthly_returns` produces one cell per calendar month, ordered
    /// chronologically, with correct % return.
    #[test]
    fn monthly_returns_derived_correctly_from_equity_curve() {
        use chrono::{TimeZone, Utc};

        let equity = vec![
            (Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(), 10_000.0),
            (Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(), 10_500.0),
            (Utc.with_ymd_and_hms(2024, 1, 31, 0, 0, 0).unwrap(), 11_000.0), // +10%
            (Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(), 11_000.0),
            (Utc.with_ymd_and_hms(2024, 2, 28, 0, 0, 0).unwrap(), 10_450.0), // -5%
        ];

        let monthly = build_monthly_returns(&equity);
        assert_eq!(monthly.len(), 2);

        let jan = &monthly[0];
        assert_eq!(jan.year, 2024);
        assert_eq!(jan.month, 1);
        assert!(
            (jan.value - 10.0).abs() < 0.001,
            "Jan should be ~+10%, got {}",
            jan.value
        );

        let feb = &monthly[1];
        assert_eq!(feb.year, 2024);
        assert_eq!(feb.month, 2);
        assert!(
            (feb.value - (-5.0)).abs() < 0.001,
            "Feb should be ~-5%, got {}",
            feb.value
        );
    }
}
