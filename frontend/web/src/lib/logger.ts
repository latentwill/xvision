export type LogLevel = "debug" | "info" | "warn" | "error" | "silent";

export type LogDomain =
  | "app"
  | "route"
  | "api"
  | "query"
  | "mutation"
  | "chat"
  | "eval"
  | "stream"
  | "chart"
  | "settings"
  | "scenario"
  | "strategy";

export type LogContext = Record<string, unknown>;

type LogEntry = {
  ts: string;
  level: Exclude<LogLevel, "silent">;
  domain: LogDomain;
  event: string;
  ctx: LogContext;
};

type TraceLogger = {
  id: string;
  debug(event: string, ctx?: LogContext): void;
  info(event: string, ctx?: LogContext): void;
  warn(event: string, ctx?: LogContext): void;
  error(event: string, ctx?: LogContext): void;
  child(base: LogContext): TraceLogger;
};

type XvnLogConsole = {
  setLevel(level: LogLevel): void;
  getLevel(): LogLevel;
  enableDebug(): void;
  disable(): void;
  dumpBuffer(): LogEntry[];
  clearBuffer(): void;
};

declare global {
  interface Window {
    xvnLog?: XvnLogConsole;
  }
}

const STORAGE_KEY = "xvn.log.level";
const BUFFER_LIMIT = 500;
const LEVELS: Record<LogLevel, number> = {
  debug: 10,
  info: 20,
  warn: 30,
  error: 40,
  silent: 50,
};
const REDACTED = "[redacted]";
const MAX_STRING = 240;
const SAFE_STRING_KEYS = new Set([
  "id",
  "trace_id",
  "request_id",
  "stream_id",
  "run_id",
  "strategy_id",
  "scenario_id",
  "session_id",
  "provider",
  "model",
  "profile",
  "method",
  "path",
  "route",
  "status",
  "phase",
  "event",
  "kind",
  "code",
  "name",
  "asset",
  "granularity",
]);

let currentLevel: LogLevel = initialLevel();
let installed = false;
const buffer: LogEntry[] = [];

function appMode(): string {
  const meta = import.meta as unknown as { env?: { MODE?: string; DEV?: boolean } };
  return meta.env?.MODE ?? "production";
}

function appIsDev(): boolean {
  const meta = import.meta as unknown as { env?: { DEV?: boolean } };
  return meta.env?.DEV ?? false;
}

function initialLevel(): LogLevel {
  const urlLevel = parseUrlLevel();
  if (urlLevel) {
    writeStorageLevel(urlLevel);
    return urlLevel;
  }
  const stored = readStorageLevel();
  if (stored) return stored;
  if (appMode() === "test") return "silent";
  return appIsDev() ? "info" : "warn";
}

function parseUrlLevel(): LogLevel | undefined {
  if (typeof window === "undefined") return undefined;
  try {
    const value = new URLSearchParams(window.location.search).get("xvn_log");
    return parseLevel(value);
  } catch {
    return undefined;
  }
}

function readStorageLevel(): LogLevel | undefined {
  if (typeof window === "undefined") return undefined;
  try {
    return parseLevel(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return undefined;
  }
}

function writeStorageLevel(level: LogLevel) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, level);
  } catch {
    // Blocked storage should not prevent app startup.
  }
}

function parseLevel(value: string | null | undefined): LogLevel | undefined {
  if (
    value === "debug" ||
    value === "info" ||
    value === "warn" ||
    value === "error" ||
    value === "silent"
  ) {
    return value;
  }
  return undefined;
}

function shouldLog(level: Exclude<LogLevel, "silent">): boolean {
  return LEVELS[level] >= LEVELS[currentLevel] && currentLevel !== "silent";
}

function nowIso(): string {
  return new Date().toISOString();
}

function currentRoute(): string | undefined {
  if (typeof window === "undefined") return undefined;
  return `${window.location.pathname}${window.location.search}`;
}

export function setLogLevel(level: LogLevel) {
  currentLevel = level;
  writeStorageLevel(level);
}

export function getLogLevel(): LogLevel {
  return currentLevel;
}

export function clearLogBuffer() {
  buffer.length = 0;
}

export function dumpLogBuffer(): LogEntry[] {
  return buffer.map((entry) => ({
    ...entry,
    ctx: { ...entry.ctx },
  }));
}

export function logDebug(domain: LogDomain, event: string, ctx?: LogContext) {
  writeLog("debug", domain, event, ctx);
}

export function logInfo(domain: LogDomain, event: string, ctx?: LogContext) {
  writeLog("info", domain, event, ctx);
}

export function logWarn(domain: LogDomain, event: string, ctx?: LogContext) {
  writeLog("warn", domain, event, ctx);
}

export function logError(domain: LogDomain, event: string, ctx?: LogContext) {
  writeLog("error", domain, event, ctx);
}

export function createTrace(domain: LogDomain, base: LogContext = {}): TraceLogger {
  const id = safeId();
  const baseCtx = { trace_id: id, ...base };
  const trace: TraceLogger = {
    id,
    debug: (event, ctx) => logDebug(domain, event, { ...baseCtx, ...ctx }),
    info: (event, ctx) => logInfo(domain, event, { ...baseCtx, ...ctx }),
    warn: (event, ctx) => logWarn(domain, event, { ...baseCtx, ...ctx }),
    error: (event, ctx) => logError(domain, event, { ...baseCtx, ...ctx }),
    child: (childBase) => createTrace(domain, { ...baseCtx, ...childBase }),
  };
  return trace;
}

