// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
//
// Untagged-union wire shape per `xvision_filters::Operand`:
//   - Indicator → string DSL token (`"ema_20"`, `"close"`)
//   - Numeric   → number literal (`0.6`, `50.0`)
//   - Range     → two-element ascending array (`[50.0, 70.0]`), only valid
//                 with Operator::Between
//
// Authors writing TOML/JSON write the bare shape (no wrapper object), so the
// frontend reads it the same way.

export type Operand = string | number | [number, number];
