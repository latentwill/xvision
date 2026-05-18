// Status envelope v1 builder. The single source of truth for the shape
// of `status --json`, `watch --json`, and the on-disk `state.json`.

import { hostname } from "node:os";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { LoadedConfig } from "../config/load.js";
import type { BoardTask, StatusEnvelope, TaskStatus } from "../types.js";
import { readLockInfo } from "../daemon/lock.js";
import { tailDigest } from "../daemon/digest.js";
import { isShadow, isEnabled } from "../modes/env.js";

export interface BuildEnvelopeInput {
  loaded: LoadedConfig;
  tasks: BoardTask[];
  stuck?: StatusEnvelope["stuck"];
  state?: StatusEnvelope["daemon"]["state"];
  lastPollAt?: string | null;
  nextPollAt?: string | null;
  digestTailLines?: number;
  now?: () => Date;
}

const SCHEMA = "agent-conductor.status/v1" as const;

async function readPackageVersion(): Promise<string> {
  // Bundled lookup: package.json sits two levels up from src/status/.
  const thisFile = fileURLToPath(import.meta.url);
  const candidates = [
    join(dirname(thisFile), "..", "..", "package.json"),
    join(dirname(thisFile), "..", "..", "..", "package.json"),
  ];
  for (const p of candidates) {
    try {
      const raw = await readFile(p, "utf8");
      const pkg = JSON.parse(raw) as { version?: string };
      if (typeof pkg.version === "string") return pkg.version;
    } catch {
      // try next candidate
    }
  }
  return "0.0.0";
}

export async function buildStatusEnvelope(
  input: BuildEnvelopeInput,
): Promise<StatusEnvelope> {
  const now = input.now ?? (() => new Date());
  const lockPath = join(input.loaded.config.paths.cacheDir, "lock");
  const lock = await readLockInfo(lockPath);
  const version = await readPackageVersion();
  const digest = await tailDigest(
    input.loaded.config.paths.cacheDir,
    input.digestTailLines ?? 20,
  ).catch(() => [] as string[]);

  return {
    envelope: {
      schema: SCHEMA,
      ts: now().toISOString(),
    },
    instance: {
      name: input.loaded.config.name,
      repo: `${input.loaded.config.repo.owner}/${input.loaded.config.repo.name}`,
      project: `${input.loaded.config.project.owner}:${input.loaded.config.project.number}`,
      host: hostname(),
      daemon_version: version,
      config_path: input.loaded.path,
      config_hash: input.loaded.hash,
      config_version: input.loaded.config.version,
    },
    daemon: {
      pid: lock?.pid ?? null,
      started_at: lock?.startedAt ?? null,
      state: input.state ?? (lock ? "running" : "stopped"),
      shadow: isShadow(),
      enabled: isEnabled(),
      poll_interval_s: input.loaded.config.pollIntervalS,
      last_poll_at: input.lastPollAt ?? null,
      next_poll_at: input.nextPollAt ?? null,
    },
    tasks: input.tasks,
    stuck: input.stuck ?? [],
    digest_tail: digest,
  };
}

// Re-export so the CLI's status/watch can re-use without importing types in
// multiple places. Type kept local to avoid an unused-import warning.
export type { TaskStatus };
