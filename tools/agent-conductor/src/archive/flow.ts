// MERGED → ARCHIVED.
//
// (1) detect PR merged on default branch
// (2) git worktree remove --force <worktreeRoot>/<track>
// (3) git branch -D <branchPrefix><track> locally
// (4) move <queueDir>/<track>__*.md → <queueDir>/archive/<date>/
// (5) update Project field status=ARCHIVED
// (6) close the Issue if not already closed
//
// Like claim(), this is exposed as a function over abstract host
// interfaces so tests can drive the steps deterministically.

import { mkdir, readdir, rename } from "node:fs/promises";
import { join } from "node:path";

export interface ArchiveGitClient {
  worktreeRemove(args: { path: string; force?: boolean }): Promise<void>;
  branchDelete(args: { name: string; force?: boolean }): Promise<void>;
}

export interface ArchiveGhClient {
  updateProjectStatus(args: {
    project: { owner: string; number: number };
    itemId: string;
    fields: { status: "ARCHIVED" };
  }): Promise<void>;
  closeIssue(args: { repo: { owner: string; name: string }; number: number }): Promise<void>;
}

export interface ArchiveEnv {
  git: ArchiveGitClient;
  gh: ArchiveGhClient;
  worktreeRoot: string;
  queueDir: string;
  branchPrefix: string;
  shadow: boolean;
  now?: () => Date;
}

export interface ArchiveRequest {
  track: string;
  itemId: string;
  issueNumber?: number;
  repo: { owner: string; name: string };
  project: { owner: string; number: number };
}

export type ArchiveOutcome =
  | { kind: "archived"; movedQueueFiles: string[] }
  | { kind: "shadow"; plan: string };

function ymdUtc(d: Date): string {
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, "0");
  const day = String(d.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export async function archive(
  env: ArchiveEnv,
  req: ArchiveRequest,
): Promise<ArchiveOutcome> {
  const branch = `${env.branchPrefix}${req.track}`;
  const worktree = `${env.worktreeRoot}/${req.track}`;
  const now = env.now ?? (() => new Date());

  if (env.shadow) {
    return {
      kind: "shadow",
      plan: `would archive ${req.track}: remove worktree ${worktree}, delete branch ${branch}, move queue markers, set status=ARCHIVED, close issue #${req.issueNumber ?? "?"}`,
    };
  }

  // (2) worktree remove (idempotent on missing).
  await env.git
    .worktreeRemove({ path: worktree, force: true })
    .catch(() => undefined);

  // (3) local branch delete (idempotent on missing).
  await env.git.branchDelete({ name: branch, force: true }).catch(() => undefined);

  // (4) move queue markers.
  const archiveDir = join(env.queueDir, "archive", ymdUtc(now()));
  await mkdir(archiveDir, { recursive: true });
  const moved: string[] = [];
  let entries: string[] = [];
  try {
    entries = await readdir(env.queueDir);
  } catch {
    entries = [];
  }
  for (const name of entries) {
    if (!name.startsWith(`${req.track}__`)) continue;
    if (!name.endsWith(".md")) continue;
    const from = join(env.queueDir, name);
    const to = join(archiveDir, name);
    await rename(from, to).catch(() => undefined);
    moved.push(to);
  }

  // (5) Project: status=ARCHIVED.
  await env.gh.updateProjectStatus({
    project: req.project,
    itemId: req.itemId,
    fields: { status: "ARCHIVED" },
  });

  // (6) close Issue if we have its number.
  if (req.issueNumber !== undefined) {
    await env.gh
      .closeIssue({ repo: req.repo, number: req.issueNumber })
      .catch(() => undefined);
  }

  return { kind: "archived", movedQueueFiles: moved };
}
