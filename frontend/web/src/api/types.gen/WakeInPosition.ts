// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
// Mirrors `xvision_filters::WakeInPosition`.

export type WakeInPosition =
  | "always"
  | "on_invalidation_or_target_only"
  | "never";
