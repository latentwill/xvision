import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { SEALED_GATE_ACTION_SRC } from "./sealedGateCode";

// Byte-parity guard. The browser sends SEALED_GATE_ACTION_SRC inline as `code`
// to Lit; Lit hashes those bytes to the CID the operator registered in the PKP
// group. If the embedded string drifts from the pinned deploy file by even one
// byte, the computed CID changes and gate authorization breaks. This test reads
// BOTH the generated module's constant and the source-of-truth deploy file and
// asserts they are byte-identical. Regenerate with
// `node frontend/web/scripts/gen-sealed-gate-code.mjs` if this fails.
describe("SEALED_GATE_ACTION_SRC", () => {
  it("is byte-identical to contracts/lit-actions/sealed-gate.deploy.js", () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const deployPath = resolve(
      here,
      "../../../../../../contracts/lit-actions/sealed-gate.deploy.js",
    );
    const onDisk = readFileSync(deployPath, "utf8");
    expect(SEALED_GATE_ACTION_SRC).toBe(onDisk);
  });
});
