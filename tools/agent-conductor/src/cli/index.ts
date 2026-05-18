// CLI entry. Subcommands: start, stop, pause, resume, status, watch,
// cancel, --help, --version.
//
// `start`/`stop` take a `--config` flag and acquire/release the PID lock.
// `status`/`watch` always emit JSON to stdout when `--json` is set; without
// `--json` and on a TTY they render a small text summary (no ink dep —
// keeps the runtime small and avoids a dev-only optional dep).

import { Command } from "commander";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { loadConfig, ConfigError } from "../config/load.js";
import { LockHeldError, acquireLock, releaseLock, readLockInfo } from "../daemon/lock.js";
import { buildStatusEnvelope } from "../status/envelope.js";
import { isEnabled } from "../modes/env.js";

async function readPkgVersion(): Promise<string> {
  const here = dirname(fileURLToPath(import.meta.url));
  for (const p of [
    join(here, "..", "..", "package.json"),
    join(here, "..", "..", "..", "package.json"),
  ]) {
    try {
      const raw = await readFile(p, "utf8");
      const v = (JSON.parse(raw) as { version?: string }).version;
      if (typeof v === "string") return v;
    } catch {
      // try next
    }
  }
  return "0.0.0";
}

interface GlobalOpts {
  config?: string;
  json?: boolean;
}

function configOptionFrom(cmd: Command): string | undefined {
  // Walk up to the root command so subcommands inherit --config.
  let c: Command | null = cmd;
  while (c) {
    const opts = c.opts<GlobalOpts>();
    if (opts.config) return opts.config;
    c = c.parent;
  }
  return undefined;
}

