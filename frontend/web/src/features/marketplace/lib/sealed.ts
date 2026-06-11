// sealed.ts — sealed-tier (Lit Protocol) bundle decryption for the marketplace.
//
// A SEALED listing's manifest is encrypted; the ciphertext is served by the
// backend, but the only way to decrypt it is to satisfy the Lit gate action
// pinned at `contracts/lit-actions/sealed-gate.js`. That gate enforces, in
// order: (1) the caller's `personal_sign` signature recovers `address`,
// (2) the signed message names this listing and is unexpired, (3) the caller
// holds the ERC-1155 license NFT, and only then (4) decrypts.
//
// This module builds the EXACT signed message the gate parses (byte-compatible
// with `validateMessage` in sealed-gate.js — a newline-delimited "Key: value"
// string), drives the wallet signature, and invokes the gate action through
// the Lit ("Chipotle") API.
//
// INTEGRITY AUTHORITY: the server is the integrity authority. The on-chain
// `content_hash` is re-checked server-side by the import-sealed route (which
// returns 409 on mismatch against the canonical Rust `canonical_json`). We do
// NOT recompute the canonical hash in the browser — JS/Rust canonicalization
// parity is fragile, and duplicating it here would be a second, drift-prone
// source of truth. The browser only JSON.parses the plaintext and sanity-checks
// it decoded to an object; the authoritative hash gate is the 409 on import.

import { apiFetch } from "@/api/client";
import { currentAddress, mantleSepolia, walletClient } from "./chain";
import { WalletRequiredError } from "./purchaseErrors";

// ---------------------------------------------------------------------------
// Lit config (from /api/marketplace/status)
// ---------------------------------------------------------------------------

/** The `lit` block of `GET /api/marketplace/status` (null when unconfigured). */
export interface LitConfig {
  api_base: string;
  gate_action_cid: string;
  pkp_id: string;
}

interface SealedStatusOut {
  lit: LitConfig | null;
  contracts: {
    license_token: string | null;
    [k: string]: string | null;
  };
}

/** Thrown when the backend has no Lit gate configured for sealed decryption. */
export class SealedNotConfiguredError extends Error {
  constructor(message = "Sealed decryption is not configured on the backend.") {
    super(message);
    this.name = "SealedNotConfiguredError";
  }
}

/** Thrown when the gate action rejects (no license, expired, bad sig, etc.). */
export class SealedGateError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SealedGateError";
  }
}

// ---------------------------------------------------------------------------
// Signed message (byte-compatible with sealed-gate.js validateMessage)
// ---------------------------------------------------------------------------

/**
 * Build the EXACT newline-delimited license message the Lit gate action's
 * `validateMessage` parses. This is a HARD PARITY REQUIREMENT — the key
 * labels, order, and the header line must match `contracts/lit-actions/
 * sealed-gate.js` (see its header doc block and `sealed-gate.test.mjs`'s
 * `buildMessage`). The gate parses `Key: value` lines case-insensitively, but
 * we reproduce the canonical form exactly:
 *
 *     xvision sealed-bundle license request
 *     Listing: <listingId>
 *     Nonce: <nonce>
 *     Expiry: <expirySec>
 *
 * Pure — no I/O, no globals. Unit-tested for byte parity.
 */
export function buildSealedMessage(params: {
  listingId: string | number;
  nonce: string;
  expirySec: number;
}): string {
  const { listingId, nonce, expirySec } = params;
  return [
    "xvision sealed-bundle license request",
    `Listing: ${listingId}`,
    `Nonce: ${nonce}`,
    `Expiry: ${expirySec}`,
  ].join("\n");
}

