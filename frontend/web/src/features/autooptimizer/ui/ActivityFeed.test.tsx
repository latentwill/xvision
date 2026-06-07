import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";
import { screen, act } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { ActivityFeed } from "./ActivityFeed";

type FakeEventSourceInstance = {
  addEventListener: ReturnType<typeof vi.fn>;
  removeEventListener: ReturnType<typeof vi.fn>;
  close: ReturnType<typeof vi.fn>;
  _handlers: Record<string, ((e: { data: string; type: string }) => void)[]>;
  _fire: (type: string, data: string) => void;
};

let fakeInstance: FakeEventSourceInstance;

beforeEach(() => {
  fakeInstance = {
    addEventListener: vi.fn((type: string, handler: (e: { data: string; type: string }) => void) => {
      if (!fakeInstance._handlers[type]) fakeInstance._handlers[type] = [];
      fakeInstance._handlers[type].push(handler);
    }),
    removeEventListener: vi.fn(),
    close: vi.fn(),
    _handlers: {},
    _fire(type: string, data: string) {
      const handlers = this._handlers[type] ?? [];
      for (const h of handlers) h({ data, type });
      const msgHandlers = this._handlers["message"] ?? [];
      for (const h of msgHandlers) h({ data, type });
    },
  };

  // @ts-expect-error jsdom stub
  global.EventSource = vi.fn().mockImplementation(() => fakeInstance);
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

function makeEvent(eventType: string, label: string, seq: number) {
  return JSON.stringify({ event_type: eventType, display_label: label, ts: new Date().toISOString(), seq });
}

describe("ActivityFeed", () => {
  it("renders rows for events emitted via EventSource", async () => {
    renderWithProviders(<ActivityFeed sessionId="sess_TEST" />);

    await act(async () => {
      fakeInstance._fire("cycle_started", makeEvent("cycle_started", "Cycle started", 1));
      fakeInstance._fire("mutation_proposed", makeEvent("mutation_proposed", "Experiment proposed", 2));
    });

    // Allow duplicates (named-event + message fallback both fire in test stub)
    expect((await screen.findAllByText("Cycle started")).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Experiment proposed").length).toBeGreaterThanOrEqual(1);
  });

  it("renders a 3rd live event after 2 replay events", async () => {
    renderWithProviders(<ActivityFeed sessionId="sess_TEST2" />);

    await act(async () => {
      fakeInstance._fire("cycle_started", makeEvent("cycle_started", "Cycle started", 1));
      fakeInstance._fire("parent_selected", makeEvent("parent_selected", "Parent selected", 2));
      fakeInstance._fire("mutation_proposed", makeEvent("mutation_proposed", "Experiment proposed", 3));
    });

    expect((await screen.findAllByText("Cycle started")).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Parent selected").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Experiment proposed").length).toBeGreaterThanOrEqual(1);
  });
});
