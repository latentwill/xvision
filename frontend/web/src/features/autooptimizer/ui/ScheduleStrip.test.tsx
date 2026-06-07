import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";
import { screen, fireEvent, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../test-utils";
import { ScheduleStrip } from "./ScheduleStrip";
import * as apiModule from "../api";

afterEach(() => vi.restoreAllMocks());

// ─── helpers ──────────────────────────────────────────────────────────────────

function mockSchedule(data: import("../api").Schedule | null) {
  vi.spyOn(apiModule, "useSchedule").mockReturnValue({
    data,
    isLoading: false,
    isError: false,
  } as unknown as ReturnType<typeof apiModule.useSchedule>);
}

function mockUpsertSchedule(mutateFn = vi.fn()) {
  vi.spyOn(apiModule, "useUpsertSchedule").mockReturnValue({
    mutate: mutateFn,
    isPending: false,
  } as unknown as ReturnType<typeof apiModule.useUpsertSchedule>);
}

const fixtureSchedule: import("../api").Schedule = {
  id: 1,
  enabled: true,
  time_local: "21:00",
  strategy_id: "strategy-abc",
  last_run_at: null,
  next_run_at: null,
};

// ─── tests ────────────────────────────────────────────────────────────────────

describe("ScheduleStrip — no schedule", () => {
  beforeEach(() => {
    mockSchedule(null);
    mockUpsertSchedule();
  });

  it("renders 'No scheduled run' when useSchedule returns null", async () => {
    renderWithProviders(<ScheduleStrip />);
    expect(await screen.findByText(/No scheduled run/i)).toBeInTheDocument();
  });

  it("renders 'Set one' link/button when no schedule", async () => {
    renderWithProviders(<ScheduleStrip />);
    expect(await screen.findByText(/Set one/i)).toBeInTheDocument();
  });

  it("clicking 'Set one' expands an inline accordion form (not a Dialog/Sheet/Popover)", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ScheduleStrip />);
    const setOneBtn = await screen.findByText(/Set one/i);
    await user.click(setOneBtn);

    // Form fields should appear inline (not in a portal/dialog)
    const inputs = await screen.findAllByRole("textbox");
    expect(inputs.length).toBeGreaterThanOrEqual(1);

    // Must NOT have any dialog/sheet/popover role
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.querySelector("[data-radix-popper-content-wrapper]")).toBeNull();
  });

  it("accordion form does not use any modal component", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ScheduleStrip />);
    const setOneBtn = await screen.findByText(/Set one/i);
    await user.click(setOneBtn);

    // Confirm no modal artifacts
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.querySelector("[aria-modal]")).toBeNull();
  });
});

describe("ScheduleStrip — has schedule", () => {
  beforeEach(() => {
    mockSchedule(fixtureSchedule);
    mockUpsertSchedule();
  });

  it("renders 'Next: 21:00 · strategy-abc' from fixture schedule", async () => {
    renderWithProviders(<ScheduleStrip />);
    // "Next:" label is in the strip
    expect(await screen.findByText(/Next:/i)).toBeInTheDocument();
    // time and strategy id appear as their own text nodes
    expect(screen.getByText("21:00")).toBeInTheDocument();
    expect(screen.getByText("strategy-abc")).toBeInTheDocument();
  });

  it("renders an enable/disable toggle", async () => {
    renderWithProviders(<ScheduleStrip />);
    // Checkbox or button with accessible label for enable/disable
    const toggle = await screen.findByRole("checkbox");
    expect(toggle).toBeInTheDocument();
  });

  it("clicking the toggle fires the update mutation", async () => {
    const mutateFn = vi.fn();
    mockUpsertSchedule(mutateFn);
    const user = userEvent.setup();
    renderWithProviders(<ScheduleStrip />);

    const toggle = await screen.findByRole("checkbox");
    await user.click(toggle);

    expect(mutateFn).toHaveBeenCalledWith(
      expect.objectContaining({ enabled: false }),
    );
  });

  it("renders an 'Edit' button", async () => {
    renderWithProviders(<ScheduleStrip />);
    expect(await screen.findByRole("button", { name: /edit/i })).toBeInTheDocument();
  });

  it("clicking 'Edit' shows inline accordion form (not a Dialog/Sheet/Popover)", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ScheduleStrip />);
    const editBtn = await screen.findByRole("button", { name: /edit/i });
    await user.click(editBtn);

    // Form fields inline
    const inputs = await screen.findAllByRole("textbox");
    expect(inputs.length).toBeGreaterThanOrEqual(1);

    // No dialog/modal/popover
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.querySelector("[data-radix-popper-content-wrapper]")).toBeNull();
    expect(document.querySelector("[aria-modal]")).toBeNull();
  });

  it("'Cancel' in accordion collapses the form", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ScheduleStrip />);
    const editBtn = await screen.findByRole("button", { name: /edit/i });
    await user.click(editBtn);
    expect(await screen.findAllByRole("textbox")).toBeTruthy();

    const cancelBtn = await screen.findByRole("button", { name: /cancel/i });
    await user.click(cancelBtn);

    await waitFor(() => {
      expect(screen.queryAllByRole("textbox")).toHaveLength(0);
    });
  });
});

describe("ScheduleStrip — accordion form submits", () => {
  it("Save calls useUpsertSchedule.mutate with form values", async () => {
    const mutateFn = vi.fn();
    mockSchedule(null);
    mockUpsertSchedule(mutateFn);
    const user = userEvent.setup();
    renderWithProviders(<ScheduleStrip />);

    const setOneBtn = await screen.findByText(/Set one/i);
    await user.click(setOneBtn);

    // Fill both required fields
    const timeInput = await screen.findByRole("textbox", { name: /scheduled time/i });
    await user.clear(timeInput);
    await user.type(timeInput, "09:00");

    const strategyInput = await screen.findByRole("textbox", { name: /strategy id/i });
    await user.clear(strategyInput);
    await user.type(strategyInput, "my-strategy");

    const saveBtn = await screen.findByRole("button", { name: /save/i });
    // Save should be enabled now
    expect(saveBtn).not.toBeDisabled();
    await user.click(saveBtn);

    expect(mutateFn).toHaveBeenCalled();
  });
});
