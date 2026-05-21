// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
// Mirrors `xvision_filters::ActivationMode`. Drift surfaces at typecheck
// once the engine regenerates types.gen via `cargo test --features ts-export`.

export type ActivationMode = "every_bar" | "filter_gated" | "compiled_rules";
