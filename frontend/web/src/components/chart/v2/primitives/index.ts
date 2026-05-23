// Canvas primitives — authored by a parallel agent.
// These exports will cause type-errors until those files exist; that is expected
// during parallel development.
export * from "./KlineCandlePane";
export * from "./UplotEquityPane";
export * from "./UplotDrawdownPane";
export * from "./UplotHistogramPane";
export * from "./UplotOscillatorPane";
export * from "./UplotLinePane";
export * from "./UplotCompareOverlayPane";

// Chrome primitives
export * from "./ChartFrame";
export * from "./LayerPanel";
export * from "./MarkerDock";
export * from "./Legend";
export * from "./ConnectionStatus";
export * from "./CacheStatusBadge";
export * from "./EmptyState";
export * from "./DataTable";
export * from "./PaneStack";
export * from "./SyncCursor";

// Track B dashboard primitives (chart-rework spec B1+).
export * from "./MultiStrategyEquityPane";
export * from "./KpiCard";
export * from "./Topbar";
export * from "./MonthlyReturnsHeatmap";
export * from "./DrawdownCard";
// B2 — Comparison AB additions.
export * from "./MiniSparkline";
export * from "./StrategyRosterPills";
export * from "./StrategyCardGrid";
export * from "./StrategyCard";
export * from "./LeadCardChrome";
