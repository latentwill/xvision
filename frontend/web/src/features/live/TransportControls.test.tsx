import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { TransportControls } from "./TransportControls";

describe("TransportControls visible action set", () => {
  test("ACTIVE shows Pause + Stop, no Resume", () => {
    render(<TransportControls status="ACTIVE" onPause={() => {}} onStop={() => {}} />);
    expect(screen.getByLabelText("Pause strategy")).toBeInTheDocument();
    expect(screen.getByLabelText("Stop strategy")).toBeInTheDocument();
    expect(screen.queryByLabelText("Resume strategy")).not.toBeInTheDocument();
  });
  test("PAUSED shows Resume + Stop, no Pause", () => {
    render(<TransportControls status="PAUSED" onResume={() => {}} onStop={() => {}} />);
    expect(screen.getByLabelText("Resume strategy")).toBeInTheDocument();
    expect(screen.getByLabelText("Stop strategy")).toBeInTheDocument();
    expect(screen.queryByLabelText("Pause strategy")).not.toBeInTheDocument();
  });
  test("STOPPED shows none of the transport buttons", () => {
    render(<TransportControls status="STOPPED" />);
    expect(screen.queryByLabelText("Pause strategy")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Resume strategy")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Stop strategy")).not.toBeInTheDocument();
  });
});

describe("wallet + busy gating", () => {
  test("walletDisabled disables all rendered buttons", () => {
    render(
      <TransportControls
        status="ACTIVE"
        walletDisabled
        onPause={() => {}}
        onStop={() => {}}
      />,
    );
    expect(screen.getByLabelText("Pause strategy")).toBeDisabled();
    expect(screen.getByLabelText("Stop strategy")).toBeDisabled();
  });
  test("busy disables buttons (blocks double-fire)", () => {
    render(<TransportControls status="ACTIVE" busy onPause={() => {}} onStop={() => {}} />);
    expect(screen.getByLabelText("Pause strategy")).toBeDisabled();
  });
  test("missing handler renders a disabled placeholder (B-I parity)", () => {
    render(<TransportControls status="ACTIVE" />);
    expect(screen.getByLabelText("Pause strategy")).toBeDisabled();
  });
});

describe("pause inline expander (no popup)", () => {
  test("shows Positions held + Flatten / Keep open", () => {
    const onFlatten = vi.fn();
    const onKeepOpen = vi.fn();
    render(
      <TransportControls
        status="PAUSED"
        pausedExpanderOpen
        onFlatten={onFlatten}
        onKeepOpen={onKeepOpen}
      />,
    );
    expect(screen.getByTestId("paused-expander")).toBeInTheDocument();
    expect(screen.getByText("Positions held.")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Flatten positions"));
    expect(onFlatten).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByText("Keep open"));
    expect(onKeepOpen).toHaveBeenCalledTimes(1);
  });
  test("flattenPending swaps to flattening… (run stays paused)", () => {
    render(<TransportControls status="PAUSED" pausedExpanderOpen flattenPending />);
    expect(screen.getByTestId("flatten-pending")).toHaveTextContent("Flattening");
    expect(screen.queryByText("Flatten positions")).not.toBeInTheDocument();
  });
});

describe("stop type-to-confirm expander (no popup)", () => {
  test("Stop button disabled until the confirm word is typed", () => {
    const onStopConfirm = vi.fn();
    render(
      <TransportControls
        status="ACTIVE"
        stopConfirmOpen
        confirmWord="STOP"
        onStopConfirm={onStopConfirm}
        onStopCancel={() => {}}
      />,
    );
    const confirm = screen.getByRole("button", { name: "Stop" });
    expect(confirm).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Type to confirm stop"), {
      target: { value: "STOP" },
    });
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    expect(onStopConfirm).toHaveBeenCalledTimes(1);
  });
  test("confirm is case-insensitive and trims", () => {
    const onStopConfirm = vi.fn();
    render(
      <TransportControls
        status="ACTIVE"
        stopConfirmOpen
        confirmWord="BTC Momentum"
        onStopConfirm={onStopConfirm}
        onStopCancel={() => {}}
      />,
    );
    fireEvent.change(screen.getByLabelText("Type to confirm stop"), {
      target: { value: "  btc momentum  " },
    });
    expect(screen.getByRole("button", { name: "Stop" })).toBeEnabled();
  });
  test("Cancel dismisses without confirming", () => {
    const onStopCancel = vi.fn();
    render(
      <TransportControls
        status="ACTIVE"
        stopConfirmOpen
        onStopCancel={onStopCancel}
        onStopConfirm={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onStopCancel).toHaveBeenCalledTimes(1);
  });
});

describe("inline error", () => {
  test("renders an alert with the error text (no toast)", () => {
    render(<TransportControls status="ACTIVE" error="Pause failed: 500" />);
    expect(screen.getByTestId("transport-error")).toHaveTextContent(
      "Pause failed: 500",
    );
  });
});
