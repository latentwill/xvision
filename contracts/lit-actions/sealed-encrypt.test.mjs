/**
 * Validity check for the sealed-encrypt action + its deploy build.
 *
 * Run: `node contracts/lit-actions/sealed-encrypt.test.mjs`
 * (plain assertions, no test framework). Exits non-zero on first failure.
 *
 * The encrypt action is tiny and Lit-runtime-only (it just calls
 * `Lit.Actions.Encrypt`), so there are no pure validators to unit-test.
 * Instead we assert:
 *   1. Importing the SOURCE in Node does NOT run the action — Chipotle
 *      auto-invokes `main(js_params)`, so the file must contain NO top-level
 *      invocation and importing it is side-effect-free by construction.
 *   2. The DEPLOY file exists, is byte-valid JS (`node --check`), defines
 *      `main`, and has no top-level self-invocation (the Datil bare-globals
 *      and invented-`jsParams` patterns both threw ReferenceError live).
 */

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const dir = dirname(fileURLToPath(import.meta.url));
let passed = 0;
function test(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`ok   - ${name}`);
  } catch (e) {
    console.error(`FAIL - ${name}`);
    console.error(`       ${e && e.message ? e.message : e}`);
    process.exitCode = 1;
    throw e;
  }
}

test("importing the source is side-effect-free (no top-level invocation)", async () => {
  // If a top-level self-invoke crept back in, this import would throw a
  // ReferenceError on `Lit`/`jsParams`. A clean import proves main is only
  // defined, never called — Chipotle's runtime is what invokes it.
  await import("./sealed-encrypt.js");
});

test("deploy file passes node --check (valid JS)", () => {
  const deployPath = join(dir, "sealed-encrypt.deploy.js");
  // throws (non-zero exit) if the deploy file is not valid JavaScript
  execFileSync(process.execPath, ["--check", deployPath]);
});

test("deploy file defines main with no top-level self-invocation", () => {
  const deploy = readFileSync(join(dir, "sealed-encrypt.deploy.js"), "utf8");
  // Chipotle auto-invokes main(js_params); any self-invoke pattern is a
  // live ReferenceError (bare globals AND a `jsParams` global both failed).
  assert.match(deploy, /async function main\(\{ pkpId, message \}\)/);
  assert.doesNotMatch(deploy, /typeof Lit !== "undefined"/);
  assert.doesNotMatch(deploy, /=\s*jsParams|jsParams\s*[.;)]/); // usage, not prose
  assert.doesNotMatch(deploy, /^\s*main\(/m);
  assert.match(deploy, /Lit\.Actions\.Encrypt/);
  // generated header present, and no stray ESM export survived
  assert.match(deploy, /^\/\/ GENERATED/);
  assert.doesNotMatch(deploy, /^export /m);
});

if (process.exitCode) {
  console.error(`\n${passed} passed, then FAILED`);
} else {
  console.log(`\nall ${passed} tests passed`);
}
