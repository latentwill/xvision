import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";

import type { RunListContentBlock } from "@/api/chat_rail";
import { useUi } from "@/stores/ui";

import { ChatRunListCard } from "./ChatRunListCard";

afterEach(() => {
  useUi.setState({ cmdkOpen: false });
});

describe("ChatRunListCard", () => {
  it("runs command-only footer actions", async () => {
    render(
      <MemoryRouter>
        <ChatRunListCard payload={runListWithCommandAction} />
      </MemoryRouter>,
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Search commands" }),
    );

    expect(useUi.getState().cmdkOpen).toBe(true);
  });
});

const runListWithCommandAction: RunListContentBlock = {
  type: "run_list",
  title: "Recent runs",
  runs: [
    {
      run_id: "run-a",
      rank: 1,
      strategy_id: "strategy-a",
      scenario: null,
      return_pct: 12.3,
      sharpe: 1.2,
      sparkline: [],
    },
  ],
  actions: [
    {
      label: "Search commands",
      command: "open_command_palette",
    },
  ],
};
