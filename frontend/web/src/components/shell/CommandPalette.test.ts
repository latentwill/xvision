import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { createElement } from "react";
import { MemoryRouter, useLocation } from "react-router-dom";

import type { SearchHit } from "@/api/search";
import * as searchApi from "@/api/search";
import { useUi } from "@/stores/ui";

import { CommandPalette, STATIC_ACTIONS } from "./CommandPalette";

vi.mock("@/api/search", async () => {
  const actual = await vi.importActual<typeof import("@/api/search")>(
    "@/api/search",
  );
  return {
    ...actual,
    searchArtifacts: vi.fn(),
  };
});

function LocationProbe() {
  const location = useLocation();
  return createElement("div", { "data-testid": "location" }, location.pathname);
}

function renderPalette() {
  return render(
    createElement(
      MemoryRouter,
      { initialEntries: ["/"] },
      createElement(CommandPalette),
      createElement(LocationProbe),
    ),
  );
}

function mockDialog() {
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
    this.dispatchEvent(new Event("close"));
  };
}

const staleHit: SearchHit = {
  kind: "strategy",
  artifact_id: "strategy:old",
  title: "Old backend result",
  summary: "Previous query result",
  tags: ["strategy"],
  href: "/strategies/old",
  updated_at: "",
  bm25_score: 1,
};

beforeEach(() => {
  vi.useFakeTimers();
  mockDialog();
  useUi.setState({ cmdkOpen: true });
  vi.mocked(searchApi.searchArtifacts).mockResolvedValue([staleHit]);
});

afterEach(() => {
  vi.useRealTimers();
  useUi.setState({ cmdkOpen: false });
  vi.restoreAllMocks();
});

describe("CommandPalette static actions", () => {
  it("names the root route as Dashboard", () => {
    const home = STATIC_ACTIONS.find((a) => a.artifact_id === "nav:home");

    expect(home).toMatchObject({
      title: "Dashboard",
      summary: "Workspace status at a glance",
      href: "/",
    });
  });

  it("resets selection and clears stale backend hits on query change", async () => {
    renderPalette();

    await act(async () => {
      vi.advanceTimersByTime(80);
      await Promise.resolve();
    });
    expect(screen.getByText("Old backend result")).toBeInTheDocument();

    const input = screen.getByPlaceholderText(
      /jump to a strategy, run, scenario, or action/i,
    );
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.change(input, { target: { value: "strategies" } });
    expect(screen.queryByText("Old backend result")).not.toBeInTheDocument();
    fireEvent.keyDown(input, { key: "Enter" });

    expect(screen.getByTestId("location")).toHaveTextContent("/strategies");
  });
});
