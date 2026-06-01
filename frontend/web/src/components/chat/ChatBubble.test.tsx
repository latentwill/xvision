import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ChatBubble } from "./ChatBubble";
import type { Bubble } from "./types";

describe("ChatBubble", () => {
  it("narrates unknown failed tools as failures", () => {
    const bubble: Bubble = {
      role: "assistant",
      blocks: [],
      tools: [
        {
          call: "some_tool",
          ok: false,
          summary: "failed",
          resultSummary: "permission denied",
        },
      ],
    };

    render(<ChatBubble bubble={bubble} isLast={false} isStreaming={false} />);

    expect(screen.getByText("!")).toBeInTheDocument();
    expect(screen.getByText(/some_tool failed:/)).toBeInTheDocument();
    expect(screen.getByText("permission denied")).toBeInTheDocument();
    expect(screen.queryByText(/some_tool completed/)).not.toBeInTheDocument();
  });

  it("pending tool (ok=false, pending=true) is not styled as failed", () => {
    const bubble: Bubble = {
      role: "assistant",
      blocks: [],
      tools: [
        {
          call: "some_tool",
          ok: false,
          pending: true,
          summary: "running",
        },
      ],
    };

    render(<ChatBubble bubble={bubble} isLast={false} isStreaming={false} />);

    expect(screen.getByLabelText("running")).toBeInTheDocument();
    expect(screen.queryByText("!")).not.toBeInTheDocument();
    expect(screen.queryByText(/some_tool failed:/)).not.toBeInTheDocument();
  });

  it("failed tool (ok=false, no pending) is styled as failed", () => {
    const bubble: Bubble = {
      role: "assistant",
      blocks: [],
      tools: [
        {
          call: "some_tool",
          ok: false,
          summary: "failed",
          resultSummary: "permission denied",
        },
      ],
    };

    render(<ChatBubble bubble={bubble} isLast={false} isStreaming={false} />);

    expect(screen.getByText("!")).toBeInTheDocument();
    expect(screen.queryByLabelText("running")).not.toBeInTheDocument();
    expect(screen.getByText(/some_tool failed:/)).toBeInTheDocument();
  });

  it("renders markdown links without opener access", () => {
    const bubble: Bubble = {
      role: "assistant",
      blocks: [{ kind: "text", text: "[open](https://attacker.example)" }],
      tools: [],
    };

    render(<ChatBubble bubble={bubble} isLast={false} isStreaming={false} />);

    const link = screen.getByRole("link", { name: "open" });
    expect(link).toHaveAttribute("href", "https://attacker.example");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });
});
