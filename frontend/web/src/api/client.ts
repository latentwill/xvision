// Lightweight typed fetch wrapper. Talks to the dashboard's `/api/*` surface
// (proxied via Vite in dev, same-origin in prod via the embedded SPA).
//
// Error shape mirrors `DashboardError`'s JSON body:
//   { "code": "not_found" | "validation" | "conflict" | "internal", "message": string }
//
// On non-2xx the helper throws `ApiError` with the parsed code + message.

import {
  bodySummary,
  createTrace,
  durationSince,
  errorSummary,
  safePath,
} from "@/lib/logger";

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
    this.name = "ApiError";
  }
}

type ApiErrorShape = {
  code: string;
  message: string;
};

export async function apiFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const method = init?.method ?? "GET";
  const trace = createTrace("api", {
    request_id: crypto.randomUUID?.().slice(0, 8),
    method,
    path: safePath(path),
    ...bodySummary(init?.body),
  });
  const started = performance.now();
  trace.debug("api.request.start");

  let res: Response;
  try {
    res = await fetch(path, {
      headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
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
    throw new ApiError(
      res.status,
      body?.code ?? "http_error",
      body?.message ?? res.statusText ?? `HTTP ${res.status}`,
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
