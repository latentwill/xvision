//! `xvn optimize *` standalone DSPy prompt-optimizer CLI tests — REMOVED.
//!
//! The standalone DSPy prompt-optimizer CLI verbs (`inspect --run`,
//! `explain-missing-data`, `memory-demos`, `memory-demos-gate`, `export-demos`,
//! `import-demos`, `accept-as-child-agent`, `revert-accepted`) were removed when
//! `xvn optimize` was folded into the AutoOptimizer strategy-mutation cycle. The
//! prompt-optimization (DSPy) flywheel now runs INSIDE the cycle automatically
//! (it emits `CycleProgressEvent::FlywheelCompiled`), so the redundant manual
//! CLI surface — and every test that drove it — was deleted.
//!
//! The surviving `xvn optimize` cycle verbs (run [--max-cycles/--ipc-socket],
//! ls, show, lineage, unlock) are covered by:
//!   - autooptimizer_cli.rs
//!   - autooptimizer_cli_inspect.rs
//!   - autooptimizer_e2e.rs
//!   - cli_surface.rs (clap surface snapshot)
//!
//! Removed verbs: run-cycle (alias), mutate-once, demo.
//!
//! This file is intentionally left with no tests.
