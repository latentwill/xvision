// Append-only daemon digest: one file per UTC date under <cacheDir>.
// Phase-1 entries describe transitions executed, transitions deferred
// (with reason), and stuck tasks. Rotation is by filename.

import { appendFile, mkdir, readFile, readdir } from "node:fs/promises";
import { join } from "node:path";

function ymd(d: Date): string {
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, "0");
  const day = String(d.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function digestPath(cacheDir: string, when: Date = new Date()): string {
  return join(cacheDir, `digest-${ymd(when)}.md`);
}

export async function appendDigest(
  cacheDir: string,
  line: string,
  when: Date = new Date(),
): Promise<void> {
  await mkdir(cacheDir, { recursive: true });
  const ts = when.toISOString();
  await appendFile(digestPath(cacheDir, when), `- \`${ts}\` ${line}\n`, "utf8");
}

export async function tailDigest(
  cacheDir: string,
  n: number,
  when: Date = new Date(),
): Promise<string[]> {
  // Try today first; if absent, fall back to most recent existing digest.
  let path = digestPath(cacheDir, when);
  let raw: string;
  try {
    raw = await readFile(path, "utf8");
  } catch {
    try {
      const files = (await readdir(cacheDir))
        .filter((f) => f.startsWith("digest-") && f.endsWith(".md"))
        .sort();
      const latest = files[files.length - 1];
      if (!latest) return [];
      path = join(cacheDir, latest);
      raw = await readFile(path, "utf8");
    } catch {
      return [];
    }
  }
  const lines = raw.split(/\r?\n/).filter((l) => l.length > 0);
  return lines.slice(-n);
}
