// Concurrency tests for the ref-creation claim primitive. The mocked
// GitClient + GhClient serialize the "create refs/heads/agent/<track>"
// step so we can prove exactly one of two racing daemons advances past
// it. The other backs off with `reference already exists` and writes
// nothing.

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  claim,
  RefAlreadyExists,
  type ClaimEnv,
  type GhClient,
  type GitClient,
  type Spawn,
} from "../src/claim/primitive.js";

interface FakeGhState {
  owner_agent: string | null;
  reads: number;
  writes: number;
  // Each write/read records the Project coordinates the caller passed
  // so the test can assert claim() threads `env.project` through (vs.
  // the pre-fix hardcoded `("", 0)`).
  writeProjects: Array<{ owner: string; number: number }>;
  readProjects: Array<{ owner: string; number: number }>;
}

function makeMocks(): {
  git: GitClient & { existingRefs: Set<string>; pushed: string[]; deleted: string[] };
  gh: GhClient & { state: FakeGhState };
  spawn: Spawn & { spawned: number[]; killed: number[] };
  resolved: Record<string, string>;
} {
  const existingRefs = new Set<string>();
  const pushed: string[] = [];
  const deleted: string[] = [];
  const resolved: Record<string, string> = {};

  const git = {
    existingRefs,
    pushed,
    deleted,
    async push({ remote, src, dest }) {
      void remote;
      if (existingRefs.has(dest)) {
        throw new RefAlreadyExists(dest);
      }
      existingRefs.add(dest);
      resolved[dest] = src;
      pushed.push(dest);
    },
    async pushDelete({ dest }) {
      existingRefs.delete(dest);
      delete resolved[dest];
      deleted.push(dest);
    },
    async fetch() {},
    async worktreeAdd() {},
    async worktreeRemove() {},
    async resolveRef(ref) {
      return resolved[ref] ?? null;
    },
  } as GitClient & { existingRefs: Set<string>; pushed: string[]; deleted: string[] };

  const gh = {
    state: {
      owner_agent: null,
      reads: 0,
      writes: 0,
      writeProjects: [],
      readProjects: [],
    } as FakeGhState,
    async updateProjectStatus({ project, fields }) {
      this.state.writes += 1;
      this.state.writeProjects.push({ owner: project.owner, number: project.number });
      if (typeof fields.owner_agent === "string") {
        this.state.owner_agent = fields.owner_agent;
      }
    },
    async readProjectItem({ project }) {
      this.state.reads += 1;
      this.state.readProjects.push({ owner: project.owner, number: project.number });
      return { owner_agent: this.state.owner_agent };
    },
  } as GhClient & { state: FakeGhState };

  const spawn = {
    spawned: [] as number[],
    killed: [] as number[],
    async spawnWorker() {
      const pid = 10000 + this.spawned.length + 1;
      this.spawned.push(pid);
      return { pid };
    },
    async killWorker(pid) {
      this.killed.push(pid);
    },
  } as Spawn & { spawned: number[]; killed: number[] };

  return { git, gh, spawn, resolved };
}

const TEST_PROJECT = { owner: "latentwill", number: 42 };

function envOf(
  cacheDir: string,
  queueDir: string,
  worktreeRoot: string,
  mocks: ReturnType<typeof makeMocks>,
  ownerAgent: string,
  project: { owner: string; number: number } = TEST_PROJECT,
): ClaimEnv {
  return {
    git: mocks.git,
    gh: mocks.gh,
    spawn: mocks.spawn,
    cacheDir,
    worktreeRoot,
    queueDir,
    branchPrefix: "agent/",
    remote: "origin",
    ownerAgent,
    contractsDir: "contracts",
    project,
    shadow: false,
  };
}

let workDir: string;

beforeEach(async () => {
  workDir = await mkdtemp(join(tmpdir(), "ac-claim-"));
});

afterEach(async () => {
  await rm(workDir, { recursive: true, force: true });
});

describe("claim primitive", () => {
  it("two daemons race; exactly one advances past the ref-create", async () => {
    const mocks = makeMocks();
    const e1 = envOf(
      join(workDir, "cache-1"),
      join(workDir, "queue"),
      join(workDir, "worktrees-1"),
      mocks,
      "alpha:host-a",
    );
    const e2 = envOf(
      join(workDir, "cache-2"),
      join(workDir, "queue"),
      join(workDir, "worktrees-2"),
      mocks,
      "beta:host-b",
    );

    const req = { track: "demo", itemId: "ITEM1", baseSha: "deadbeef" };
    const [r1, r2] = await Promise.all([claim(e1, req), claim(e2, req)]);

    const outcomes = [r1.kind, r2.kind].sort();
    expect(outcomes).toEqual(["claimed", "skipped"]);

    expect(mocks.git.pushed).toEqual(["refs/heads/agent/demo"]); // one push only
    expect(mocks.spawn.spawned.length).toBe(1); // one worker spawn
    expect(mocks.gh.state.writes).toBe(1); // one Project write
    // Project coords from env.project are threaded into both the write
    // and the verify-after-write read. Pre-fix this was `("", 0)` and
    // a real GhClient would have targeted an invalid Project.
    expect(mocks.gh.state.writeProjects).toEqual([TEST_PROJECT]);
    expect(mocks.gh.state.readProjects).toEqual([TEST_PROJECT]);
  });

  it("operator beat-the-daemon: pre-existing remote ref → skip with no side effects", async () => {
    const mocks = makeMocks();
    mocks.git.existingRefs.add("refs/heads/agent/demo");

    const env = envOf(
      join(workDir, "cache"),
      join(workDir, "queue"),
      join(workDir, "worktrees"),
      mocks,
      "alpha:host-a",
    );

    const out = await claim(env, { track: "demo", itemId: "X", baseSha: "abc" });
    expect(out.kind).toBe("skipped");
    expect(mocks.git.pushed).toEqual([]);
    expect(mocks.spawn.spawned).toEqual([]);
    expect(mocks.gh.state.writes).toBe(0);
    // No queue marker either.
    let queue: string[] = [];
    try {
      queue = await readdir(join(workDir, "queue"));
    } catch {
      queue = [];
    }
    expect(queue).toEqual([]);
  });

  it("rolls back when verify-after-write sees a different owner_agent", async () => {
    const mocks = makeMocks();
    const env = envOf(
      join(workDir, "cache"),
      join(workDir, "queue"),
      join(workDir, "worktrees"),
      mocks,
      "alpha:host-a",
    );
    // Have GhClient.readProjectItem report a foreign owner.
    mocks.gh.readProjectItem = async function (this: FakeGhState) {
      return { owner_agent: "intruder:host-z" };
    } as unknown as GhClient["readProjectItem"];

    const out = await claim(env, { track: "demo", itemId: "X", baseSha: "abc" });
    expect(out.kind).toBe("rolled-back");
    expect(mocks.spawn.killed.length).toBe(1);
    expect(mocks.git.deleted).toContain("refs/heads/agent/demo");
  });
});
