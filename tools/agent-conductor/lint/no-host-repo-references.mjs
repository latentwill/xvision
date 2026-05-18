#!/usr/bin/env node
// Modularity boundary lint.
//
// src/ must be host-repo agnostic. Daemon code reads identifiers, paths,
// and project coordinates from the loaded config — never from string
// literals. This greps for a denylist of host-repo terms and exits
// non-zero on any hit.
//
// Usage:  node lint/no-host-repo-references.mjs src/

import { readdir, readFile, stat } from "node:fs/promises";
import { join, resolve } from "node:path";

const DENYLIST = [
  "xvision",
  "latentwill",
  ".worktrees/",
  "team/",
  "agent/", // hardcoded branch prefix
  "project number 7",
  "project=7",
  "projects/7",
  "ghcr.io/latentwill",
];

async function walk(dir) {
  const out = [];
  let entries = [];
  try {
    entries = await readdir(dir);
  } catch {
    return out;
  }
  for (const name of entries) {
    const p = join(dir, name);
    const s = await stat(p);
    if (s.isDirectory()) {
      out.push(...(await walk(p)));
    } else if (s.isFile() && (p.endsWith(".ts") || p.endsWith(".js") || p.endsWith(".mjs"))) {
      out.push(p);
    }
  }
  return out;
}

async function main() {
  const targets = process.argv.slice(2);
  if (targets.length === 0) {
    process.stderr.write("usage: no-host-repo-references.mjs <dir...>\n");
    process.exit(2);
  }

  const violations = [];
  for (const t of targets) {
    const files = await walk(resolve(t));
    for (const f of files) {
      const text = await readFile(f, "utf8");
      const lines = text.split(/\r?\n/);
      lines.forEach((line, i) => {
        for (const term of DENYLIST) {
          if (line.includes(term)) {
            violations.push({ file: f, lineNo: i + 1, term, line });
          }
        }
      });
    }
  }

  if (violations.length === 0) {
    process.stdout.write("no-host-repo-references: clean\n");
    return;
  }

  for (const v of violations) {
    process.stderr.write(
      `${v.file}:${v.lineNo}: forbidden token "${v.term}"\n    ${v.line.trim()}\n`,
    );
  }
  process.stderr.write(
    `\n${violations.length} violation(s). src/ must read host-repo values from config, not hardcode them.\n`,
  );
  process.exit(1);
}

await main();
