import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const driveMock = vi.fn();
const destroyMock = vi.fn();
const driverFactory = vi.fn((_config?: unknown) => ({
  drive: driveMock,
  destroy: destroyMock,
}));

vi.mock("driver.js", () => ({
  driver: driverFactory,
}));
vi.mock("driver.js/dist/driver.css", () => ({}));

import { TOUR_COMPLETED_KEY } from "./keys";
import { RestartTourButton } from "./RestartTourButton";
import { firstRunTourSteps } from "./steps";
import {
  __resetFirstRunTourForTests,
  hasCompletedFirstRunTour,
  useFirstRunTour,
} from "./useFirstRunTour";

function Harness() {
  useFirstRunTour();
  return <div data-testid="harness" />;
}

beforeEach(() => {
  driveMock.mockReset();
  destroyMock.mockReset();
  driverFactory.mockClear();
  localStorage.clear();
  __resetFirstRunTourForTests();
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("useFirstRunTour", () => {
  it("walks the getting-started path, then the advanced surfaces", () => {
    expect(firstRunTourSteps).toHaveLength(9);
    expect(firstRunTourSteps.map((step) => step.element ?? null)).toEqual([
      null,
      'a[href="/settings"]',
      'a[href="/strategies"]',
      'a[href="/scenarios"]',
      'a[href="/eval-runs"]',
      'a[href="/live"]',
      'a[href="/optimizer"]',
      'a[href="/marketplace"]',
      null,
    ]);
    expect(firstRunTourSteps.map((step) => step.popover?.title)).toEqual([
      "Welcome to XVN",
      "Connect a model",
      "Build a strategy",
      "Pick a market window",
      "Run your first backtest",
      "Deploy your winners",
      "Improve your strategy",
      "Discover & monetize",
      "You're set",
    ]);
  });

  it("fires once on a clean workspace", async () => {
    render(<Harness />);
    await waitFor(() => expect(driveMock).toHaveBeenCalledTimes(1));
    expect(driverFactory).toHaveBeenCalledTimes(1);
    const config = driverFactory.mock.calls[0]?.[0] as {
      allowClose?: boolean;
      doneBtnText?: string;
      popoverClass?: string;
      showProgress?: boolean;
      steps?: unknown[];
    };
    expect(config?.allowClose).toBe(true);
    expect(config?.showProgress).toBe(false);
    expect(config?.popoverClass).toBe("xvn-tour");
    expect(config?.doneBtnText).toBe("Finish tour");
    expect(Array.isArray(config?.steps)).toBe(true);
    expect((config?.steps ?? []).length).toBe(9);
  });

  it("does not fire again once completed", async () => {
    localStorage.setItem(TOUR_COMPLETED_KEY, "1");
    render(<Harness />);
    // Give the lazy import a tick to resolve before asserting.
    await new Promise((r) => setTimeout(r, 0));
    expect(driveMock).not.toHaveBeenCalled();
  });

  it("namespaces its storage key", () => {
    expect(TOUR_COMPLETED_KEY.startsWith("xvn.onboarding.")).toBe(true);
  });

  it("does not double-fire under React StrictMode remount", async () => {
    render(
      <StrictMode>
        <Harness />
      </StrictMode>,
    );
    // Let both StrictMode effects flush + the dynamic import resolve.
    await new Promise((r) => setTimeout(r, 10));
    await waitFor(() => expect(driveMock).toHaveBeenCalledTimes(1));
    // Settle any in-flight promises so a late second runTour would still
    // surface here.
    await new Promise((r) => setTimeout(r, 10));
    expect(driverFactory).toHaveBeenCalledTimes(1);
    expect(driveMock).toHaveBeenCalledTimes(1);
  });

  it("only reads its own storage key", async () => {
    localStorage.setItem("unrelated.key", "value");
    localStorage.setItem("theme.preference", "auto");
    render(<Harness />);
    await waitFor(() => expect(driveMock).toHaveBeenCalledTimes(1));
    expect(localStorage.getItem("unrelated.key")).toBe("value");
    expect(localStorage.getItem("theme.preference")).toBe("auto");
  });
});

describe("RestartTourButton", () => {
  it("clears the completion flag and re-fires the tour", async () => {
    localStorage.setItem(TOUR_COMPLETED_KEY, "1");
    expect(hasCompletedFirstRunTour()).toBe(true);
    render(<RestartTourButton />);
    fireEvent.click(screen.getByRole("button", { name: /restart tour/i }));
    expect(localStorage.getItem(TOUR_COMPLETED_KEY)).toBeNull();
    await waitFor(() => expect(driveMock).toHaveBeenCalledTimes(1));
  });

  it("restarts even when a prior tour left the launch guard set", async () => {
    // Auto-launch the tour. The driver mock never invokes onDestroyed, so the
    // module-level `tourLaunching` guard stays true — mirroring a real session
    // where the user completed/closed the tour and later hits "Restart tour".
    render(<Harness />);
    await waitFor(() => expect(driveMock).toHaveBeenCalledTimes(1));

    render(<RestartTourButton />);
    fireEvent.click(screen.getByRole("button", { name: /restart tour/i }));

    // Restart must bypass the stuck guard and drive a fresh tour.
    await waitFor(() => expect(driveMock).toHaveBeenCalledTimes(2));
  });
});
