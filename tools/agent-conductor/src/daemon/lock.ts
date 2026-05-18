// PID-file lock. `~/.cache/agent-conductor/lock` holds `{pid, startedAt}`.
// Exits with the holding PID in the error message when another live daemon
// owns the lock; reclaims when the holding PID is dead.

import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

export interface LockInfo {
  pid: number;
  startedAt: string; // ISO 8601 UTC
}

export class LockHeldError extends Error {
  readonly holder: LockInfo;
  constructor(holder: LockInfo) {
    super(
      `agent-conductor is already running (pid ${holder.pid}, started ${holder.startedAt})`,
    );
    this.name = "LockHeldError";
    this.holder = holder;
  }
}

/// Probe whether `pid` is alive on this host. `signal 0` doesn't
/// deliver a signal — it just tests for permission to signal. `EPERM`
/// means the process exists but belongs to another user; treat as
/// alive. Any other error (`ESRCH` etc.) means dead.
///
/// Exported so non-lock callers (e.g. the status envelope builder)
/// can decide whether a lock file represents a live daemon vs. a
/// stale one without owning their own copy of the predicate.
export function isPidAlive(pid: number): boolean {
  if (pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    // EPERM means the process exists but we don't have permission.
    return (e as NodeJS.ErrnoException).code === "EPERM";
  }
}

// Internal alias kept for the existing acquireLock call sites.
function isAlive(pid: number): boolean {
  return isPidAlive(pid);
}

async function readLock(path: string): Promise<LockInfo | null> {
  try {
    const raw = await readFile(path, "utf8");
    const parsed = JSON.parse(raw) as LockInfo;
    if (typeof parsed.pid !== "number" || typeof parsed.startedAt !== "string") {
      return null;
    }
    return parsed;
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code === "ENOENT") return null;
    return null; // unreadable / corrupt → treat as missing
  }
}

export async function acquireLock(
  path: string,
  pid: number = process.pid,
  now: () => Date = () => new Date(),
): Promise<LockInfo> {
  await mkdir(dirname(path), { recursive: true });

  const existing = await readLock(path);
  if (existing && existing.pid !== pid && isAlive(existing.pid)) {
    throw new LockHeldError(existing);
  }

  // Either no lock, stale lock (dead pid), or our own pid → write fresh.
  const info: LockInfo = { pid, startedAt: now().toISOString() };
  await writeFile(path, JSON.stringify(info) + "\n", "utf8");
  return info;
}

export async function releaseLock(
  path: string,
  expectedPid: number = process.pid,
): Promise<boolean> {
  const existing = await readLock(path);
  if (!existing) return false;
  if (existing.pid !== expectedPid) return false;
  await rm(path, { force: true });
  return true;
}

export async function readLockInfo(path: string): Promise<LockInfo | null> {
  return readLock(path);
}
