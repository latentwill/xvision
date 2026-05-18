import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";

vi.mock("@/api/chat_rail", async () => {
  const actual = await vi.importActual<typeof import("@/api/chat_rail")>(
    "@/api/chat_rail",
  );
  return {
    ...actual,
    createSession: vi.fn(),
    deleteSession: vi.fn(),
    streamChat: vi.fn(),
  };
});

const chatRail = await import("@/api/chat_rail");
const {
  __resetChatTitleForTests,
  cleanTitle,
  summarizeChatTitle,
  useChatTitle,
} = await import("./useChatTitle");

function asyncTokens(text: string) {
  return (async function* () {
    yield { type: "token" as const, text };
    yield { type: "done" as const };
  })();
}

function Harness(props: {
  sessionId: string;
  firstUser?: string;
  firstAssistant?: string;
  providerName?: string | null;
  modelId?: string;
  providersConfigured?: boolean;
  ready?: boolean;
}) {
  const title = useChatTitle({
    sessionId: props.sessionId,
    firstUser: props.firstUser,
    firstAssistant: props.firstAssistant,
    providerName: props.providerName ?? "anthropic",
    modelId: props.modelId ?? "claude-haiku",
    providersConfigured: props.providersConfigured ?? true,
    ready: props.ready ?? true,
  });
  return <div data-testid="title">{title ?? "(no title)"}</div>;
}

beforeEach(() => {
  __resetChatTitleForTests();
  localStorage.clear();
  vi.mocked(chatRail.createSession).mockResolvedValue({
    session_id: "throwaway",
    history: [],
  });
  vi.mocked(chatRail.deleteSession).mockResolvedValue(undefined);
  vi.mocked(chatRail.streamChat).mockImplementation(() =>
    asyncTokens(" Strategy review of Q2 momentum signals "),
  );
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("cleanTitle", () => {
  test("strips wrapping quotes and terminal punctuation", () => {
    expect(cleanTitle('"Strategy review of Q2 momentum signals."')).toBe(
      "Strategy review of Q2 momentum signals",
    );
  });

  test("clamps to 7 words and 80 chars", () => {
    const eightWords = "one two three four five six seven eight";
    expect(cleanTitle(eightWords)).toBe("one two three four five six seven");
  });

  test("collapses whitespace", () => {
    expect(cleanTitle("  hello   world  ")).toBe("hello world");
  });
});

describe("summarizeChatTitle", () => {
  test("creates a throwaway session, accumulates tokens, deletes after", async () => {
    const out = await summarizeChatTitle({
      firstUser: "How should I tune SMA cross?",
      firstAssistant: "Start with 20/50 and walk forward.",
      providerName: "anthropic",
      modelId: "claude-haiku",
    });
    expect(out).toBe("Strategy review of Q2 momentum signals");
    expect(chatRail.createSession).toHaveBeenCalledOnce();
    expect(chatRail.streamChat).toHaveBeenCalledOnce();
    expect(chatRail.deleteSession).toHaveBeenCalledWith("throwaway");
  });

  test("throws on empty body", async () => {
    vi.mocked(chatRail.streamChat).mockImplementation(() => asyncTokens(""));
    await expect(
      summarizeChatTitle({
        firstUser: "x",
        firstAssistant: "y",
        providerName: "anthropic",
      }),
    ).rejects.toThrow(/empty title/);
  });
});

describe("useChatTitle", () => {
  test("renders the generated title once summarize resolves", async () => {
    render(
      <Harness
        sessionId="s1"
        firstUser="How do I tune SMA?"
        firstAssistant="Try 20/50."
      />,
    );
    await waitFor(() =>
      expect(screen.getByTestId("title")).toHaveTextContent(
        "Strategy review of Q2 momentum signals",
      ),
    );
  });

  test("falls back to null (date label upstream) when no provider configured", async () => {
    render(
      <Harness
        sessionId="s_no_provider"
        firstUser="hi"
        firstAssistant="hello"
        providersConfigured={false}
        providerName={null}
      />,
    );
    await new Promise((r) => setTimeout(r, 10));
    expect(screen.getByTestId("title")).toHaveTextContent("(no title)");
    expect(chatRail.streamChat).not.toHaveBeenCalled();
  });

  test("fires summarize exactly once per session even across remounts", async () => {
    const { unmount } = render(
      <Harness
        sessionId="s_once"
        firstUser="hi"
        firstAssistant="hello"
      />,
    );
    await waitFor(() => expect(chatRail.streamChat).toHaveBeenCalledOnce());
    unmount();
    render(
      <Harness
        sessionId="s_once"
        firstUser="hi"
        firstAssistant="hello"
      />,
    );
    // Second mount reads the cache — no new streamChat call.
    await new Promise((r) => setTimeout(r, 10));
    expect(chatRail.streamChat).toHaveBeenCalledOnce();
  });

  test("does not throw on summarizer failure and does not retry", async () => {
    vi.mocked(chatRail.streamChat).mockImplementation(() => {
      return (async function* () {
        yield { type: "error" as const, message: "boom" };
      })();
    });
    const { unmount } = render(
      <Harness sessionId="s_fail" firstUser="hi" firstAssistant="hello" />,
    );
    await waitFor(() => expect(chatRail.streamChat).toHaveBeenCalled());
    // Renders falls back to no-title so the consumer shows the date.
    expect(screen.getByTestId("title")).toHaveTextContent("(no title)");
    unmount();
    render(<Harness sessionId="s_fail" firstUser="hi" firstAssistant="hello" />);
    await new Promise((r) => setTimeout(r, 10));
    // Module-level failure guard prevents a retry storm.
    expect(chatRail.streamChat).toHaveBeenCalledOnce();
  });

  test("skips dispatch until ready (first assistant turn complete)", async () => {
    render(
      <Harness
        sessionId="s_not_ready"
        firstUser="hi"
        firstAssistant="hello"
        ready={false}
      />,
    );
    await new Promise((r) => setTimeout(r, 10));
    expect(chatRail.streamChat).not.toHaveBeenCalled();
  });
});
