// useDeployDegenArena — thin POST wrapper for the Degen Arena deploy route.
//
// Wraps  POST /api/live/deploy/degen-arena
// Body:  { apiKey: string, accountAddress: string, network: "testnet"|"mainnet" }
// 200:   { ok: true }
// 4xx/5xx: throws with the server's `message` field (or a fallback string).
//
// Deliberately NOT using `apiFetch` (which logs body summaries) to avoid
// accidentally surfacing the private key in traces. Uses a raw `fetch` POST
// so the body is never passed through the shared logging helper.
//
// Usage (page):
//   const deploy = useDeployDegenArena();
//   <DegenDeployStrip onDeploy={(p) => deploy(p)} />

import type { DegenDeployPayload } from "./DegenDeployStrip";

export type DeployDegenArenaResult = { ok: true };

/** Thrown when the server responds with a non-2xx status. */
export class DeployDegenArenaError extends Error {
  readonly status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = "DeployDegenArenaError";
  }
}

/**
 * Returns a function that POSTs to /api/live/deploy/degen-arena.
 * Throws `DeployDegenArenaError` on non-2xx; resolves with `{ ok: true }` otherwise.
 *
 * The private key travels in the request body over HTTPS and is NEVER
 * rendered to the DOM, logged, or inserted into any React state visible
 * to the component tree.
 */
export function useDeployDegenArena() {
  return async function deployDegenArena(
    payload: DegenDeployPayload,
  ): Promise<DeployDegenArenaResult> {
    const res = await fetch("/api/live/deploy/degen-arena", {
      method: "POST",
      headers: { "content-type": "application/json" },
      // Keys stay server-side only: the body is never echoed, stored in
      // component state, or rendered to the DOM after this point.
      body: JSON.stringify({
        apiKey: payload.apiKey,
        accountAddress: payload.accountAddress,
        network: payload.network,
      }),
    });

    if (!res.ok) {
      let message = `HTTP ${res.status}`;
      try {
        const body = (await res.json()) as { message?: string };
        if (typeof body.message === "string" && body.message.length > 0) {
          message = body.message;
        }
      } catch {
        // Non-JSON error body — keep the HTTP-status fallback message.
      }
      throw new DeployDegenArenaError(res.status, message);
    }

    return { ok: true };
  };
}
