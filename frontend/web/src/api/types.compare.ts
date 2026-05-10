// Hand-written TS types matching `engine::eval::compare::ComparisonReport`'s
// serialized shape. We don't auto-generate these via ts-rs because the
// engine types reference `Finding`/`MetricsSummary`/`RunMode`/`RunStatus`
// which don't all carry `#[derive(TS)]` yet — the manual mirror keeps
// this PR scoped to the compare route alone. Update when the engine
// shape changes.

export type ComparisonRunSummary = {
  id: string;
  strategy_bundle_hash: string;
  scenario_id: string;
  mode: "backtest" | "paper";
  status: "queued" | "running" | "completed" | "failed" | "cancelled";
  started_at: string;
  completed_at: string | null;
  metrics: MetricsSummary | null;
  error: string | null;
};

export type MetricsSummary = {
  total_return_pct: number;
  sharpe: number;
  max_drawdown_pct: number;
  win_rate: number;
  n_trades: number;
  n_decisions: number;
};

export type ComparisonEquitySample = {
  timestamp: string;
  equity_usd: number;
};

export type ComparisonEquityCurve = {
  run_id: string;
  samples: ComparisonEquitySample[];
};

export type CompareFinding = {
  id: string;
  run_id: string;
  kind: string;
  severity: "info" | "warning" | "critical";
  summary: string;
  evidence: unknown;
  extracted_at: string;
  schema_version: string;
};

export type ComparisonReport = {
  runs: ComparisonRunSummary[];
  equity_curves: ComparisonEquityCurve[];
  findings: CompareFinding[];
};
