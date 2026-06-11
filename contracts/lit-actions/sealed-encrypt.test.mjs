/**
 * Validity check for the sealed-encrypt action + its deploy build.
 *
 * Run: `node contracts/lit-actions/sealed-encrypt.test.mjs`
 * (plain assertions, no test framework). Exits non-zero on first failure.
 *
 * The encrypt action is tiny and Lit-runtime-only (it just calls
 * `Lit.Actions.Encrypt`), so there are no pure validators to unit-test.
 * Instead we assert:
 *   1. Importing the SOURCE in Node does NOT run the action (the `typeof Lit`
 *      guard holds) — i.e. importing is side-effect-free.
 *   2. The DEPLOY file exists, is byte-valid JS (`node --check`), and still
 *      carries the guard so it won't crash if loaded outside the TEE.
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

test("importing the source is side-effect-free (Lit guard holds)", async () => {
  // If the `typeof Lit` guard were missing, this import would throw a
  // ReferenceError on `Lit`/`pkpId`. A clean import proves the guard holds.
  await import("./sealed-encrypt.js");
});

test("deploy file passes node --check (valid JS)", () => {
  const deployPath = join(dir, "sealed-encrypt.deploy.js");
  // throws (non-zero exit) if the deploy file is not valid JavaScript
  execFileSync(process.execPath, ["--check", deployPath]);
});

test("deploy file keeps the typeof-Lit runtime guard", () => {
  const deploy = readFileSync(join(dir, "sealed-encrypt.deploy.js"), "utf8");
  assert.match(deploy, /typeof Lit !== "undefined"/);
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
