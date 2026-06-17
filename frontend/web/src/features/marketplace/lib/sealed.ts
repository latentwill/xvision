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
import {
  currentAddress,
  getActiveNetworkConfigOrDefault,
  walletClient,
} from "./chain";
import { WalletRequiredError } from "./purchaseErrors";
import { SEALED_GATE_ACTION_SRC } from "./sealedGateCode";

// ---------------------------------------------------------------------------
// Lit config (from /api/marketplace/status)
// ---------------------------------------------------------------------------

/** The `lit` block of `GET /api/marketplace/status` (null when unconfigured). */
export interface LitConfig {
  api_base: string;
  /**
   * Authorization-hash reference for the gate action — the CID the operator
   * registered in the PKP's authorized group. NOT sent on execution: the gate
   * source is sent inline as `code` (see `invokeGateAction`), and Lit hashes
   * those inline bytes to this same CID. Kept for the operator's group binding.
   */
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

/**
 * Response of `GET /api/marketplace/listings/:id/import-challenge` (lane cgz):
 * a fresh, server-issued, single-use, time-bounded nonce plus the EXACT byte
 * string the client must `personal_sign`. The server owns the nonce so it can
 * enforce single-use server-side (the Lit gate is deliberately stateless and
 * cannot detect replays) — the client signs the SERVER's `message`, not a
 * self-minted one, so the same signature proves address control at BOTH the
 * Lit gate and the server's import-sealed proof check.
 */
export interface ImportChallenge {
  nonce: string;
  expiry_unix: number;
  message: string;
}

/**
 * Fetch a server-issued import challenge for `listingId`. The returned
 * `message` embeds the listing id, the server nonce, and an expiry, and is the
 * exact string to `personal_sign`. 404 → the listing is not in the indexed
 * snapshot (surfaced as an ApiError by `apiFetch`).
 */
export async function fetchImportChallenge(
  listingId: string | number,
): Promise<ImportChallenge> {
  return apiFetch<ImportChallenge>(
    `/api/marketplace/listings/${encodeURIComponent(String(listingId))}/import-challenge`,
  );
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
 * (`VITE_LIT_CLIENT_KEY`). MUST stay the literal `import.meta.env.VITE_…`
 * expression (typed via src/vite-env.d.ts): Vite's define replacement only
 * rewrites this exact form — an alias read survives to production bundles as
 * a runtime lookup, where browsers have no `import.meta.env` (the bug that
 * disabled the marketplace subgraph client, fixed in PR #926). Vitest keeps
 * env live and the read happens at call time, so `vi.stubEnv` still works.
 */
export function litClientKey(): string | undefined {
  return import.meta.env.VITE_LIT_CLIENT_KEY;
}

/**
 * Invoke the pinned gate action against the Lit ("Chipotle") API.
 *
 * Matches the Chipotle OpenAPI as of 2026-06-12 (api_direct). There is a SINGLE
 * endpoint for running an action:
 *   POST {api_base}/core/v1/lit_action
 *   header X-Api-Key: <VITE_LIT_CLIENT_KEY>
 *   body   { code: <gate action JS source>, js_params: {<gate params>} }
 * The body accepts EITHER `{ ipfs_id, js_params }` (resolve a cached action by
 * CID — but Lit's CID cache is in-memory / non-durable) OR `{ code, js_params }`
 * (run the JS inline — always works). We ALWAYS send the gate source inline as
 * `code` ([SEALED_GATE_ACTION_SRC], byte-identical to the pinned deploy file).
 * Authorization is unchanged: Lit hashes the inline bytes to the CID and checks
 * the operator's group binding, so `litCfg.gate_action_cid` is still the
 * authorization-hash reference the operator registers — we just don't send it.
 * The response is an envelope `{ response, logs, has_error }` where `response`
 * is the gate action's return value — delivered EITHER as a JSON object
 * (`{plaintext}` / `{error}`) OR as a JSON STRING that itself parses to that
 * object (the `setResponse({response: JSON.stringify(...)})` pattern the gate
 * uses). Both forms are handled. `has_error: true` is a hard failure.
 *
 * Everything Lit-shape-specific is intentionally isolated in this one function.
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

  const url = `${litCfg.api_base.replace(/\/+$/, "")}/core/v1/lit_action`;
  const res = await fetch(url, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-api-key": key,
    },
    body: JSON.stringify({
      // Send the gate action source INLINE as `code` (not by
      // `litCfg.gate_action_cid` / `ipfs_id`): Lit's CID-keyed cache is
      // non-durable, so inline `code` is the only reliable path. The bytes are
      // byte-identical to the pinned deploy file, so the CID Lit computes over
      // them matches the operator's registered group binding (authorization
      // unchanged).
      code: SEALED_GATE_ACTION_SRC,
      js_params: jsParams,
    }),
  });

  if (!res.ok) {
    throw new SealedGateError(
      `Lit gate action HTTP ${res.status}: ${await safeText(res)}`,
    );
  }

  // Envelope: `{ response, logs, has_error }`. `has_error` is a hard failure
  // regardless of `response`; otherwise `response` is the action's return
  // value, as an object OR a JSON string — normalizeGatePayload handles both.
  const raw = (await res.json()) as
    | { has_error?: boolean; response?: unknown }
    | undefined;
  if (raw?.has_error) {
    throw new SealedGateError(
      `Lit gate action reported has_error=true: ${gateErrorDetail(raw.response)}`,
    );
  }
  const payload = normalizeGatePayload(raw?.response);
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

/**
 * Flatten the Chipotle envelope's `response` value to `{plaintext?, error?}`.
 *
 * Matches the Chipotle OpenAPI as of 2026-06-12 (api_direct): `response` is the
 * action's return value, delivered EITHER as an object (`{plaintext}` /
 * `{error}`) OR as a JSON STRING that itself parses to that object (the
 * `setResponse({response: JSON.stringify(...)})` pattern). Both are handled.
 */
function normalizeGatePayload(
  response: unknown,
): { plaintext?: string; error?: string } {
  if (response === undefined || response === null) {
    return { error: "Lit gate action returned an empty response." };
  }
  // JSON-string form: response is a string holding the JSON payload.
  if (typeof response === "string") {
    try {
      return JSON.parse(response) as { plaintext?: string; error?: string };
    } catch {
      return { error: "Lit gate action returned an unparseable response." };
    }
  }
  // Object form: response is already the payload object.
  if (typeof response === "object") {
    return response as { plaintext?: string; error?: string };
  }
  return { error: "Lit gate action returned an unexpected response." };
}

/** Best-effort string detail for a `has_error` envelope (object or string). */
function gateErrorDetail(response: unknown): string {
  const payload = normalizeGatePayload(response);
  if (payload.error) return payload.error;
  return typeof response === "string" ? response : JSON.stringify(response);
}

// ---------------------------------------------------------------------------
// Decrypt orchestration
// ---------------------------------------------------------------------------

/**
 * The decrypted manifest PLUS the proof-of-address the caller must replay to
 * the server's import-sealed route (lane cgz). `message`/`signature` are the
 * SERVER-issued challenge string and its `personal_sign`; the server recovers
 * the signer, requires it to equal the wallet address, validates the message
 * binding/freshness, and consumes the nonce single-use.
 */
export interface DecryptedSealedBundle {
  manifest: Record<string, unknown>;
  /** The server-issued challenge message that was signed. */
  message: string;
  /** EIP-191 `personal_sign` of `message` by the connected wallet. */
  signature: string;
}

/**
 * Decrypt a sealed bundle's ciphertext into its plaintext manifest object AND
 * return the proof-of-address (server-issued `message` + `signature`) so the
 * caller can replay it to import-sealed (lane cgz).
 *
 * Flow:
 *   1. Fetch Lit + contracts config from /api/marketplace/status.
 *      (throws SealedNotConfiguredError when `lit` is null.)
 *   2. Resolve the connected wallet (throws WalletRequiredError if none).
 *   3. Fetch a SERVER-issued challenge (`GET …/import-challenge`): a fresh,
 *      single-use, time-bounded nonce + the exact message to sign. Signing the
 *      server's string (not a self-minted nonce) is what lets the server
 *      enforce single-use replay defense — the Lit gate stays stateless.
 *   4. personal_sign the server's message; invoke the pinned gate action with
 *      the ciphertext + signature.
 *   5. JSON.parse the returned plaintext; sanity-check it's an object.
 *      (Integrity authority is the server's import-sealed 409 hash recheck —
 *      see the module header. We do NOT recompute the canonical hash here.)
 */
export async function decryptSealedBundle(params: {
  listingId: string | number;
  ciphertext: string;
}): Promise<DecryptedSealedBundle> {
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

  // Server-issued single-use nonce challenge (lane cgz). The server owns the
  // nonce; we sign its canonical `message` so the SAME signature proves address
  // control at the Lit gate AND at the server's import-sealed proof check.
  const challenge = await fetchImportChallenge(listingId);
  const message = challenge.message;

  const signature = await walletClient().signMessage({ account: address, message });

  // The Lit gate action checks the caller's ERC-1155 license balance on-chain,
  // so it must hit the BACKEND-selected chain's RPC — not the build-time default
  // (a prebuilt sepolia bundle on a mainnet backend would query the wrong chain
  // and fail every license check). Lenient: a status outage falls back to the
  // build-time default rather than blocking decrypt.
  const net = await getActiveNetworkConfigOrDefault();

  const { plaintext } = await invokeGateAction(litCfg, {
    pkpId: litCfg.pkp_id,
    ciphertext,
    address,
    message,
    signature,
    listingId: String(listingId),
    nftAddress,
    rpcUrl: net.chain.rpcUrls.default.http[0],
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
  return { manifest: manifest as Record<string, unknown>, message, signature };
}
