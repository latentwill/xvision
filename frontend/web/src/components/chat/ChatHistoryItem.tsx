import { useChatTitle } from "./useChatTitle";

/**
 * Conversation-history row. Shows the auto-generated title primary
 * with the timestamp as a smaller secondary line (per
 * `chat-history-auto-title` acceptance). Falls back to the date-only
 * label when no title has been generated yet — the no-provider /
 * pre-first-response paths land here.
 */
export function ChatHistoryItem({
  sessionId,
  lastActivityAt,
  isActive,
  firstUser,
  firstAssistant,
  providerName,
  modelId,
  providersConfigured,
  ready,
  onClick,
}: {
  sessionId: string;
  lastActivityAt: string;
  isActive: boolean;
  firstUser?: string;
  firstAssistant?: string;
  providerName: string | null;
  modelId: string;
  providersConfigured: boolean;
  ready: boolean;
  onClick: () => void;
}) {
  const title = useChatTitle({
    sessionId,
    firstUser,
    firstAssistant,
    providerName,
    modelId,
    providersConfigured,
    ready,
  });
  const dateLabel = new Date(lastActivityAt).toLocaleString();

  return (
    <button
      type="button"
      onClick={onClick}
      data-testid={`chat-history-item-${sessionId}`}
      className={`w-full text-left rounded px-2 py-1 border ${
        isActive
          ? "border-gold/40 text-text bg-gold/5"
          : "border-border-soft text-text-2 hover:text-text"
      }`}
    >
      {title ? (
        <>
          <div
            data-testid="chat-history-title"
            className="text-[12px] leading-tight text-text"
          >
            {title}
          </div>
          <div className="text-[10px] text-text-3 leading-tight">
            {dateLabel}
          </div>
        </>
      ) : (
        <div data-testid="chat-history-date" className="text-[11px]">
          {dateLabel}
        </div>
      )}
    </button>
  );
}
