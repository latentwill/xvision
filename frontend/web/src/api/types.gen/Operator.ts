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
  | "between";
