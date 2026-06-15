// finalizeImport.ts — bounded retry around the post-purchase sealed import.
//
// After a paid purchase relay resolves, the license is minted on-chain (the
// `/api/marketplace/buy` relay awaits `get_receipt()` before returning). The
// import-sealed route then license-gates on `balanceOf(address, listing_id)`,
// which can briefly read 403 if the RPC node serving the gate hasn't yet seen
// the freshly-mined license (read-replica / indexer lag). This helper retries
// the import ONLY on that license-not-yet-visible condition, surfacing any
// other error (or a final 403) to the caller for the inline buy-error strip.
import { ApiError } from "@/api/client";
import { SealedGateError } from "./sealed";

/**
 * True when the error is the transient "license not yet visible" condition:
 * an HTTP 403 from the import route, or a Lit gate rejection whose message
 * names the missing license. Everything else (409 hash mismatch, wallet
 * missing, network, malformed) is terminal and must NOT be retried.
 */
export function isLicenseNotYetVisible(e: unknown): boolean {
  if (e instanceof ApiError) return e.status === 403;
  if (e instanceof SealedGateError) return /license/i.test(e.message);
  return false;
}

export interface FinalizeOptions {
  /** Max attempts (including the first). Default 5. */
  attempts?: number;
  /** Delay between attempts in ms. Default 1500. */
  delayMs?: number;
  /** Injectable sleep (tests pass a no-op / fake-timer driver). */
  sleep?: (ms: number) => Promise<void>;
}

const defaultSleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/**
 * Run `importFn` with a bounded retry on the license-not-yet-visible condition.
 * Resolves with `{ agent_id }` on the first success; rejects with the last
 * error once attempts are exhausted or a non-retryable error is thrown.
 */
export async function finalizeImportWithRetry(
  importFn: () => Promise<{ agent_id: string }>,
  opts: FinalizeOptions = {},
): Promise<{ agent_id: string }> {
  const attempts = opts.attempts ?? 5;
  const delayMs = opts.delayMs ?? 1500;
  const sleep = opts.sleep ?? defaultSleep;

  let lastError: unknown;
  for (let attempt = 0; attempt < attempts; attempt++) {
    try {
      return await importFn();
    } catch (e) {
      lastError = e;
      // Only the license-not-yet-visible condition is retryable, and only when
      // we have attempts left.
      if (!isLicenseNotYetVisible(e) || attempt === attempts - 1) {
        throw e;
      }
      await sleep(delayMs);
    }
  }
  // Unreachable (the loop always returns or throws), but satisfies the type.
  throw lastError;
}
