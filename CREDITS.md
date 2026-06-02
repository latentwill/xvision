# Credits

## Anthropic / Claude

- Models: Claude Sonnet 4.x, Claude Opus 4.x
- Role: Primary implementation author and collaborative engineer. The majority of xvision's codebase — engine, CLI, dashboard, frontend, migrations, tests, deployment automation, and architecture — was written by Claude models working closely with the operator across hundreds of sessions.
- Note: All generated code is original work covered by the project's own license. Anthropic retains no IP claim on generated outputs.

## gambletan/cortex

- Upstream: https://github.com/gambletan/cortex
- License: MIT
- Version target: v1.3.0 or the pinned commit selected during cortex-core adoption
- Role: persistent memory engine reference and planned `cortex-core` backend for `xvision-memory`

xvision's current `xvision-memory` crate is a trading-safety adapter and in-tree implementation of the Observation/Pattern memory substrate. The Phase 1.5 adoption plan moves that adapter onto gambletan/cortex while preserving xvision's F+L+T leakage protections.
