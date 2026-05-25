#!/usr/bin/env bash
set -euo pipefail

cargo test -p xvision-memory --test leakage_regression
cargo test -p xvision-engine --test agent_memory_dispatch \
  phase0_leakage_regression_harness_flt_prompt_is_deterministic
cargo test -p xvision-engine --test agent_memory_dispatch \
  recall_wraps_each_pattern_in_caselaw_framing
cargo test -p xvision-engine --test agent_memory_dispatch \
  recall_excludes_pattern_when_training_window_overlaps_scenario
