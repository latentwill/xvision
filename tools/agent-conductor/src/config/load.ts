// Config loader. Supports `agent-conductor.config.json` (and `.ts` via
// dynamic import once compiled). Validates against AgentConductorConfig.
// Refuses to load with a pointed message on the failing field.

import { createHash } from "node:crypto";
import { readFile, stat } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, isAbsolute, join, resolve } from "node:path";
import type { AgentConductorConfig } from "../types.js";

export interface LoadedConfig {
  config: Required<
    Omit<AgentConductorConfig, "paths" | "branch" | "project" | "repo">
  > & {
    repo: AgentConductorConfig["repo"];
    project: AgentConductorConfig["project"];
    paths: Required<AgentConductorConfig["paths"]>;
    branch: AgentConductorConfig["branch"];
  };
  path: string;
  hash: string;
}

export class ConfigError extends Error {
  readonly field?: string;
  constructor(message: string, field?: string) {
    super(message);
    this.name = "ConfigError";
    if (field !== undefined) this.field = field;
  }
}

const CONFIG_FILENAMES = [
  "agent-conductor.config.json",
  "agent-conductor.config.ts",
];

export async function findConfig(startDir: string): Promise<string | null> {
  let dir = resolve(startDir);
  // Walk up until filesystem root.
  while (true) {
    for (const name of CONFIG_FILENAMES) {
      const candidate = join(dir, name);
      try {
        await stat(candidate);
        return candidate;
      } catch {
        // not here, keep walking
      }
    }
    const parent = dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

export async function loadConfig(
  explicitPath: string | undefined,
  cwd: string,
): Promise<LoadedConfig> {
  const path = explicitPath
    ? isAbsolute(explicitPath)
      ? explicitPath
      : resolve(cwd, explicitPath)
    : await findConfig(cwd);

  if (!path) {
    throw new ConfigError(
      `config file not found. Pass --config <path> or place agent-conductor.config.json in the repo root (searched up from ${cwd}).`,
    );
  }

  let raw: string;
  try {
    raw = await readFile(path, "utf8");
  } catch (e) {
    throw new ConfigError(`failed to read config at ${path}: ${(e as Error).message}`);
  }

  let parsed: unknown;
  if (path.endsWith(".json")) {
    try {
      parsed = JSON.parse(raw);
    } catch (e) {
      throw new ConfigError(`config at ${path} is not valid JSON: ${(e as Error).message}`);
    }
  } else if (path.endsWith(".ts")) {
    try {
      // tsx exposes a programmatic ESM TypeScript loader. Importing the
      // config gives us back its default export.
      const tsx = (await import("tsx/esm/api")) as {
        tsImport: (specifier: string, parent: string) => Promise<unknown>;
      };
      const mod = (await tsx.tsImport(path, import.meta.url)) as {
        default?: unknown;
      };
      if (!mod || typeof mod !== "object" || !("default" in mod)) {
        throw new ConfigError(
          `TypeScript config at ${path} must have a default export`,
        );
      }
      // tsImport sometimes wraps a CJS-style default in another esModule
      // envelope ({ __esModule: true, default: <real value> }). Peel it.
      const top = mod.default as { __esModule?: boolean; default?: unknown } | unknown;
      parsed =
        top && typeof top === "object" && (top as Record<string, unknown>)["__esModule"] === true &&
        "default" in (top as Record<string, unknown>)
          ? (top as { default: unknown }).default
          : top;
    } catch (e) {
      if (e instanceof ConfigError) throw e;
      throw new ConfigError(
        `failed to load TypeScript config at ${path}: ${(e as Error).message}`,
      );
    }
  } else {
    throw new ConfigError(`unsupported config extension: ${path}`);
  }

  const validated = validateConfig(parsed);
  const hash = createHash("sha256").update(raw).digest("hex");

  const config: LoadedConfig["config"] = {
    version: validated.version ?? "v1",
    name: validated.name,
    repo: validated.repo,
    project: validated.project,
    paths: {
      worktreeRoot: validated.paths.worktreeRoot,
      queueDir: validated.paths.queueDir,
      cacheDir:
        validated.paths.cacheDir ??
        join(homedir(), ".cache", "agent-conductor"),
    },
    branch: validated.branch,
    pollIntervalS: validated.pollIntervalS ?? 30,
    contractsDir: validated.contractsDir,
    schemaPath: validated.schemaPath,
  };

  return { config, path: resolve(path), hash };
}

function validateConfig(input: unknown): AgentConductorConfig {
  if (!input || typeof input !== "object") {
    throw new ConfigError("config root must be an object", "root");
  }
  const cfg = input as Record<string, unknown>;

  const name = cfg["name"];
  if (typeof name !== "string" || name.trim().length === 0) {
    throw new ConfigError(
      "config.name must be a non-empty string; daemon refuses to start without an instance name",
      "name",
    );
  }

  const repo = cfg["repo"] as Record<string, unknown> | undefined;
  if (!repo || typeof repo["owner"] !== "string" || typeof repo["name"] !== "string") {
    throw new ConfigError("config.repo.{owner,name} are required strings", "repo");
  }

  const project = cfg["project"] as Record<string, unknown> | undefined;
  if (
    !project ||
    typeof project["owner"] !== "string" ||
    typeof project["number"] !== "number"
  ) {
    throw new ConfigError(
      "config.project.{owner:string, number:integer} are required",
      "project",
    );
  }

  const paths = cfg["paths"] as Record<string, unknown> | undefined;
  if (
    !paths ||
    typeof paths["worktreeRoot"] !== "string" ||
    typeof paths["queueDir"] !== "string"
  ) {
    throw new ConfigError(
      "config.paths.{worktreeRoot, queueDir} are required strings",
      "paths",
    );
  }
  if (paths["cacheDir"] !== undefined && typeof paths["cacheDir"] !== "string") {
    throw new ConfigError("config.paths.cacheDir must be a string", "paths.cacheDir");
  }

  const branch = cfg["branch"] as Record<string, unknown> | undefined;
  if (!branch || typeof branch["prefix"] !== "string") {
    throw new ConfigError("config.branch.prefix is a required string", "branch.prefix");
  }

  const version = cfg["version"];
  if (version !== undefined && typeof version !== "string") {
    throw new ConfigError("config.version must be a string", "version");
  }

  const poll = cfg["pollIntervalS"];
  if (poll !== undefined && (typeof poll !== "number" || poll < 1)) {
    throw new ConfigError(
      "config.pollIntervalS must be a positive number",
      "pollIntervalS",
    );
  }

  const contractsDir = cfg["contractsDir"];
  if (typeof contractsDir !== "string" || contractsDir.length === 0) {
    throw new ConfigError(
      "config.contractsDir is required (the daemon has no default — host repo decides)",
      "contractsDir",
    );
  }

  const schemaPath = cfg["schemaPath"];
  if (typeof schemaPath !== "string" || schemaPath.length === 0) {
    throw new ConfigError(
      "config.schemaPath is required (path to the board task JSON Schema)",
      "schemaPath",
    );
  }

  return {
    version: version as string | undefined,
    name,
    repo: { owner: repo["owner"] as string, name: repo["name"] as string },
    project: {
      owner: project["owner"] as string,
      number: project["number"] as number,
    },
    paths: {
      worktreeRoot: paths["worktreeRoot"] as string,
      queueDir: paths["queueDir"] as string,
      cacheDir: paths["cacheDir"] as string | undefined,
    },
    branch: { prefix: branch["prefix"] as string },
    pollIntervalS: poll as number | undefined,
    contractsDir,
    schemaPath,
  };
}
