# Credits

## gambletan/cortex

- Upstream: https://github.com/gambletan/cortex
- License: MIT
- Version target: v1.3.0 or the pinned commit selected during cortex-core adoption
- Role: persistent memory engine reference and planned `cortex-core` backend for `xvision-memory`

xvision's current `xvision-memory` crate is a trading-safety adapter and in-tree implementation of the Observation/Pattern memory substrate. The Phase 1.5 adoption plan moves that adapter onto gambletan/cortex while preserving xvision's F+L+T leakage protections.
