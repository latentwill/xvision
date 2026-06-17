//! Typed rich display blocks for chat rail messages.
//!
//! These blocks are built by server-side tools from trusted domain data. The
//! model should ask for actions; it should not hand-author chart JSON.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::api::eval::{RunDetail, RunSummary};
use crate::api::strategy::StrategySummary;
use crate::eval::compare::ComparisonReport;

const MAX_INLINE_CHART_POINTS: usize = 500;
const KNOWN_COMMANDS: &[&str] = &[
    "open_command_palette",
    "start_eval",
    "create_strategy",
    "compare_runs",
    "open_settings",
];

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RichContentBlock {
    InlineChart(InlineChartPayload),
    RunList(ChatRunListPayload),
    StrategyCard(ChatStrategyPayload),
    ActionCard(ChatActionPayload),
    ChoiceChips { chips: Vec<InlineAction> },
}

impl RichContentBlock {
    pub fn validate(&self) -> Result<(), RichBlockError> {
        match self {
            RichContentBlock::InlineChart(payload) => payload.validate(),
            RichContentBlock::RunList(payload) => payload.validate(),
            RichContentBlock::StrategyCard(payload) => payload.validate(),
            RichContentBlock::ActionCard(payload) => payload.validate(),
            RichContentBlock::ChoiceChips { chips } => validate_actions(chips),
        }
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InlineChartPayload {
    pub chart_id: String,
    pub kind: InlineChartKind,
    pub title: String,
    pub subtitle: Option<String>,
    pub primary_metric: Option<InlineMetric>,
    pub metrics: Vec<InlineMetric>,
    pub series: Vec<InlineChartSeries>,
    pub source: Option<InlineChartSource>,
    pub actions: Vec<InlineAction>,
    pub a11y_summary: String,
    #[serde(default)]
    pub downsampled: bool,
}

impl InlineChartPayload {
    pub fn validate(&self) -> Result<(), RichBlockError> {
        require_nonempty("chart_id", &self.chart_id)?;
        require_nonempty("title", &self.title)?;
        require_nonempty("a11y_summary", &self.a11y_summary)?;
        if self.series.is_empty() {
            return Err(RichBlockError::MissingField("series"));
        }

        let mut total_points = 0usize;
        for series in &self.series {
            series.validate()?;
            total_points += series.points.len();
        }
        if total_points > MAX_INLINE_CHART_POINTS {
            return Err(RichBlockError::TooManyPoints {
                max: MAX_INLINE_CHART_POINTS,
                actual: total_points,
            });
        }

        validate_actions(&self.actions)?;
        Ok(())
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InlineChartKind {
    Equity,
    Compare,
    Histogram,
    Drawdown,
    Sparkline,
    TradeMarkers,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InlineChartSeries {
    pub id: String,
    pub label: String,
    pub tone: Option<InlineTone>,
    pub points: Vec<InlinePoint>,
}

impl InlineChartSeries {
    fn validate(&self) -> Result<(), RichBlockError> {
        require_nonempty("series.id", &self.id)?;
        require_nonempty("series.label", &self.label)?;
        if self.points.is_empty() {
            return Err(RichBlockError::MissingField("series.points"));
        }
        if self.points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return Err(RichBlockError::NonFinitePoint);
        }
        Ok(())
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InlinePoint {
    pub x: f64,
    pub y: f64,
    pub label: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InlineMetric {
    pub label: String,
    pub value: String,
    pub unit: Option<String>,
    pub tone: Option<InlineTone>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InlineAction {
    pub label: String,
    pub href: Option<String>,
    pub command: Option<String>,
}

impl InlineAction {
    fn validate(&self) -> Result<(), RichBlockError> {
        require_nonempty("action.label", &self.label)?;
        match (&self.href, &self.command) {
            (Some(href), None) if is_spa_href(href) => Ok(()),
            (None, Some(command)) if KNOWN_COMMANDS.contains(&command.as_str()) => Ok(()),
            (Some(_), None) => Err(RichBlockError::InvalidActionTarget),
            (None, Some(_)) => Err(RichBlockError::UnknownCommand),
            _ => Err(RichBlockError::InvalidActionTarget),
        }
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InlineChartSource {
    pub label: String,
    pub href: Option<String>,
    pub run_id: Option<String>,
    pub strategy_id: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InlineTone {
    Default,
    Gold,
    Info,
    Warn,
    Danger,
    Muted,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRunListPayload {
    pub title: String,
    pub runs: Vec<ChatRunListItem>,
    pub actions: Vec<InlineAction>,
}

impl ChatRunListPayload {
    pub fn validate(&self) -> Result<(), RichBlockError> {
        require_nonempty("title", &self.title)?;
        if self.runs.is_empty() {
            return Err(RichBlockError::MissingField("runs"));
        }
        for run in &self.runs {
            require_nonempty("run.run_id", &run.run_id)?;
        }
        validate_actions(&self.actions)
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRunListItem {
    pub rank: u32,
    pub run_id: String,
    pub strategy_id: Option<String>,
    pub scenario: Option<String>,
    pub return_pct: Option<f64>,
    pub sharpe: Option<f64>,
    pub sparkline: Option<Vec<InlinePoint>>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatStrategyPayload {
    pub strategy_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub status: Option<String>,
    pub metrics: Vec<InlineMetric>,
    pub tags: Vec<String>,
    pub actions: Vec<InlineAction>,
}

impl ChatStrategyPayload {
    pub fn validate(&self) -> Result<(), RichBlockError> {
        require_nonempty("strategy_id", &self.strategy_id)?;
        require_nonempty("title", &self.title)?;
        validate_actions(&self.actions)
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatActionPayload {
    pub action_id: String,
    pub title: String,
    pub body: String,
    pub confirm: InlineAction,
    pub cancel: Option<InlineAction>,
}

impl ChatActionPayload {
    pub fn validate(&self) -> Result<(), RichBlockError> {
        require_nonempty("action_id", &self.action_id)?;
        require_nonempty("title", &self.title)?;
        require_nonempty("body", &self.body)?;
        self.confirm.validate()?;
        if let Some(cancel) = &self.cancel {
            cancel.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RichBlockError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("inline chart has too many points: max {max}, actual {actual}")]
    TooManyPoints { max: usize, actual: usize },
    #[error("inline chart contains a non-finite point")]
    NonFinitePoint,
    #[error("invalid action target")]
    InvalidActionTarget,
    #[error("unknown action command")]
    UnknownCommand,
}

pub fn build_inline_chart(mut payload: InlineChartPayload) -> Result<InlineChartPayload, RichBlockError> {
    normalize_chart_values(&mut payload);
    let total_points: usize = payload.series.iter().map(|s| s.points.len()).sum();
    if total_points > MAX_INLINE_CHART_POINTS {
        let series_count = payload.series.len().max(1);
        let max_per_series = (MAX_INLINE_CHART_POINTS / series_count).max(1);
        for series in &mut payload.series {
            series.points = downsample_points(&series.points, max_per_series);
        }
        payload.downsampled = true;
    }
    payload.validate()?;
    Ok(payload)
}

pub fn inline_equity_chart_from_run_detail(detail: &RunDetail) -> Result<RichContentBlock, RichBlockError> {
    let run = &detail.summary;
    let payload = InlineChartPayload {
        chart_id: format!("run:{}:equity", run.id),
        kind: InlineChartKind::Equity,
        title: "Equity curve".into(),
        subtitle: Some(format!("{} / {}", run.agent_id, run.scenario_id)),
        primary_metric: run.total_return_pct.map(|value| InlineMetric {
            label: "Return".into(),
            value: signed_number(value),
            unit: Some("%".into()),
            tone: tone_for_signed(value),
        }),
        metrics: run_metrics(run),
        series: vec![InlineChartSeries {
            id: "equity".into(),
            label: "Equity".into(),
            tone: Some(InlineTone::Gold),
            points: detail
                .equity_curve
                .iter()
                .map(|point| InlinePoint {
                    x: point.timestamp.timestamp_millis() as f64,
                    y: point.equity_usd,
                    label: None,
                })
                .collect(),
        }],
        source: Some(InlineChartSource {
            label: format!("Run {}", run.id),
            href: Some(format!("/eval-runs/{}", run.id)),
            run_id: Some(run.id.clone()),
            strategy_id: Some(run.agent_id.clone()),
        }),
        actions: vec![InlineAction {
            label: "Open run".into(),
            href: Some(format!("/eval-runs/{}", run.id)),
            command: None,
        }],
        a11y_summary: format!(
            "Equity chart for run {} with {} samples.",
            run.id,
            detail.equity_curve.len()
        ),
        downsampled: false,
    };
    build_inline_chart(payload).map(RichContentBlock::InlineChart)
}

pub fn inline_compare_chart_from_report(
    report: &ComparisonReport,
) -> Result<RichContentBlock, RichBlockError> {
    let series = report
        .equity_curves
        .iter()
        .take(4)
        .map(|curve| InlineChartSeries {
            id: curve.run_id.clone(),
            label: curve.run_id.clone(),
            tone: None,
            points: curve
                .samples
                .iter()
                .map(|sample| InlinePoint {
                    x: sample.timestamp.timestamp_millis() as f64,
                    y: sample.equity_usd,
                    label: None,
                })
                .collect(),
        })
        .collect::<Vec<_>>();

    let payload = InlineChartPayload {
        chart_id: format!(
            "compare:{}",
            report
                .runs
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ),
        kind: InlineChartKind::Compare,
        title: "Run comparison".into(),
        subtitle: Some(format!("{} runs", report.runs.len())),
        primary_metric: None,
        metrics: vec![InlineMetric {
            label: "Findings".into(),
            value: report.findings.len().to_string(),
            unit: None,
            tone: Some(InlineTone::Info),
        }],
        series,
        source: None,
        actions: vec![InlineAction {
            label: "Open compare".into(),
            href: Some(format!(
                "/eval-runs/compare?ids={}",
                report
                    .runs
                    .iter()
                    .map(|r| r.id.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            )),
            command: None,
        }],
        a11y_summary: format!("Comparison chart for {} runs.", report.runs.len()),
        downsampled: false,
    };
    build_inline_chart(payload).map(RichContentBlock::InlineChart)
}

pub fn inline_returns_histogram_from_runs(runs: &[RunSummary]) -> Result<RichContentBlock, RichBlockError> {
    let points = runs
        .iter()
        .enumerate()
        .filter_map(|(index, run)| {
            run.total_return_pct.map(|value| InlinePoint {
                x: index as f64,
                y: value,
                label: Some(run.id.clone()),
            })
        })
        .collect::<Vec<_>>();
    let payload = InlineChartPayload {
        chart_id: "runs:return-histogram".into(),
        kind: InlineChartKind::Histogram,
        title: "Return distribution".into(),
        subtitle: Some(format!("{} runs", runs.len())),
        primary_metric: None,
        metrics: vec![],
        series: vec![InlineChartSeries {
            id: "returns".into(),
            label: "Return %".into(),
            tone: Some(InlineTone::Gold),
            points,
        }],
        source: None,
        actions: vec![InlineAction {
            label: "Open runs".into(),
            href: Some("/eval-runs".into()),
            command: None,
        }],
        a11y_summary: format!("Histogram of returns for {} runs.", runs.len()),
        downsampled: false,
    };
    build_inline_chart(payload).map(RichContentBlock::InlineChart)
}

pub fn run_list_card_from_summaries(runs: &[RunSummary]) -> Result<RichContentBlock, RichBlockError> {
    let payload = ChatRunListPayload {
        title: "Eval runs".into(),
        runs: runs
            .iter()
            .take(5)
            .enumerate()
            .map(|(index, run)| ChatRunListItem {
                rank: index as u32 + 1,
                run_id: run.id.clone(),
                strategy_id: Some(run.agent_id.clone()),
                scenario: Some(run.scenario_id.clone()),
                return_pct: run.total_return_pct,
                sharpe: run.sharpe,
                sparkline: None,
            })
            .collect(),
        actions: vec![InlineAction {
            label: "Open runs".into(),
            href: Some("/eval-runs".into()),
            command: None,
        }],
    };
    payload.validate()?;
    Ok(RichContentBlock::RunList(payload))
}

pub fn inline_strategy_card_from_summary(
    summary: &StrategySummary,
) -> Result<RichContentBlock, RichBlockError> {
    let payload = ChatStrategyPayload {
        strategy_id: summary.agent_id.clone(),
        title: summary.display_name.clone(),
        subtitle: Some(summary.template.clone()),
        status: Some("validated".into()),
        metrics: vec![InlineMetric {
            label: "Cadence".into(),
            value: summary.decision_cadence_minutes.to_string(),
            unit: Some("m".into()),
            tone: Some(InlineTone::Info),
        }],
        tags: summary.tags.clone(),
        actions: vec![InlineAction {
            label: "Open strategy".into(),
            href: Some(format!("/strategies/{}", summary.agent_id)),
            command: None,
        }],
    };
    payload.validate()?;
    Ok(RichContentBlock::StrategyCard(payload))
}

pub fn action_confirmation_card(
    action_id: impl Into<String>,
    title: impl Into<String>,
    body: impl Into<String>,
    confirm: InlineAction,
) -> Result<RichContentBlock, RichBlockError> {
    let payload = ChatActionPayload {
        action_id: action_id.into(),
        title: title.into(),
        body: body.into(),
        confirm,
        cancel: None,
    };
    payload.validate()?;
    Ok(RichContentBlock::ActionCard(payload))
}

fn run_metrics(run: &RunSummary) -> Vec<InlineMetric> {
    let mut metrics = Vec::new();
    if let Some(value) = run.sharpe {
        metrics.push(InlineMetric {
            label: "Sharpe".into(),
            value: format!("{value:.2}"),
            unit: None,
            tone: Some(InlineTone::Info),
        });
    }
    if let Some(value) = run.max_drawdown_pct {
        metrics.push(InlineMetric {
            label: "Max DD".into(),
            value: format!("{value:.1}"),
            unit: Some("%".into()),
            tone: Some(InlineTone::Danger),
        });
    }
    if let Some(value) = run.total_return_pct {
        metrics.push(InlineMetric {
            label: "Return".into(),
            value: signed_number(value),
            unit: Some("%".into()),
            tone: tone_for_signed(value),
        });
    }
    metrics
}

fn signed_number(value: f64) -> String {
    if value > 0.0 {
        format!("+{value:.1}")
    } else {
        format!("{value:.1}")
    }
}

fn tone_for_signed(value: f64) -> Option<InlineTone> {
    if value < 0.0 {
        Some(InlineTone::Danger)
    } else {
        Some(InlineTone::Gold)
    }
}

fn normalize_chart_values(payload: &mut InlineChartPayload) {
    for series in &mut payload.series {
        for point in &mut series.points {
            point.x = round4(point.x);
            point.y = round4(point.y);
        }
    }
}

fn downsample_points(points: &[InlinePoint], max_points: usize) -> Vec<InlinePoint> {
    if points.len() <= max_points {
        return points.to_vec();
    }
    if max_points <= 1 {
        return points.first().cloned().into_iter().collect();
    }

    let last = points.len() - 1;
    (0..max_points)
        .map(|i| {
            let idx = ((i * last) + ((max_points - 1) / 2)) / (max_points - 1);
            points[idx].clone()
        })
        .collect()
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn require_nonempty(field: &'static str, value: &str) -> Result<(), RichBlockError> {
    if value.trim().is_empty() {
        Err(RichBlockError::MissingField(field))
    } else {
        Ok(())
    }
}

fn validate_actions(actions: &[InlineAction]) -> Result<(), RichBlockError> {
    for action in actions {
        action.validate()?;
    }
    Ok(())
}

fn is_spa_href(href: &str) -> bool {
    href.starts_with('/')
        && !href.starts_with("//")
        && !href.chars().any(char::is_whitespace)
        && !href.contains("://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    use crate::api::eval::{EquityPoint, RunDetail};
    use crate::api::strategy::StrategySummary;
    use crate::eval::compare::{
        ComparisonEquityCurve, ComparisonEquitySample, ComparisonReport, ComparisonRunSummary,
    };
    use crate::eval::run::{MetricsSummary, RunMode, RunStatus};

    #[test]
    fn valid_equity_chart_payload_passes() {
        let payload = equity_payload(32);

        let built = build_inline_chart(payload).expect("valid chart");

        assert!(!built.downsampled);
        assert_eq!(built.series[0].points[1].y, 1001.1235);
    }

    #[test]
    fn missing_a11y_summary_fails() {
        let mut payload = equity_payload(8);
        payload.a11y_summary = " ".into();

        let err = payload.validate().expect_err("missing summary should fail");

        assert_eq!(err, RichBlockError::MissingField("a11y_summary"));
    }

    #[test]
    fn too_many_points_fails_without_builder_downsampling() {
        let payload = equity_payload(501);

        let err = payload.validate().expect_err("too many points should fail");

        assert_eq!(
            err,
            RichBlockError::TooManyPoints {
                max: MAX_INLINE_CHART_POINTS,
                actual: 501,
            }
        );
    }

    #[test]
    fn builder_downsamples_large_payload() {
        let payload = equity_payload(900);

        let built = build_inline_chart(payload).expect("builder downsamples");

        assert!(built.downsampled);
        assert_eq!(built.series[0].points.len(), MAX_INLINE_CHART_POINTS);
    }

    #[test]
    fn invalid_action_target_fails() {
        let mut payload = equity_payload(8);
        payload.actions = vec![InlineAction {
            label: "External".into(),
            href: Some("https://example.com".into()),
            command: None,
        }];

        let err = payload.validate().expect_err("external href should fail");

        assert_eq!(err, RichBlockError::InvalidActionTarget);
    }

    #[test]
    fn run_detail_builder_produces_equity_chart() {
        let detail = RunDetail {
            summary: run_summary("run-a", 12.0),
            decisions: vec![],
            equity_curve: vec![
                EquityPoint {
                    timestamp: Utc::now(),
                    equity_usd: 1000.0,
                },
                EquityPoint {
                    timestamp: Utc::now(),
                    equity_usd: 1120.0,
                },
            ],
            filter_events: vec![],
            filter_summaries: vec![],
            signals_used: None,
        };

        let block = inline_equity_chart_from_run_detail(&detail).expect("equity card");

        match block {
            RichContentBlock::InlineChart(payload) => {
                assert_eq!(payload.kind, InlineChartKind::Equity);
                assert_eq!(payload.series[0].points.len(), 2);
            }
            _ => panic!("expected inline chart"),
        }
    }

    #[test]
    fn compare_builder_produces_two_series() {
        let now = Utc::now();
        let report = ComparisonReport {
            runs: vec![comparison_run("run-a", 10.0), comparison_run("run-b", -3.0)],
            equity_curves: vec![
                ComparisonEquityCurve {
                    run_id: "run-a".into(),
                    samples: vec![ComparisonEquitySample {
                        timestamp: now,
                        equity_usd: 1100.0,
                    }],
                },
                ComparisonEquityCurve {
                    run_id: "run-b".into(),
                    samples: vec![ComparisonEquitySample {
                        timestamp: now,
                        equity_usd: 970.0,
                    }],
                },
            ],
            findings: vec![],
        };

        let block = inline_compare_chart_from_report(&report).expect("compare card");

        match block {
            RichContentBlock::InlineChart(payload) => {
                assert_eq!(payload.kind, InlineChartKind::Compare);
                assert_eq!(payload.series.len(), 2);
            }
            _ => panic!("expected inline chart"),
        }
    }

    #[test]
    fn run_summaries_build_histogram_and_run_list() {
        let runs = vec![run_summary("run-a", 7.5), run_summary("run-b", -1.2)];

        let histogram = inline_returns_histogram_from_runs(&runs).expect("histogram");
        let list = run_list_card_from_summaries(&runs).expect("run list");

        assert!(matches!(histogram, RichContentBlock::InlineChart(_)));
        assert!(matches!(list, RichContentBlock::RunList(_)));
    }

    #[test]
    fn action_confirmation_builder_validates_card() {
        let block = action_confirmation_card(
            "started-run",
            "Run started",
            "The eval run has been queued.",
            InlineAction {
                label: "Open run".into(),
                href: Some("/eval-runs/run-a".into()),
                command: None,
            },
        )
        .expect("action card");

        assert!(matches!(block, RichContentBlock::ActionCard(_)));
    }

    #[test]
    fn strategy_summary_builder_creates_card() {
        let summary = StrategySummary {
            agent_id: "agent-a".into(),
            display_name: "Agent A".into(),
            template: "mean-reversion".into(),
            creator: "@tester".into(),
            decision_cadence_minutes: 60,
            tags: vec!["btc".into()],
            color: Some("#D4A547".into()),
            model: Some("claude".into()),
            providers: vec!["anthropic".into()],
            models: vec!["claude".into()],
            provider_models: vec![],
            capabilities: vec!["trader".into()],
            agent_count: 1,
            filter_count: 0,
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            asset_universe: vec!["BTC/USD".into()],
            execution_mode: "per_asset".into(),
            bundle_hash: None,
            origin: crate::api::strategy::StrategyOrigin::User,
            evaluated: false,
            last_eval_completed_at: None,
        };

        let block = inline_strategy_card_from_summary(&summary).expect("strategy card");

        assert!(matches!(block, RichContentBlock::StrategyCard(_)));
    }

    fn equity_payload(points: usize) -> InlineChartPayload {
        InlineChartPayload {
            chart_id: "equity-main".into(),
            kind: InlineChartKind::Equity,
            title: "Equity curve".into(),
            subtitle: Some("AAPL 1h".into()),
            primary_metric: Some(InlineMetric {
                label: "Return".into(),
                value: "+12.4".into(),
                unit: Some("%".into()),
                tone: Some(InlineTone::Gold),
            }),
            metrics: vec![],
            series: vec![InlineChartSeries {
                id: "equity".into(),
                label: "Equity".into(),
                tone: Some(InlineTone::Gold),
                points: (0..points)
                    .map(|i| InlinePoint {
                        x: i as f64,
                        y: 1000.0 + (i as f64 * 1.12345),
                        label: None,
                    })
                    .collect(),
            }],
            source: Some(InlineChartSource {
                label: "Run abc123".into(),
                href: Some("/eval-runs/abc123".into()),
                run_id: Some("abc123".into()),
                strategy_id: None,
            }),
            actions: vec![InlineAction {
                label: "Open run".into(),
                href: Some("/eval-runs/abc123".into()),
                command: None,
            }],
            a11y_summary: "Equity rises from 1000 to 1035.95.".into(),
            downsampled: false,
        }
    }

    fn run_summary(id: &str, total_return_pct: f64) -> RunSummary {
        RunSummary {
            id: id.into(),
            agent_id: "agent-a".into(),
            scenario_id: "scenario-a".into(),
            strategy: None,
            scenario: None,
            mode: "backtest".into(),
            status: "completed".into(),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            sharpe: Some(1.25),
            max_drawdown_pct: Some(4.2),
            total_return_pct: Some(total_return_pct),
            error: None,
            actual_input_tokens: Some(10),
            actual_output_tokens: Some(20),
            inference_cost_quote_total: None,
            net_return_pct: None,
            filter_summaries: vec![],
            auto_fire_review: false,
            review_model: None,
            max_annotations_per_review: Some(8),
            paused: false,
            paused_at: None,
            flatten_requested: false,
            live_config: None,
            source: Default::default(),
            unrealized_pnl_usd: None,
        }
    }

    fn comparison_run(id: &str, total_return_pct: f64) -> ComparisonRunSummary {
        ComparisonRunSummary {
            id: id.into(),
            agent_id: "agent-a".into(),
            strategy_name: Some("Agent A".into()),
            scenario_id: "scenario-a".into(),
            mode: RunMode::Backtest,
            status: RunStatus::Completed,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            metrics: Some(MetricsSummary {
                total_return_pct,
                sharpe: 1.0,
                max_drawdown_pct: 2.0,
                win_rate: 0.5,
                n_trades: 2,
                n_decisions: 4,
                baselines: None,
                ..Default::default()
            }),
            error: None,
            behavior: None,
            bars_content_hash: None,
            manifest_canonical: None,
            net_return_pct: None,
            input_tokens: None,
            output_tokens: None,
            cost_usd_estimate: None,
            cost_estimate_complete: true,
            wall_clock_ms: None,
        }
    }
}
