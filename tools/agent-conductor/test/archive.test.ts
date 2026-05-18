import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  archive,
  type ArchiveEnv,
  type ArchiveGhClient,
  type ArchiveGitClient,
} from "../src/archive/flow.js";

interface SpyGit extends ArchiveGitClient {
  worktreesRemoved: string[];
  branchesDeleted: string[];
}
interface SpyGh extends ArchiveGhClient {
  projectWrites: Array<{ itemId: string; status: string }>;
  issuesClosed: number[];
}

function spies(): { git: SpyGit; gh: SpyGh } {
  const git: SpyGit = {
    worktreesRemoved: [],
    branchesDeleted: [],
    async worktreeRemove({ path }) {
      this.worktreesRemoved.push(path);
    },
    async branchDelete({ name }) {
      this.branchesDeleted.push(name);
    },
  };
  const gh: SpyGh = {
    projectWrites: [],
    issuesClosed: [],
    async updateProjectStatus({ itemId, fields }) {
      this.projectWrites.push({ itemId, status: fields.status });
    },
    async closeIssue({ number }) {
      this.issuesClosed.push(number);
    },
  };
  return { git, gh };
}

let workDir: string;

beforeEach(async () => {
  workDir = await mkdtemp(join(tmpdir(), "ac-archive-"));
});

afterEach(async () => {
  await rm(workDir, { recursive: true, force: true });
});

describe("archive flow", () => {
  it("moves queue markers, removes worktree+branch, closes issue, updates Project", async () => {
    const queueDir = join(workDir, "queue");
    await mkdir(queueDir, { recursive: true });
    await writeFile(join(queueDir, "demo__2026-05-18T08-00-00Z__claimed.md"), "");
    await writeFile(join(queueDir, "demo__2026-05-18T08-30-00Z__blocked.md"), "");
    await writeFile(join(queueDir, "other__2026-05-18T08-30-00Z__claimed.md"), "untouched");

    const sp = spies();
    const env: ArchiveEnv = {
      git: sp.git,
      gh: sp.gh,
      worktreeRoot: join(workDir, "worktrees"),
      queueDir,
      branchPrefix: "agent/",
      shadow: false,
      now: () => new Date("2026-05-18T09:00:00Z"),
    };

    const out = await archive(env, {
      track: "demo",
      itemId: "ITEM1",
      issueNumber: 99,
      repo: { owner: "o", name: "r" },
      project: { owner: "o", number: 7 },
    });

    expect(out.kind).toBe("archived");
    if (out.kind !== "archived") throw new Error("type narrow");
    expect(out.movedQueueFiles.length).toBe(2);

    expect(sp.git.worktreesRemoved).toEqual([join(workDir, "worktrees", "demo")]);
    expect(sp.git.branchesDeleted).toEqual(["agent/demo"]);
    expect(sp.gh.projectWrites).toEqual([{ itemId: "ITEM1", status: "ARCHIVED" }]);
    expect(sp.gh.issuesClosed).toEqual([99]);

    const archiveDir = join(queueDir, "archive", "2026-05-18");
    const archived = await readdir(archiveDir);
    expect(archived.sort()).toEqual([
      "demo__2026-05-18T08-00-00Z__claimed.md",
      "demo__2026-05-18T08-30-00Z__blocked.md",
    ]);
    const remaining = await readdir(queueDir);
    expect(remaining).toContain("other__2026-05-18T08-30-00Z__claimed.md");
  });

  it("shadow mode plans without side effects", async () => {
    const sp = spies();
    const env: ArchiveEnv = {
      git: sp.git,
      gh: sp.gh,
      worktreeRoot: join(workDir, "wt"),
      queueDir: join(workDir, "queue"),
      branchPrefix: "agent/",
      shadow: true,
    };
    const out = await archive(env, {
      track: "demo",
      itemId: "X",
      issueNumber: 1,
      repo: { owner: "o", name: "r" },
      project: { owner: "o", number: 7 },
    });
    expect(out.kind).toBe("shadow");
    expect(sp.git.worktreesRemoved).toEqual([]);
    expect(sp.gh.projectWrites).toEqual([]);
    expect(sp.gh.issuesClosed).toEqual([]);
  });
});
