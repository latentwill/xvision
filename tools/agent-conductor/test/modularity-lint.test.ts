import { describe, expect, it } from "vitest";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const LINT = resolve(HERE, "..", "lint", "no-host-repo-references.mjs");

function runLint(dir: string): { code: number; stdout: string; stderr: string } {
  const r = spawnSync("node", [LINT, dir], { encoding: "utf8" });
  return { code: r.status ?? -1, stdout: r.stdout, stderr: r.stderr };
}

describe("modularity boundary lint", () => {
  it("passes the actual src/ tree", () => {
    const srcDir = resolve(HERE, "..", "src");
    const r = runLint(srcDir);
    expect(r.code, r.stderr).toBe(0);
    expect(r.stdout).toContain("clean");
  });

  it("flags any host-repo identifier in a fixture", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-lint-"));
    await mkdir(join(dir, "src"), { recursive: true });
    await writeFile(
      join(dir, "src", "bad.ts"),
      "export const x = 'xvision';\nexport const y = 'latentwill/something';\n",
    );
    const r = runLint(join(dir, "src"));
    expect(r.code).toBe(1);
    expect(r.stderr).toContain('forbidden token "xvision"');
    expect(r.stderr).toContain('forbidden token "latentwill"');
  });
});