function writeLog(
  level: Exclude<LogLevel, "silent">,
  domain: LogDomain,
  event: string,
  ctx?: LogContext,
) {
  if (!shouldLog(level)) return;
  const sanitized = sanitizeContext({
    route: currentRoute(),
    ...(ctx ?? {}),
  }) as LogContext;
  const entry: LogEntry = {
    ts: nowIso(),
    level,
    domain,
    event,
    ctx: sanitized,
  };
  buffer.push(entry);
  if (buffer.length > BUFFER_LIMIT) buffer.shift();

  const method = level === "debug" ? "debug" : level;
  console[method](`[xvn:${domain}] ${event}`, sanitized);
}

export function sanitizeContext(value: unknown, key = ""): unknown {
  if (isSensitiveKey(key)) return REDACTED;
  if (value == null) return value;
  if (value instanceof Error) return errorSummary(value);
  if (typeof value === "string") return sanitizeString(value, key);
  if (typeof value === "number" || typeof value === "boolean") return value;
  if (Array.isArray(value)) {
    const primitiveSample = value
      .filter((item) => ["string", "number", "boolean"].includes(typeof item))
      .slice(0, 3)
      .map((item) => sanitizeContext(item));
    return primitiveSample.length > 0
      ? { length: value.length, sample: primitiveSample }
      : { length: value.length };
  }
  if (typeof value === "object") {
    const out: LogContext = {};
    for (const [childKey, childValue] of Object.entries(value)) {
      out[childKey] = sanitizeContext(childValue, childKey);
    }
    return out;
  }
  return String(value);
}

function sanitizeString(value: string, key: string): string {
  if (SAFE_STRING_KEYS.has(key) || value.length <= MAX_STRING) return value;
  return `${value.slice(0, MAX_STRING)}…`;
}

function isSensitiveKey(key: string): boolean {
  const normalized = key.toLowerCase();
  return (
    normalized.includes("api_key") ||
    normalized === "key" ||
    normalized.includes("token") ||
    normalized.includes("authorization") ||
    normalized.includes("secret") ||
    normalized.includes("password") ||
    normalized === "cookie" ||
    normalized === "body" ||
    normalized === "prompt" ||
    normalized === "message" ||
    normalized === "content" ||
    normalized === "transcript" ||
    normalized === "raw" ||
    normalized === "response"
  );
}

export function bodySummary(body: BodyInit | null | undefined): LogContext | undefined {
  if (!body || typeof body !== "string") return undefined;
  try {
    const parsed = JSON.parse(body) as unknown;
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return { body_keys: Object.keys(parsed as Record<string, unknown>) };
    }
    if (Array.isArray(parsed)) return { body_array_len: parsed.length };
  } catch {
    return { payload_bytes: body.length };
  }
  return { payload_bytes: body.length };
}

export function safePath(path: string): string {
  try {
    const url = new URL(path, "http://xvn.local");
    const redacted = new URLSearchParams();
    for (const [key, value] of url.searchParams.entries()) {
      redacted.set(key, isSensitiveKey(key) ? REDACTED : value);
    }
    return `${url.pathname}${redacted.size ? `?${redacted.toString()}` : ""}`;
  } catch {
    return path;
  }
}

export function safeUrlHost(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url).host;
  } catch {
    return null;
  }
}

export function errorSummary(err: unknown): LogContext {
  if (err instanceof Error) {
    const out: LogContext = {
      name: err.name,
      error_message: err.message,
    };
    const maybeApi = err as Error & { status?: number; code?: string };
    if (typeof maybeApi.status === "number") out.status = maybeApi.status;
    if (typeof maybeApi.code === "string") out.code = maybeApi.code;
    const stackTop = err.stack?.split("\n").slice(0, 2).join("\n");
    if (stackTop) out.stack_top = stackTop;
    return out;
  }
  return { error_message: String(err) };
}

export function durationSince(startMs: number): number {
  return Math.round(performance.now() - startMs);
}

export function installBrowserLogging() {
  if (installed || typeof window === "undefined") return;
  installed = true;
  window.xvnLog = {
    setLevel: setLogLevel,
    getLevel: getLogLevel,
    enableDebug: () => setLogLevel("debug"),
    disable: () => setLogLevel("silent"),
    dumpBuffer: dumpLogBuffer,
    clearBuffer: clearLogBuffer,
  };
  window.addEventListener("error", (event) => {
    logError("app", "app.unhandled_error", {
      error: errorSummary(event.error ?? event.message),
      filename: event.filename,
      lineno: event.lineno,
      colno: event.colno,
      user_agent: navigator.userAgent,
    });
  });
  window.addEventListener("unhandledrejection", (event) => {
    logError("app", "app.unhandled_rejection", {
      error: errorSummary(event.reason),
      user_agent: navigator.userAgent,
    });
  });
}

export function runtimeMode(): string {
  return appMode();
}

export function safeId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID().slice(0, 8);
  }
  return Math.random().toString(36).slice(2, 10);
}
