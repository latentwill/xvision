// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
// Mirrors `xvision_filters::Operator`. DSL form (`>`, `crosses_above`, …)
// is the wire form per the engine's serde renames.

export type Operator =
  | ">"
  | "<"
  | ">="
  | "<="
  | "=="
  | "crosses_above"
  | "crosses_below"
  | "between"
  | `above_for_${number}`
  | `below_for_${number}`
  | `crossed_above_${number}`
  | `crossed_below_${number}`
  | `slope_gt_${number}`
  | `slope_lt_${number}`
  | `zscore_gt_${number}`
  | `zscore_lt_${number}`
  | `within_pct_${number}`;
