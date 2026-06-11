/**
 * Unit tests for the pure validators in sealed-gate.js.
 *
 * Run: `node contracts/lit-actions/sealed-gate.test.mjs`
 * (plain assertions, no test framework). Exits non-zero on first failure.
 *
 * These cover the replay/binding-protection core — the thing that must be
 * correct. The Lit-runtime `main()` (signature recovery, RPC balanceOf,
 * Lit.Actions.Decrypt) is not exercised here; it depends on the Lit globals.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  MIN_NONCE_LEN,
  checkSigner,
  parseMessage,
  validateMessage,
} from "./sealed-gate.js";

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
    throw e; // abort on first failure
  }
}

/** Build a well-formed message for a listing. */
function buildMessage({ listing = 42, nonce = "3f9a1c8e7b2d4056", expiry = 1760000000 } = {}) {
  return [
    "xvision sealed-bundle license request",
    `Listing: ${listing}`,
    `Nonce: ${nonce}`,
    `Expiry: ${expiry}`,
  ].join("\n");
}

const NOW = 1759999000; // a few seconds before the default expiry

test("valid message passes", () => {
  const msg = buildMessage();
  const r = validateMessage(msg, { expectedListingId: 42, nowSec: NOW });
  assert.equal(r.ok, true, JSON.stringify(r));
  assert.equal(r.fields.listingId, "42");
  assert.equal(r.fields.nonce, "3f9a1c8e7b2d4056");
  assert.equal(r.fields.expiry, 1760000000);
});

test("valid message passes with string expectedListingId", () => {
  const r = validateMessage(buildMessage(), { expectedListingId: "42", nowSec: NOW });
  assert.equal(r.ok, true, JSON.stringify(r));
});

test("expired message is rejected", () => {
  const msg = buildMessage({ expiry: 1000 });
  const r = validateMessage(msg, { expectedListingId: 42, nowSec: NOW });
  assert.equal(r.ok, false);
  assert.match(r.error, /expired/);
});

test("expiry exactly equal to now is still valid (not strictly-after)", () => {
  const msg = buildMessage({ expiry: NOW });
  const r = validateMessage(msg, { expectedListingId: 42, nowSec: NOW });
  assert.equal(r.ok, true, JSON.stringify(r));
});

test("wrong listingId is rejected (binding protection)", () => {
  const msg = buildMessage({ listing: 42 });
  const r = validateMessage(msg, { expectedListingId: 99, nowSec: NOW });
  assert.equal(r.ok, false);
  assert.match(r.error, /listingId mismatch/);
});

test("nonce too short is rejected", () => {
  const msg = buildMessage({ nonce: "short" }); // < MIN_NONCE_LEN
  assert.ok("short".length < MIN_NONCE_LEN, "fixture must be below threshold");
  const r = validateMessage(msg, { expectedListingId: 42, nowSec: NOW });
  assert.equal(r.ok, false);
  assert.match(r.error, /nonce/);
});

test("missing nonce is rejected", () => {
  const msg = [
    "xvision sealed-bundle license request",
    "Listing: 42",
    "Expiry: 1760000000",
  ].join("\n");
  const r = validateMessage(msg, { expectedListingId: 42, nowSec: NOW });
  assert.equal(r.ok, false);
  assert.match(r.error, /Nonce/);
});

test("missing Listing field is rejected", () => {
  const msg = [
    "xvision sealed-bundle license request",
    "Nonce: 3f9a1c8e7b2d4056",
    "Expiry: 1760000000",
  ].join("\n");
  const r = validateMessage(msg, { expectedListingId: 42, nowSec: NOW });
  assert.equal(r.ok, false);
  assert.match(r.error, /Listing/);
});

test("non-decimal Listing is rejected", () => {
  const msg = buildMessage({ listing: "0x2a" });
  const r = validateMessage(msg, { expectedListingId: "0x2a", nowSec: NOW });
  assert.equal(r.ok, false);
  assert.match(r.error, /decimal integer/);
});

test("non-numeric Expiry is rejected", () => {
  const msg = [
    "xvision sealed-bundle license request",
    "Listing: 42",
    "Nonce: 3f9a1c8e7b2d4056",
    "Expiry: soon",
  ].join("\n");
  const r = validateMessage(msg, { expectedListingId: 42, nowSec: NOW });
  assert.equal(r.ok, false);
  assert.match(r.error, /Expiry/);
});

test("empty message is rejected", () => {
  const r = validateMessage("", { expectedListingId: 42, nowSec: NOW });
  assert.equal(r.ok, false);
  assert.match(r.error, /empty/);
});

test("matching signer passes (case-insensitive)", () => {
  const r = checkSigner(
    "0xABCdef0000000000000000000000000000000001",
    "0xabcdef0000000000000000000000000000000001",
  );
  assert.equal(r.ok, true, JSON.stringify(r));
});

test("sig-mismatch (recovered != claimed address) is rejected", () => {
  const r = checkSigner(
    "0x1111111111111111111111111111111111111111",
    "0x2222222222222222222222222222222222222222",
  );
  assert.equal(r.ok, false);
  assert.match(r.error, /signature does not match/);
});

test("parseMessage ignores the freeform header line", () => {
  const r = parseMessage(buildMessage());
  assert.equal(r.ok, true);
  assert.equal(r.fields.listingId, "42");
});

test("runtime decrypt call uses Chipotle's exported Decrypt function", () => {
  const source = readFileSync(join(dir, "sealed-gate.js"), "utf8");
  assert.match(source, /Lit\.Actions\.Decrypt\(/);
  assert.doesNotMatch(source, /Lit\.Actions\.decrypt\(/);
});

// --- summary ---------------------------------------------------------------
if (process.exitCode) {
  console.error(`\n${passed} passed, then FAILED`);
} else {
  console.log(`\nall ${passed} tests passed`);
}
