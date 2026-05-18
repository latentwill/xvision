import { describe, expect, it } from "vitest";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  LockHeldError,
  acquireLock,
  readLockInfo,
  releaseLock,
} from "../src/daemon/lock.js";

describe("PID-file lock", () => {
  it("acquires on a fresh path and persists the holder", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-lock-"));
    const path = join(dir, "lock");
    const info = await acquireLock(path);
    expect(info.pid).toBe(process.pid);
    const raw = await readFile(path, "utf8");
    expect(JSON.parse(raw).pid).toBe(process.pid);
  });

  it("rejects when a live PID owns the lock", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-lock-"));
    const path = join(dir, "lock");
    // Pretend a different live PID (use 1 — init/launchd, always alive on macOS/Linux).
    await acquireLock(path, 1);
    await expect(acquireLock(path)).rejects.toBeInstanceOf(LockHeldError);
    const err = await acquireLock(path).catch((e) => e);
    expect(String(err)).toContain("pid 1");
  });

  it("reclaims a stale lock when the PID is dead", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-lock-"));
    const path = join(dir, "lock");
    // PID 2_147_483_647 is the max signed-32-bit int — unlikely to be in use.
    await acquireLock(path, 2_147_483_647);
    const info = await acquireLock(path);
    expect(info.pid).toBe(process.pid);
  });

  it("releaseLock only frees when the expected PID matches", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-lock-"));
    const path = join(dir, "lock");
    await acquireLock(path, 1);
    const releasedByWrong = await releaseLock(path, 2_147_483_646);
    expect(releasedByWrong).toBe(false);
    const stillHeld = await readLockInfo(path);
    expect(stillHeld?.pid).toBe(1);
    const releasedByRight = await releaseLock(path, 1);
    expect(releasedByRight).toBe(true);
    expect(await readLockInfo(path)).toBeNull();
  });
});
