#!/usr/bin/env bash
# PostToolUse hook: format + clippy the touched crate after Write|Edit on a Rust file.
# Reads the hook input JSON from stdin, no-ops on non-Rust files or non-crates paths.
# Intentionally swallows errors so a transient cargo failure never blocks Claude.

set -u

input="$(cat)"
file="$(printf '%s' "$input" | jq -r '.tool_input.file_path // .tool_response.filePath // empty')"

case "$file" in
  */crates/*/src/*.rs|*/crates/*/tests/*.rs|*/crates/*/benches/*.rs|*/crates/*/examples/*.rs) ;;
  *) exit 0 ;;
esac

# Extract crate name from .../crates/<crate>/...
crate="$(printf '%s' "$file" | sed -n 's#.*/crates/\([^/]*\)/.*#\1#p')"
[ -n "$crate" ] || exit 0

repo_root="/Users/edkennedy/Code/xvision"
cd "$repo_root" 2>/dev/null || exit 0

export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null 2>&1 || exit 0

# fmt is fast and idempotent. Run synchronously.
cargo fmt -p "$crate" >/dev/null 2>&1 || true

# clippy is heavier; fire-and-forget in the background so the hook returns fast.
(
  cargo clippy -p "$crate" --no-deps -q --message-format short \
    >>"$repo_root/.claude/hooks/.clippy-tail.log" 2>&1 || true
) &

exit 0
