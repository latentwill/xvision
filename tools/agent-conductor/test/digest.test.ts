import { describe, expect, it } from "vitest";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { appendDigest, digestPath, tailDigest } from "../src/daemon/digest.js";

describe("daemon digest", () => {
  it("appends timestamped lines to today's file", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-dig-"));
    const when = new Date("2026-05-18T08:00:00Z");
    await appendDigest(dir, "transitioned demo READY → CLAIMED", when);
    await appendDigest(dir, "stuck: foo READY 26h", when);

    const path = digestPath(dir, when);
    expect(path).toContain("digest-2026-05-18.md");

    const text = await readFile(path, "utf8");
    expect(text.split("\n").filter(Boolean).length).toBe(2);
    expect(text).toContain("transitioned demo READY → CLAIMED");

    const tail = await tailDigest(dir, 1, when);
    expect(tail).toHaveLength(1);
    expect(tail[0]).toContain("stuck: foo");

    await rm(dir, { recursive: true });
  });

  it("tail falls back to the most recent date when today is missing", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ac-dig-"));
    const yesterday = new Date("2026-05-17T12:00:00Z");
    await appendDigest(dir, "old entry", yesterday);
    const today = new Date("2026-05-18T00:00:00Z");
    const tail = await tailDigest(dir, 5, today);
    expect(tail.some((l) => l.includes("old entry"))).toBe(true);
    await rm(dir, { recursive: true });
  });
});
