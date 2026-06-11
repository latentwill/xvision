#!/usr/bin/env node
// Builds the plain-script DEPLOY forms Lit's TEE runtime executes, from the
// version-controlled sources. The DEPLOY file's CID is what the operator pins
// to IPFS:
//   sealed-gate.js    -> sealed-gate.deploy.js    (CID = XVN_LIT_GATE_ACTION_CID)
//   sealed-encrypt.js -> sealed-encrypt.deploy.js (CID = XVN_LIT_ENCRYPT_ACTION_CID)
// Sources keep `export` (where present) for the Node test harness; deploy is
// what you pin. `sealed-gate.js` has ESM `export`s that must be stripped;
// `sealed-encrypt.js` has none, so its deploy copy is the source plus header.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const dir = dirname(fileURLToPath(import.meta.url));

/**
 * Emit `<name>.deploy.js` from `<name>.js`, stripping ESM `export ` keywords.
 *
 * `header2` is the exact second header line. It is per-file and load-bearing:
 * the sealed-gate deploy file is ALREADY PINNED (its CID is
 * XVN_LIT_GATE_ACTION_CID), so even one changed byte of header text would
 * change the CID and break the operator's setup. Keep sealed-gate's header2
 * verbatim.
 */
function build(name, header2) {
  const src = readFileSync(join(dir, `${name}.js`), "utf8");
  const deploy =
    `// GENERATED — do not edit. Source: ${name}.js (run build-deploy.mjs).\n` +
    `${header2}\n` +
    src.replace(/^export (function|const) /gm, "$1 ");
  const out = join(dir, `${name}.deploy.js`);
  writeFileSync(out, deploy);
  // sanity: deploy must have no surviving `export`
  if (/^export /m.test(deploy)) {
    console.error(`FAIL: export survived in ${name}.deploy.js`);
    process.exit(1);
  }
  console.log("wrote", out, `(${deploy.length} bytes)`);
}

// sealed-gate header2 is VERBATIM — its CID is pinned (XVN_LIT_GATE_ACTION_CID).
build("sealed-gate", "// This is the plain-script form Lit's TEE runs; its IPFS CID is the gate.");
build("sealed-encrypt", "// This is the plain-script form Lit's TEE runs; its IPFS CID is what you pin.");
