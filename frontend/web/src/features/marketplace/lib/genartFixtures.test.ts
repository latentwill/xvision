// genartFixtures.test.ts — golden parity contract with the Rust twin.
// Regenerate with: REGEN_GENART_FIXTURES=1 npm test -- genartFixtures
import { describe, expect, it } from "vitest";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { deriveTraits, generateSvg, generateTokenUri } from "./genart";

const __dirname = dirname(fileURLToPath(import.meta.url));

const FIXTURE = resolve(
  __dirname,
  "../../../../../../crates/xvision-identity/tests/fixtures/genart_v3.json",
);

const SEEDS: Array<[string, string]> = Array.from({ length: 24 }, (_, i) => [
  `01HXVNFIX${i.toString(36).toUpperCase().padStart(2, "0")}`,
  // deterministic synthetic manifest hashes (any 64-hex is valid input)
  (BigInt(i + 1) * 0x9e3779b97f4a7c15n).toString(16).padStart(64, "0").slice(0, 64),
]);

describe("genart v3 golden fixtures", () => {
  it("matches (or regenerates) the fixture file", () => {
    const computed = SEEDS.map(([agentId, manifestHash]) => ({
      agent_id: agentId,
      manifest_hash: manifestHash,
      traits: deriveTraits(agentId, manifestHash),
      svg: generateSvg(agentId, manifestHash),
      token_uri: generateTokenUri(agentId, manifestHash),
    }));
    if (process.env.REGEN_GENART_FIXTURES) {
      writeFileSync(FIXTURE, JSON.stringify(computed, null, 2) + "\n");
      return;
    }
    const golden = JSON.parse(readFileSync(FIXTURE, "utf8"));
    expect(computed).toEqual(golden);
  });
});
