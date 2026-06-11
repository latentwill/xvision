#!/usr/bin/env node
// Strips ESM `export ` keywords from sealed-gate.js to produce the plain-script
// form Lit's TEE runtime executes. The DEPLOY file's CID is XVN_LIT_GATE_ACTION_CID.
// Source keeps `export` for the Node test harness; deploy is what you pin to IPFS.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
const dir = dirname(fileURLToPath(import.meta.url));
const src = readFileSync(join(dir, "sealed-gate.js"), "utf8");
const deploy =
  "// GENERATED — do not edit. Source: sealed-gate.js (run build-deploy.mjs).\n" +
  "// This is the plain-script form Lit's TEE runs; its IPFS CID is the gate.\n" +
  src.replace(/^export (function|const) /gm, "$1 ");
const out = join(dir, "sealed-gate.deploy.js");
writeFileSync(out, deploy);
// sanity: deploy must have no `export`
if (/^export /m.test(deploy)) { console.error("FAIL: export survived"); process.exit(1); }
console.log("wrote", out, `(${deploy.length} bytes)`);
