import { describe, expect, it } from "vitest";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadConfig } from "../src/config/load.js";
import { buildStatusEnvelope } from "../src/status/envelope.js";

const CONFIG = {
  name: "demo",
  repo: { owner: "o", name: "r" },
  project: { owner: "o", number: 7 },
  paths: { worktreeRoot: ".wt", queueDir: "q" },
  branch: { prefix: "work/" },
  contractsDir: "contracts",
  schemaPath: "schema.json",
};

describe("status envelope v1", () => {
  it("has the expected top-level shape", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-env-"));
    const cfgPath = join(dir, "agent-conductor.config.json");
    await writeFile(cfgPath, JSON.stringify({ ...CONFIG, paths: { ...CONFIG.paths, cacheDir: dir } }));
    const loaded = await loadConfig(cfgPath, dir);

    const env = await buildStatusEnvelope({
      loaded,
      tasks: [],
      now: () => new Date("2026-05-18T08:00:00Z"),
    });

    expect(env.envelope.schema).toBe("agent-conductor.status/v1");
    expect(env.envelope.ts).toBe("2026-05-18T08:00:00.000Z");
    expect(env.instance.name).toBe("demo");
    expect(env.instance.repo).toBe("o/r");
    expect(env.instance.project).toBe("o:7");
    expect(env.instance.config_path).toBe(cfgPath);
    expect(env.instance.config_hash).toMatch(/^[a-f0-9]{64}$/);
    expect(env.instance.config_version).toBe("v1");
    expect(env.instance.host).toBeTruthy();
    expect(env.instance.daemon_version).toBeTruthy();
    expect(env.daemon).toMatchObject({
      pid: null,
      state: "stopped",
      shadow: false,
      enabled: true,
      poll_interval_s: 30,
    });
    expect(env.tasks).toEqual([]);
    expect(env.stuck).toEqual([]);
    expect(env.digest_tail).toEqual([]);
  });

  it("reflects an active lock and surfaces enable/shadow flags", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-env-"));
    const cfgPath = join(dir, "agent-conductor.config.json");
    await writeFile(cfgPath, JSON.stringify({ ...CONFIG, paths: { ...CONFIG.paths, cacheDir: dir } }));
    const loaded = await loadConfig(cfgPath, dir);

    const { acquireLock } = await import("../src/daemon/lock.js");
    await acquireLock(join(dir, "lock"), 4242, () => new Date("2026-05-18T07:00:00Z"));

    process.env["AGENT_CONDUCTOR_SHADOW"] = "1";
    process.env["AGENT_CONDUCTOR_ENABLE"] = "0";
    try {
      const env = await buildStatusEnvelope({ loaded, tasks: [] });
      expect(env.daemon.pid).toBe(4242);
      expect(env.daemon.started_at).toBe("2026-05-18T07:00:00.000Z");
      expect(env.daemon.shadow).toBe(true);
      expect(env.daemon.enabled).toBe(false);
    } finally {
      delete process.env["AGENT_CONDUCTOR_SHADOW"];
      delete process.env["AGENT_CONDUCTOR_ENABLE"];
    }
  });
});
