// Compatibility wrapper for older setup code. New chat surfaces should use
// `api/chat_rail.ts` so they share session resolution and profile controls.

import {
  streamChat as streamAgentChat,
  type WizardEvent as AgentWizardEvent,
} from "./chat_rail";

export type WizardEvent = AgentWizardEvent;

export type ChatRequest = {
  session_id?: string;
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
  if (!req.session_id) {
    throw new Error("wizard streamChat compatibility wrapper requires session_id");
  }
  for await (const ev of streamAgentChat(
    {
      session_id: req.session_id,
      message: req.message,
      provider: req.provider,
      model: req.model,
      profile: "strategy_setup",
    },
    signal,
  )) {
    yield ev;
  }
}
