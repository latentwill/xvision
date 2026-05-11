// Wizard chat — POSTs to /api/wizard/chat and streams Server-Sent Events
// of `WizardEvent` JSON. EventSource is GET-only, so we hand-roll the SSE
// parse over `fetch().body.getReader()`.

import { ApiError } from "./client";

export type WizardEvent =
  | { type: "token"; text: string }
  | { type: "tool_call"; tool: string; args: unknown }
  | { type: "tool_result"; tool: string; result: unknown }
  | { type: "done"; draft_id?: string | null }
  | { type: "error"; message: string };

export type ChatRequest = {
  message: string;
  /// Optional model id. When omitted, the dashboard falls back to
  /// [intern].model for the default provider.
  model?: string;
  /// Optional provider name. When omitted, the dashboard falls back to
  /// the [intern]-referenced default.
  provider?: string;
};

/// Async generator that yields one `WizardEvent` per server SSE frame.
/// Throws `ApiError` on non-2xx; the WizardEvent::Error variant carries
/// model/dispatch failures *during* the stream.
export async function* streamChat(
  req: ChatRequest,
  signal?: AbortSignal,
): AsyncGenerator<WizardEvent> {
  console.info("[wizard] streamChat", {
    provider: req.provider,
    model: req.model,
    message_len: req.message.length,
  });
  const res = await fetch("/api/wizard/chat", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      accept: "text/event-stream",
    },
    body: JSON.stringify(req),
    signal,
  });
  if (!res.ok || !res.body) {
    let body: { code?: string; message?: string } | undefined;
    try {
      body = await res.json();
    } catch {
      // not JSON
    }
    console.error("[wizard] streamChat failed", {
      status: res.status,
      code: body?.code,
      message: body?.message,
    });
    throw new ApiError(
      res.status,
      body?.code ?? "http_error",
      body?.message ?? res.statusText ?? `HTTP ${res.status}`,
    );
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    // SSE frame separator is a blank line ("\n\n"). Anything left over
    // after the final separator is the start of the next frame.
    const frames = buf.split("\n\n");
    buf = frames.pop() ?? "";
    for (const frame of frames) {
      const dataLine = frame
        .split("\n")
        .find((l) => l.startsWith("data:"));
      if (!dataLine) continue;
      const json = dataLine.slice(5).trim();
      if (!json) continue;
      try {
        yield JSON.parse(json) as WizardEvent;
      } catch {
        // skip malformed frames; the server should never produce them
      }
    }
  }
}
