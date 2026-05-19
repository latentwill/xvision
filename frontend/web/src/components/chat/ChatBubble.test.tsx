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
