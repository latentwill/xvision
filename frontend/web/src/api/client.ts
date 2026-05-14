// Lightweight typed fetch wrapper. Talks to the dashboard's `/api/*` surface
// (proxied via Vite in dev, same-origin in prod via the embedded SPA).
//
// Error shape mirrors `DashboardError`'s JSON body:
//   { "code": "not_found" | "validation" | "conflict" | "internal", "message": string }
//
// On non-2xx the helper throws `ApiError` with the parsed code + message.

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
  const res = await fetch(path, {
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
    ...init,
  });

  if (!res.ok) {
    let body: ApiErrorShape | undefined;
    try {
      body = (await res.json()) as ApiErrorShape;
    } catch {
      // server didn't send JSON — fall back to status text
    }
    throw new ApiError(
      res.status,
      body?.code ?? "http_error",
      body?.message ?? res.statusText ?? `HTTP ${res.status}`,
    );
  }

  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}
