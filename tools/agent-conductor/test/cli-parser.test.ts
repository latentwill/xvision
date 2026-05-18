import { describe, expect, it } from "vitest";
import { buildProgram } from "../src/cli/index.js";

describe("CLI surface", () => {
  it("exposes the full Phase-1 subcommand set", async () => {
    const program = await buildProgram();
    const names = program.commands.map((c) => c.name()).sort();
    expect(names).toEqual(["cancel", "pause", "resume", "start", "status", "stop", "watch"]);
  });

  it("--version emits the package version", async () => {
    const program = await buildProgram();
    expect(typeof program.version()).toBe("string");
    expect(program.version()).toMatch(/^\d+\.\d+\.\d+/);
  });

  it("status accepts --json", async () => {
    const program = await buildProgram();
    const status = program.commands.find((c) => c.name() === "status");
    expect(status).toBeDefined();
    expect(status!.options.some((o) => o.long === "--json")).toBe(true);
  });

  it("watch accepts --json and --interval", async () => {
    const program = await buildProgram();
    const watch = program.commands.find((c) => c.name() === "watch");
    expect(watch).toBeDefined();
    expect(watch!.options.some((o) => o.long === "--json")).toBe(true);
    expect(watch!.options.some((o) => o.long === "--interval")).toBe(true);
  });

  it("cancel takes a track argument", async () => {
    const program = await buildProgram();
    const cancel = program.commands.find((c) => c.name() === "cancel");
    expect(cancel).toBeDefined();
    expect(cancel!.registeredArguments.map((a) => a.name())).toEqual(["track"]);
  });

  it("root parses --config without dispatching", async () => {
    const program = await buildProgram();
    program.exitOverride();
    // Parse just the global flag and a help-style exit so we don't actually run.
    const args = ["node", "agent-conductor", "--config", "/tmp/x.json", "--help"];
    try {
      await program.parseAsync(args);
    } catch (e) {
      // commander.exitOverride throws on --help. Ignore.
      void e;
    }
    expect(program.opts<{ config?: string }>().config).toBe("/tmp/x.json");
  });
});
