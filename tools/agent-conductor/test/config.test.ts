import { describe, expect, it } from "vitest";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ConfigError, findConfig, loadConfig } from "../src/config/load.js";

describe("config loader", () => {
  it("loads and normalizes a valid JSON config", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-cfg-"));
    const path = join(dir, "agent-conductor.config.json");
    await writeFile(
      path,
      JSON.stringify({
        name: "demo",
        repo: { owner: "o", name: "r" },
        project: { owner: "o", number: 1 },
        paths: { worktreeRoot: ".wt", queueDir: "q" },
        branch: { prefix: "x/" }, contractsDir: "contracts", schemaPath: "schema.json",
      }),
    );
    const loaded = await loadConfig(path, dir);
    expect(loaded.config.name).toBe("demo");
    expect(loaded.config.pollIntervalS).toBe(30); // default
    expect(loaded.config.version).toBe("v1");
    expect(loaded.config.paths.cacheDir).toMatch(/\.cache\/agent-conductor$/);
    expect(loaded.hash).toMatch(/^[a-f0-9]{64}$/);
  });

  it("rejects missing name with a pointed field message", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-cfg-"));
    const path = join(dir, "agent-conductor.config.json");
    await writeFile(
      path,
      JSON.stringify({
        repo: { owner: "o", name: "r" },
        project: { owner: "o", number: 1 },
        paths: { worktreeRoot: ".wt", queueDir: "q" },
        branch: { prefix: "x/" }, contractsDir: "contracts", schemaPath: "schema.json",
      }),
    );
    await expect(loadConfig(path, dir)).rejects.toBeInstanceOf(ConfigError);
  });

  it("rejects malformed JSON with the failing field highlighted", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-cfg-"));
    const path = join(dir, "agent-conductor.config.json");
    await writeFile(path, "{ not json");
    const err = await loadConfig(path, dir).catch((e) => e);
    expect(err).toBeInstanceOf(ConfigError);
    expect((err as ConfigError).message).toMatch(/not valid JSON/);
  });

  it("auto-discovers by walking up from cwd", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-cfg-"));
    const nested = join(dir, "a", "b", "c");
    await import("node:fs/promises").then((m) => m.mkdir(nested, { recursive: true }));
    const path = join(dir, "agent-conductor.config.json");
    await writeFile(
      path,
      JSON.stringify({
        name: "demo",
        repo: { owner: "o", name: "r" },
        project: { owner: "o", number: 1 },
        paths: { worktreeRoot: ".wt", queueDir: "q" },
        branch: { prefix: "x/" }, contractsDir: "contracts", schemaPath: "schema.json",
      }),
    );
    const found = await findConfig(nested);
    expect(found).toBe(path);
  });
});
