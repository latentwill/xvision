// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
// Mirrors `xvision_filters::IndicatorName`. v1 catalog is closed at 6.

export type IndicatorName =
  | "ema"
  | "sma"
  | "rsi"
  | "atr"
  | "atr_pct"
  | "close";
