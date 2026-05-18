// Environment-driven mode flags. Kept tiny and host-repo agnostic.

const TRUE = new Set(["1", "true", "yes", "on"]);
const FALSE = new Set(["0", "false", "no", "off"]);

function flag(name: string, defaultValue: boolean): boolean {
  const v = process.env[name];
  if (v === undefined) return defaultValue;
  const lc = v.trim().toLowerCase();
  if (TRUE.has(lc)) return true;
  if (FALSE.has(lc)) return false;
  return defaultValue;
}

// Shadow mode — print-only; no GraphQL mutations, no git worktree calls,
// no claude spawns. Default false.
export function isShadow(): boolean {
  return flag("AGENT_CONDUCTOR_SHADOW", false);
}

// Kill switch — daemon refuses to start when false. Default true (the
// launchd plist sets this explicitly per the contract).
export function isEnabled(): boolean {
  return flag("AGENT_CONDUCTOR_ENABLE", true);
}

// Poll interval override. Returns null when unset or invalid.
export function pollIntervalOverrideS(): number | null {
  const v = process.env["AGENT_CONDUCTOR_POLL_S"];
  if (v === undefined) return null;
  const n = Number(v);
  if (!Number.isFinite(n) || n < 1) return null;
  return n;
}
