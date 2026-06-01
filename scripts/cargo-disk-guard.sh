#!/usr/bin/env bash
#
# Cargo disk guard — keep the shared cargo target dir from filling the disk.
#
# This workspace shares ONE cargo target dir across many agents/worktrees, and
# cargo never garbage-collects old artifacts, so target/debug/deps grows
# unbounded until the volume is full (and unrelated builds start failing with
# "No space left on device"). This guard checks free space and, when low, frees
# it in escalating tiers — cheapest/safest first.
#
# Modes:
#   (default)   check; if free < threshold, tiered clean; report.
#   --warn      check; if low, print ONE warning line; never clean; exit 0.
#   --check     report; exit 0 if ok, 3 if below threshold (no clean).
#
# Tunables (env):
#   XVISION_DISK_MIN_GB   free-GB threshold (default 12)
#   CARGO_TARGET_DIR      explicit target dir (else auto-detected)
set -euo pipefail

MIN_GB="${XVISION_DISK_MIN_GB:-12}"
MODE="${1:-clean}"

# Resolve the target dir cheaply (no `cargo metadata` — keep the --warn path fast
# enough for a pre-commit hook): explicit env → this repo's known external volume
# → <repo>/target.
target_dir="${CARGO_TARGET_DIR:-}"
if [ -z "$target_dir" ] && [ -d /Volumes/XvisionCargoTarget/target ]; then
  target_dir="/Volumes/XvisionCargoTarget/target"
fi
if [ -z "$target_dir" ]; then
  target_dir="$(git rev-parse --show-toplevel 2>/dev/null || echo .)/target"
fi

# df the dir (or its nearest existing ancestor). macOS `df -g` => 1 GiB blocks.
probe="$target_dir"
while [ ! -d "$probe" ] && [ "$probe" != "/" ]; do probe="$(dirname "$probe")"; done
free_gb() { df -g "$probe" 2>/dev/null | awk 'NR==2 {print $4+0}'; }

have="$(free_gb)"
if [ -z "$have" ]; then
  echo "cargo-disk-guard: could not read free space for $probe" >&2
  exit 0   # never block a build because the guard itself failed
fi

if [ "$have" -ge "$MIN_GB" ]; then
  [ "$MODE" = "--check" ] && echo "cargo-disk-guard: ${have}G free (>= ${MIN_GB}G) — ok"
  exit 0
fi

case "$MODE" in
  --warn)
    echo "⚠ cargo-disk-guard: only ${have}G free on ${probe} (< ${MIN_GB}G)." \
         "Free space with: scripts/cargo-disk-guard.sh" >&2
    exit 0 ;;
  --check)
    echo "cargo-disk-guard: only ${have}G free (< ${MIN_GB}G)" >&2
    exit 3 ;;
esac

echo "cargo-disk-guard: ${have}G free < ${MIN_GB}G — cleaning ${target_dir}" >&2

# Tier 1 — incremental caches. Cheap, always safe, often the biggest quick win.
rm -rf "$target_dir"/debug/incremental "$target_dir"/release/incremental 2>/dev/null || true
have="$(free_gb)"; echo "  tier1 (incremental): ${have}G free" >&2
[ "$have" -ge "$MIN_GB" ] && exit 0

# Tier 2 — cargo-sweep: drop artifacts not touched in 7 days. Auto-install once.
if ! command -v cargo-sweep >/dev/null 2>&1; then
  echo "  installing cargo-sweep (one-time)…" >&2
  cargo install cargo-sweep --quiet 2>/dev/null || echo "  cargo-sweep unavailable; skipping tier2" >&2
fi
if command -v cargo-sweep >/dev/null 2>&1; then
  repo="$(git rev-parse --show-toplevel 2>/dev/null || echo .)"
  cargo sweep --time 7 "$repo" >/dev/null 2>&1 || cargo sweep --time 7 >/dev/null 2>&1 || true
  have="$(free_gb)"; echo "  tier2 (cargo-sweep >7d): ${have}G free" >&2
  [ "$have" -ge "$MIN_GB" ] && exit 0
fi

# Tier 3 — last resort: drop compiled deps/build outputs (forces a from-scratch
# rebuild next time). Skipped artifacts rebuild; nothing here is source.
echo "  tier3 (last resort): dropping deps/build outputs — next build is full" >&2
rm -rf "$target_dir"/debug/deps  "$target_dir"/debug/build \
       "$target_dir"/release/deps "$target_dir"/release/build 2>/dev/null || true
have="$(free_gb)"; echo "  tier3 done: ${have}G free" >&2
exit 0
