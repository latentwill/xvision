// Ref-creation claim primitive.
//
// updateProjectV2ItemFieldValue has no compare-and-swap input, so the
// Project field write CANNOT itself be the claim. We use a non-force
// `git push <baseSha>:refs/heads/<branchPrefix><track>` as the atomic
// server-side claim — exactly one racing daemon will succeed; the rest
// see GitHub's `reference already exists` / non-fast-forward error and
// back off with zero side effects.
//
// The daemon-side flow is:
//   1. host-conflict check (worktree clean? OWNERSHIP overlap?)
//   2. local advisory lock <cacheDir>/claims/<track>.lock (fast path)
//   3. SERVER-SIDE CLAIM: git push origin <baseSha>:refs/heads/<branch>
//   4. refuse if local <worktreeRoot>/<track> already exists dirty; rollback ref
//   5. git fetch + git worktree add
//   6. write queue marker
//   7. update Project: status=CLAIMED + owner_agent + branch + worktree
//   8. spawn claude worker
//   9. verify-after-write: re-read Project item; rollback on identity mismatch
//
// This module exposes the primitive as a thin function over abstracted
// host interfaces (`GitClient`, `GhClient`, `Spawn`, `Fs`) so the
// concurrency tests can drive each step against an in-memory mock.

import { existsSync } from "node:fs";
import { mkdir, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";

export interface GitClient {
  push(args: {
    remote: string;
    src: string; // sha
    dest: string; // refs/heads/...
    force?: boolean;
  }): Promise<void>;
  pushDelete(args: { remote: string; dest: string }): Promise<void>;
  fetch(args: { remote: string; ref: string }): Promise<void>;
  worktreeAdd(args: { path: string; ref: string }): Promise<void>;
  worktreeRemove(args: { path: string; force?: boolean }): Promise<void>;
  resolveRef(ref: string): Promise<string | null>;
}

export interface GhClient {
  updateProjectStatus(args: {
    project: { owner: string; number: number };
    itemId: string;
    fields: {
      status: string;
      owner_agent?: string;
      branch?: string;
      worktree?: string;
    };
  }): Promise<void>;
  readProjectItem(args: {
    project: { owner: string; number: number };
    itemId: string;
  }): Promise<{ owner_agent: string | null }>;
}

export interface Spawn {
  spawnWorker(args: {
    cwd: string;
    contractPath: string;
    track: string;
  }): Promise<{ pid: number }>;
  killWorker(pid: number): Promise<void>;
}

export interface ClaimEnv {
  git: GitClient;
  gh: GhClient;
  spawn: Spawn;
  cacheDir: string;
  worktreeRoot: string;
  queueDir: string;
  branchPrefix: string;
  remote: string; // typically "origin"
  ownerAgent: string; // "<instance.name>:<host>"
  contractsDir: string;
  shadow: boolean;
  now?: () => Date;
}

export interface ClaimRequest {
  track: string;
  itemId: string;
  baseSha: string;
}

export type ClaimOutcome =
  | { kind: "claimed"; branch: string; worktree: string; workerPid: number }
  | { kind: "skipped"; reason: string }
  | { kind: "rolled-back"; reason: string }
  | { kind: "shadow"; plan: string };

export class RefAlreadyExists extends Error {
  constructor(ref: string) {
    super(`reference already exists: ${ref}`);
    this.name = "RefAlreadyExists";
  }
}

function utcStamp(d: Date): string {
  // 2026-05-18T08-00-00Z — safe for filenames.
  return d.toISOString().replace(/[:]/g, "-").replace(/\.\d{3}Z$/, "Z");
}

export async function claim(
  env: ClaimEnv,
  req: ClaimRequest,
): Promise<ClaimOutcome> {
  const branch = `${env.branchPrefix}${req.track}`;
  const branchRef = `refs/heads/${branch}`;
  const worktree = `${env.worktreeRoot}/${req.track}`;
  const now = env.now ?? (() => new Date());

  if (env.shadow) {
    return {
      kind: "shadow",
      plan: `would claim ${req.track} via push ${req.baseSha}:${branchRef} → worktree ${worktree}`,
    };
  }

  // 1. host-conflict check is a host-repo concern; the caller supplies a
  //    `skipReason` by simply not calling claim() for conflicting tracks.

  // 2. Local advisory lock — fast path only; the authority is step 3.
  const lockPath = join(env.cacheDir, "claims", `${req.track}.lock`);
  await mkdir(dirname(lockPath), { recursive: true });
  try {
    await writeFile(lockPath, `${process.pid}\n`, { flag: "wx" });
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code === "EEXIST") {
      return { kind: "skipped", reason: `local lock held: ${lockPath}` };
    }
    throw e;
  }

  // Always release the local lock when this function returns, regardless
  // of outcome. The remote ref is the authority.
  const releaseLocalLock = async () => {
    await rm(lockPath, { force: true });
  };

  try {
    // 3. SERVER-SIDE CLAIM.
    try {
      await env.git.push({
        remote: env.remote,
        src: req.baseSha,
        dest: branchRef,
      });
    } catch (e) {
      if (e instanceof RefAlreadyExists) {
        return {
          kind: "skipped",
          reason: `remote ref ${branchRef} already exists; another daemon or operator owns this claim`,
        };
      }
      throw e;
    }

    // 4. refuse if worktree already exists dirty; rollback the ref.
    if (existsSync(worktree)) {
      await safeRollbackRef(env, branchRef, req.baseSha);
      return {
        kind: "rolled-back",
        reason: `worktree ${worktree} already present locally; surfaced to digest`,
      };
    }

    // 5. fetch + worktree add.
    await env.git.fetch({ remote: env.remote, ref: branch });
    await env.git.worktreeAdd({ path: worktree, ref: branch });

    // 6. queue marker.
    const queuePath = join(
      env.queueDir,
      `${req.track}__${utcStamp(now())}__claimed.md`,
    );
    await mkdir(dirname(queuePath), { recursive: true });
    await writeFile(
      queuePath,
      [
        `# ${req.track}`,
        ``,
        `claimed_by: ${env.ownerAgent}`,
        `daemon_pid: ${process.pid}`,
        `base_sha: ${req.baseSha}`,
        `branch: ${branch}`,
        `worktree: ${worktree}`,
        `at: ${now().toISOString()}`,
        ``,
      ].join("\n"),
      "utf8",
    );

    // 7. update Project.
    await env.gh.updateProjectStatus({
      project: { owner: "", number: 0 }, // caller fills via wrapper
      itemId: req.itemId,
      fields: {
        status: "CLAIMED",
        owner_agent: env.ownerAgent,
        branch,
        worktree,
      },
    });

    // 8. spawn worker.
    const contractPath = `${env.contractsDir}/${req.track}.md`;
    const worker = await env.spawn.spawnWorker({
      cwd: worktree,
      contractPath,
      track: req.track,
    });
    await writeFile(
      queuePath,
      `worker_pid: ${worker.pid}\n`,
      { flag: "a", encoding: "utf8" },
    );

    // 9. verify-after-write.
    const echoed = await env.gh.readProjectItem({
      project: { owner: "", number: 0 },
      itemId: req.itemId,
    });
    if (echoed.owner_agent !== env.ownerAgent) {
      await env.spawn.killWorker(worker.pid);
      await env.git
        .worktreeRemove({ path: worktree, force: true })
        .catch(() => undefined);
      await safeRollbackRef(env, branchRef, req.baseSha);
      return {
        kind: "rolled-back",
        reason: `verify-after-write mismatch: project owner_agent is "${echoed.owner_agent}", expected "${env.ownerAgent}"`,
      };
    }

    return { kind: "claimed", branch, worktree, workerPid: worker.pid };
  } finally {
    await releaseLocalLock();
  }
}

async function safeRollbackRef(
  env: ClaimEnv,
  ref: string,
  expectedSha: string,
): Promise<void> {
  // Only delete the ref if its tip still equals our base sha — never
  // clobber another daemon's work.
  const tip = await env.git.resolveRef(ref).catch(() => null);
  if (tip === expectedSha) {
    await env.git.pushDelete({ remote: env.remote, dest: ref }).catch(() => undefined);
  }
}
