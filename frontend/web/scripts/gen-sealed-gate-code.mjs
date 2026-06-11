// Regenerates frontend/web/src/features/marketplace/lib/sealedGateCode.ts from
// the pinned Lit gate action deploy file. The browser sends the gate action
// source INLINE as `code` on every /core/v1/lit_action call (Lit's CID-keyed
// cache is non-durable), so we embed the exact bytes here. The bytes MUST stay
// byte-identical to the deploy file — Lit hashes them to the CID the operator
// registered. sealedGateCode.test.ts asserts parity so drift is caught in CI.
//
//   node frontend/web/scripts/gen-sealed-gate-code.mjs
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const deployPath = resolve(
  here,
  "../../../contracts/lit-actions/sealed-gate.deploy.js",
);
const outPath = resolve(
  here,
  "../src/features/marketplace/lib/sealedGateCode.ts",
);

const src = readFileSync(deployPath, "utf8");

const out = `// GENERATED — do not edit by hand.
// Source of truth: contracts/lit-actions/sealed-gate.deploy.js
// Regenerate with: node frontend/web/scripts/gen-sealed-gate-code.mjs
//
// The Lit ("Chipotle") gate action JS source, embedded as a string so the
// browser can send it INLINE as \`code\` on every /core/v1/lit_action call.
// Lit's CID-keyed action cache is non-durable, so an \`ipfs_id\` reference is not
// reliable; inline \`code\` always works (Lit caches it by hash). These bytes
// MUST stay byte-identical to the pinned deploy file — Lit hashes them to the
// CID the operator registered in the PKP group, which is what keeps gate
// authorization intact. sealedGateCode.test.ts enforces byte parity.
export const SEALED_GATE_ACTION_SRC = ${JSON.stringify(src)};
`;

writeFileSync(outPath, out);
console.log(`wrote ${outPath} (${src.length} bytes of gate source)`);
