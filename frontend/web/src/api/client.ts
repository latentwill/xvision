// Lightweight typed fetch wrapper. Talks to the dashboard's `/api/*` surface
// (proxied via Vite in dev, same-origin in prod via the embedded SPA).
//
// Error shape mirrors `DashboardError`'s JSON body:
//   { "code": "not_found" | "validation" | "conflict" | "internal",
//     "message": string,
//     "field"?: string }   // present only on `validation`
//
// On non-2xx the helper throws `ApiError` with the parsed code + message
// (plus `field` when the body carried one).

import {
  bodySummary,
  createTrace,
  durationSince,
  errorSummary,
  safeId,
  safePath,
} from "@/lib/logger";
import { getSessionToken } from "@/stores/auth";

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  /// Optional structured field name accompanying a `validation` error.
  /// Backend emits this as a sibling of `message` so the message itself
  /// stays operator-readable (no `"field: msg"` prefix). Undefined for
  /// non-validation responses.
  readonly field?: string;

  constructor(status: number, code: string, message: string, field?: string) {
    super(message);
    this.status = status;
    this.code = code;
    this.field = field;
    this.name = "ApiError";
  }
}

type ApiErrorShape = {
  code?: string;
  message?: string;
  field?: string;
  // The session-auth middleware uses `{"error": "unauthenticated"}` (key "error")
  // rather than the DashboardError shape (key "code"). We accept both.
  error?: string;
};

export async function apiFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const method = init?.method ?? "GET";
  const trace = createTrace("api", {
    request_id: safeId(),
    method,
    path: safePath(path),
    ...bodySummary(init?.body),
  });
  const started = performance.now();
  trace.debug("api.request.start");

  // Attach the session token on mutating requests (POST/PUT/PATCH/DELETE).
  // GET requests are exempted — read-only routes are open to all clients.
  // The loopback-exemption in the server middleware means this header is a
  // no-op for local deployments; it only matters on remote/non-loopback setups.
  const MUTATING_METHODS = ["POST", "PUT", "PATCH", "DELETE"];
  const authHeaders: Record<string, string> = {};
  if (MUTATING_METHODS.includes(method.toUpperCase())) {
    const token = getSessionToken();
    if (token) {
      authHeaders["authorization"] = `Bearer ${token}`;
    }
  }

  let res: Response;
  try {
    res = await fetch(path, {
      headers: {
        "content-type": "application/json",
        ...authHeaders,
        ...(init?.headers ?? {}),
      },
      ...init,
    });
  } catch (err) {
    const event =
      err instanceof DOMException && err.name === "AbortError"
        ? "api.request.abort"
        : "api.request.error";
    trace.warn(event, {
      duration_ms: durationSince(started),
      error: errorSummary(err),
    });
    throw err;
  }

  if (!res.ok) {
    let body: ApiErrorShape | undefined;
    try {
      body = (await res.json()) as ApiErrorShape;
    } catch {
      // server didn't send JSON — fall back to status text
    }
    trace.warn("api.request.error", {
      status: res.status,
      code: body?.code,
      error_message: body?.message,
      duration_ms: durationSince(started),
    });

    // On 401 with "unauthenticated" (session gate), redirect to /login so the
    // user can create a session. Pass `next` so they land back on this page.
    // Loopback clients never hit this because the middleware passes them through.
    if (res.status === 401 && body?.error === "unauthenticated") {
      const next = encodeURIComponent(
        window.location.pathname + window.location.search,
      );
      window.location.href = `/login?next=${next}`;
    }

    throw new ApiError(
      res.status,
      body?.code ?? body?.error ?? "http_error",
      body?.message ?? res.statusText ?? `HTTP ${res.status}`,
      body?.field,
    );
  }

  trace.debug("api.request.ok", {
    status: res.status,
    duration_ms: durationSince(started),
  });
  if (res.status === 204) return undefined as T;
  try {
    return (await res.json()) as T;
  } catch (err) {
    trace.error("api.response.parse_error", {
      status: res.status,
      duration_ms: durationSince(started),
      error: errorSummary(err),
    });
    throw err;
  }
}
