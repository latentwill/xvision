import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { DocsMarkdown } from "./DocsMarkdown";

// Mock navigator.clipboard
const writeTextMock = vi.fn().mockResolvedValue(undefined);

beforeEach(() => {
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText: writeTextMock },
    configurable: true,
    writable: true,
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("DocsMarkdown — fenced code blocks", () => {
  test("renders a language badge for a fenced rust block", async () => {
    const body = "```rust\nfn main() {}\n```";
    render(<DocsMarkdown body={body} />);
    const badge = await screen.findByTestId("code-lang-badge");
    expect(badge).toHaveTextContent("rust");
  });

  test("renders no language badge for a fenced block with no language", () => {
    const body = "```\nplain text\n```";
    render(<DocsMarkdown body={body} />);
    expect(screen.queryByTestId("code-lang-badge")).toBeNull();
  });

  test("renders a copy button for a fenced block", async () => {
    const body = "```toml\n[package]\nname = \"x\"\n```";
    render(<DocsMarkdown body={body} />);
    const btn = await screen.findByRole("button", { name: /copy/i });
    expect(btn).toBeInTheDocument();
  });

  test("clicking copy button calls navigator.clipboard.writeText with the code text", async () => {
    const code = "fn main() {}";
    const body = `\`\`\`rust\n${code}\n\`\`\``;
    render(<DocsMarkdown body={body} />);
    const btn = await screen.findByRole("button", { name: /copy/i });
    fireEvent.click(btn);
    await waitFor(() => expect(writeTextMock).toHaveBeenCalledTimes(1));
    expect(writeTextMock).toHaveBeenCalledWith(expect.stringContaining(code));
  });

  test("copy button shows 'Copied' transiently after click then reverts", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: false });
    const body = "```rust\nlet x = 1;\n```";
    render(<DocsMarkdown body={body} />);
    const btn = screen.getByRole("button", { name: /copy/i });

    // Click and flush the Promise resolution + React update inside act
    await act(async () => {
      fireEvent.click(btn);
      // flush the clipboard promise chain
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(btn).toHaveTextContent("Copied");

    // Advance past the 1500ms timeout and flush React update
    act(() => {
      vi.advanceTimersByTime(1600);
    });

    expect(btn).toHaveTextContent("Copy");
    vi.useRealTimers();
  });

  test("copy button does nothing when navigator.clipboard is unavailable", () => {
    Object.defineProperty(navigator, "clipboard", {
      value: undefined,
      configurable: true,
      writable: true,
    });
    const body = "```rust\nlet x = 1;\n```";
    render(<DocsMarkdown body={body} />);
    // Use synchronous query — the button renders immediately
    const btn = screen.getByRole("button", { name: /copy/i });
    // Should not throw
    expect(() => fireEvent.click(btn)).not.toThrow();
    expect(writeTextMock).not.toHaveBeenCalled();
  });

  test("inline code does NOT render a badge or copy button", () => {
    const body = "Use the `xvn run` command.";
    render(<DocsMarkdown body={body} />);
    expect(screen.queryByTestId("code-lang-badge")).toBeNull();
    expect(screen.queryByRole("button", { name: /copy/i })).toBeNull();
  });
});