/** 32-byte random hex nonce (no 0x prefix) for the signed message. */
export function randomSealedNonce(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

// ---------------------------------------------------------------------------
// Lit gate-action invocation (UNVERIFIED SHAPE — isolated here on purpose)
// ---------------------------------------------------------------------------

/** Parameters passed to the gate action as Lit `jsParams`. */
export interface GateJsParams {
  pkpId: string;
  ciphertext: string;
  address: string;
  message: string;
  signature: string;
  listingId: string;
  nftAddress: string;
  rpcUrl: string;
}

/**
 * Read the scoped Lit client key the operator sets at build time
 * (`VITE_LIT_CLIENT_KEY`). Read dynamically (not destructured) so test setups
 * can flip it with `vi.stubEnv` between cases.
 */
export function litClientKey(): string | undefined {
  const meta = import.meta as unknown as {
    env?: Record<string, string | undefined>;
  };
  return meta.env?.VITE_LIT_CLIENT_KEY;
}

/**
 * Invoke the pinned gate action against the Lit ("Chipotle") API.
 *
 * !!! VERIFY against the Chipotle OpenAPI before live use. !!!
 * The exact run-action endpoint path, request body shape, auth header, and
 * the `{plaintext} | {error}` response envelope below are an EDUCATED GUESS
 * from the Lit v3 conventions; they are NOT confirmed against a live Chipotle
 * deployment. Everything Lit-shape-specific is intentionally isolated in this
 * one function so a single edit fixes it once the OpenAPI is confirmed.
 *
 * Returns the raw `{ plaintext }` (or throws SealedGateError on `{ error }`).
 */
export async function invokeGateAction(
  litCfg: LitConfig,
  jsParams: GateJsParams,
): Promise<{ plaintext: string }> {
  const key = litClientKey();
  if (!key) {
    throw new SealedNotConfiguredError(
      "Sealed decryption is not configured (VITE_LIT_CLIENT_KEY unset).",
    );
  }

  // UNVERIFIED: endpoint + body shape. See the warning above.
  const url = `${litCfg.api_base.replace(/\/+$/, "")}/run-action`;
  const res = await fetch(url, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      authorization: `Bearer ${key}`,
    },
    body: JSON.stringify({
      ipfsId: litCfg.gate_action_cid,
      jsParams,
    }),
  });

  if (!res.ok) {
    throw new SealedGateError(
      `Lit gate action HTTP ${res.status}: ${await safeText(res)}`,
    );
  }

  // UNVERIFIED: response envelope. Chipotle may wrap the action's setResponse
  // payload under a `response` field; we accept either a top-level
  // `{plaintext|error}` or a nested `{response:{plaintext|error}}`.
  const raw = (await res.json()) as
    | { plaintext?: string; error?: string; response?: unknown }
    | undefined;
  const payload = normalizeGatePayload(raw);
  if (payload.error) throw new SealedGateError(payload.error);
  if (typeof payload.plaintext !== "string") {
    throw new SealedGateError("Lit gate action returned no plaintext.");
  }
  return { plaintext: payload.plaintext };
}

async function safeText(res: Response): Promise<string> {
  try {
    return await res.text();
  } catch {
    return "<no body>";
  }
}

/** Flatten the (unverified) Chipotle envelope to `{plaintext?, error?}`. */
function normalizeGatePayload(
  raw: { plaintext?: string; error?: string; response?: unknown } | undefined,
): { plaintext?: string; error?: string } {
  if (!raw) return { error: "Lit gate action returned an empty response." };
  // Nested `response` may itself be a JSON string (action setResponse pattern).
  if (raw.plaintext === undefined && raw.error === undefined && raw.response !== undefined) {
    const r = raw.response;
    if (typeof r === "string") {
      try {
        return JSON.parse(r) as { plaintext?: string; error?: string };
      } catch {
        return { error: "Lit gate action returned an unparseable response." };
      }
    }
    if (r && typeof r === "object") {
      return r as { plaintext?: string; error?: string };
    }
  }
  return { plaintext: raw.plaintext, error: raw.error };
}

// ---------------------------------------------------------------------------
// Decrypt orchestration
// ---------------------------------------------------------------------------

/**
 * Decrypt a sealed bundle's ciphertext into its plaintext manifest object.
 *
 * Flow:
 *   1. Fetch Lit + contracts config from /api/marketplace/status.
 *      (throws SealedNotConfiguredError when `lit` is null.)
 *   2. Resolve the connected wallet (throws WalletRequiredError if none).
 *   3. Build a fresh, listing-bound, 10-minute message; personal_sign it.
 *   4. Invoke the pinned gate action with the ciphertext + signature.
 *   5. JSON.parse the returned plaintext; sanity-check it's an object.
 *      (Integrity authority is the server's import-sealed 409 hash recheck —
 *      see the module header. We do NOT recompute the canonical hash here.)
 */
export async function decryptSealedBundle(params: {
  listingId: string | number;
  ciphertext: string;
}): Promise<Record<string, unknown>> {
  const { listingId, ciphertext } = params;

  const status = await apiFetch<SealedStatusOut>("/api/marketplace/status");
  const litCfg = status.lit;
  if (!litCfg) throw new SealedNotConfiguredError();
  const nftAddress = status.contracts.license_token;
  if (!nftAddress) {
    throw new SealedNotConfiguredError(
      "Sealed decryption is not configured (license token address missing).",
    );
  }

  const address = await currentAddress();
  if (!address) throw new WalletRequiredError();

  const nonce = randomSealedNonce();
  const expirySec = Math.floor(Date.now() / 1000) + 600;
  const message = buildSealedMessage({ listingId, nonce, expirySec });

  const signature = await walletClient().signMessage({ account: address, message });

  const { plaintext } = await invokeGateAction(litCfg, {
    pkpId: litCfg.pkp_id,
    ciphertext,
    address,
    message,
    signature,
    listingId: String(listingId),
    nftAddress,
    rpcUrl: mantleSepolia.rpcUrls.default.http[0],
  });

  let manifest: unknown;
  try {
    manifest = JSON.parse(plaintext);
  } catch {
    throw new SealedGateError("Decrypted bundle is not valid JSON.");
  }
  if (manifest === null || typeof manifest !== "object" || Array.isArray(manifest)) {
    throw new SealedGateError("Decrypted bundle is not a manifest object.");
  }
  return manifest as Record<string, unknown>;
}