export async function buildProgram(): Promise<Command> {
  const program = new Command();
  const version = await readPkgVersion();

  program
    .name("agent-conductor")
    .description(
      "Local control plane that claims READY tasks, manages worktrees, spawns Claude Code workers, opens PRs, and archives merged work.",
    )
    .version(version, "-V, --version")
    .option("--config <path>", "path to agent-conductor.config.json");

  program
    .command("start")
    .description("acquire the PID lock and begin the poll loop")
    .action(async function (this: Command) {
      if (!isEnabled()) {
        process.stdout.write(
          "kill switch engaged (AGENT_CONDUCTOR_ENABLE=0); exiting cleanly\n",
        );
        process.exitCode = 0;
        return;
      }
      const loaded = await loadOrDie(configOptionFrom(this));
      try {
        const info = await acquireLock(join(loaded.config.paths.cacheDir, "lock"));
        process.stdout.write(
          `acquired lock at ${join(loaded.config.paths.cacheDir, "lock")} (pid ${info.pid})\n`,
        );
        // Phase-1 skeleton: the poll loop wires up here in a later commit.
        // For now we exit so `start` is testable as a one-shot.
      } catch (e) {
        if (e instanceof LockHeldError) {
          process.stderr.write(`${e.message}\n`);
          process.exit(1);
        }
        throw e;
      }
    });

  program
    .command("stop")
    .description("release the PID lock (no-op if not held)")
    .action(async function (this: Command) {
      const loaded = await loadOrDie(configOptionFrom(this));
      const lockPath = join(loaded.config.paths.cacheDir, "lock");
      const info = await readLockInfo(lockPath);
      if (!info) {
        process.stdout.write("no lock held\n");
        return;
      }
      // Best effort: signal the holder and clear the lock when it's our
      // own PID (test path). Real operator use would `launchctl bootout`
      // or send SIGTERM out-of-band.
      if (info.pid === process.pid) {
        await releaseLock(lockPath);
        process.stdout.write("released lock (held by current process)\n");
        return;
      }
      try {
        process.kill(info.pid, "SIGTERM");
        process.stdout.write(`sent SIGTERM to pid ${info.pid}\n`);
      } catch (e) {
        process.stderr.write(
          `could not signal pid ${info.pid}: ${(e as Error).message}\n`,
        );
      }
    });

  program
    .command("pause")
    .description("set a sentinel that pauses the poll loop without exiting")
    .action(async function (this: Command) {
      const loaded = await loadOrDie(configOptionFrom(this));
      const path = join(loaded.config.paths.cacheDir, "paused");
      const { mkdir, writeFile } = await import("node:fs/promises");
      await mkdir(dirname(path), { recursive: true });
      await writeFile(path, new Date().toISOString() + "\n", "utf8");
      process.stdout.write(`paused (sentinel: ${path})\n`);
    });

  program
    .command("resume")
    .description("remove the pause sentinel")
    .action(async function (this: Command) {
      const loaded = await loadOrDie(configOptionFrom(this));
      const path = join(loaded.config.paths.cacheDir, "paused");
      const { rm } = await import("node:fs/promises");
      await rm(path, { force: true });
      process.stdout.write("resumed\n");
    });

  program
    .command("status")
    .description("print the v1 status envelope")
    .option("--json", "force JSON output")
    .action(async function (this: Command) {
      const loaded = await loadOrDie(configOptionFrom(this));
      const env = await buildStatusEnvelope({ loaded, tasks: [] });
      const wantJson = Boolean(this.opts<GlobalOpts>().json) || !process.stdout.isTTY;
      if (wantJson) {
        process.stdout.write(JSON.stringify(env, null, 2) + "\n");
        return;
      }
      process.stdout.write(renderStatusText(env));
    });

  program
    .command("watch")
    .description("emit one v1 status envelope per poll until SIGINT")
    .option("--json", "force JSON output")
    .option("--interval <seconds>", "override poll interval", parseInteger)
    .action(async function (this: Command) {
      const loaded = await loadOrDie(configOptionFrom(this));
      const intervalOpt = this.opts<{ interval?: number }>().interval;
      const intervalS =
        intervalOpt ?? loaded.config.pollIntervalS ?? 30;
      const wantJson = Boolean(this.opts<GlobalOpts>().json) || !process.stdout.isTTY;

      const tick = async () => {
        const envObj = await buildStatusEnvelope({ loaded, tasks: [] });
        if (wantJson) {
          process.stdout.write(JSON.stringify(envObj) + "\n");
        } else {
          process.stdout.write(renderStatusText(envObj));
        }
      };

      await tick();
      const handle = setInterval(() => {
        tick().catch((e) => {
          process.stderr.write(`watch tick failed: ${(e as Error).message}\n`);
        });
      }, intervalS * 1000);
      const onSig = () => {
        clearInterval(handle);
        process.exit(0);
      };
      process.on("SIGINT", onSig);
      process.on("SIGTERM", onSig);
    });

  program
    .command("cancel <track>")
    .description("cancel an in-flight claim (Phase-1: writes a queue marker)")
    .action(async function (this: Command, track: string) {
      const loaded = await loadOrDie(configOptionFrom(this));
      const { mkdir, writeFile } = await import("node:fs/promises");
      const ts = new Date().toISOString().replace(/[:]/g, "-").replace(/\.\d{3}Z$/, "Z");
      const path = join(loaded.config.paths.queueDir, `${track}__${ts}__cancel.md`);
      await mkdir(dirname(path), { recursive: true });
      await writeFile(
        path,
        `# ${track}\n\ncancel_requested_at: ${new Date().toISOString()}\n`,
        "utf8",
      );
      process.stdout.write(`cancel marker written: ${path}\n`);
    });

  return program;
}

async function loadOrDie(explicit: string | undefined) {
  try {
    return await loadConfig(explicit, process.cwd());
  } catch (e) {
    if (e instanceof ConfigError) {
      process.stderr.write(`config error: ${e.message}\n`);
      process.exit(1);
    }
    throw e;
  }
}

function parseInteger(value: string): number {
  const n = Number(value);
  if (!Number.isFinite(n) || n < 1) {
    throw new Error(`expected positive integer, got ${value}`);
  }
  return n;
}

function renderStatusText(env: Awaited<ReturnType<typeof buildStatusEnvelope>>): string {
  const lines: string[] = [];
  lines.push(`agent-conductor v${env.instance.daemon_version} (${env.envelope.schema})`);
  lines.push(`  instance: ${env.instance.name}@${env.instance.host}`);
  lines.push(`  repo:     ${env.instance.repo}`);
  lines.push(`  project:  ${env.instance.project}`);
  lines.push(
    `  daemon:   state=${env.daemon.state} pid=${env.daemon.pid ?? "-"} shadow=${env.daemon.shadow} enabled=${env.daemon.enabled} poll=${env.daemon.poll_interval_s}s`,
  );
  lines.push(`  tasks:    ${env.tasks.length}`);
  lines.push(`  stuck:    ${env.stuck.length}`);
  lines.push(`  digest:   ${env.digest_tail.length} tail lines`);
  return lines.join("\n") + "\n";
}

// Wrapper so the bin script is a one-liner.
export async function runCli(argv: string[]): Promise<void> {
  const program = await buildProgram();
  await program.parseAsync(argv);
}
