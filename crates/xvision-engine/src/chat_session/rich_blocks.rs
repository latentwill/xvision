//! Typed rich display blocks for chat rail messages.
//!
//! These blocks are built by server-side tools from trusted domain data. The
//! model should ask for actions; it should not hand-author chart JSON.

use serde::{Deserialize, Serialize};
use thiserror::Error;

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

pub fn build_inline_chart(
    mut payload: InlineChartPayload,
) -> Result<InlineChartPayload, RichBlockError> {
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
}
