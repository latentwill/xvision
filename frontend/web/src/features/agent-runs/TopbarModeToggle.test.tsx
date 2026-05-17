import { afterEach, beforeEach, describe, expect, test } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TopbarModeToggle } from "./TopbarModeToggle";
import { useTraceDock } from "@/stores/trace-dock";

describe("TopbarModeToggle", () => {
  beforeEach(() => {
    useTraceDock.setState({
      height: "collapsed",
      selectedSpanId: null,
      activeRunId: null,
      mode: "post-hoc",
      lastOpenHeight: "working",
    });
  });
  afterEach(() => cleanup());

  test("renders nothing when activeRunId is null", () => {
    render(<TopbarModeToggle />);
    expect(screen.queryByTestId("topbar-mode-toggle")).toBeNull();
  });

  test("shows POST-HOC active when mode=post-hoc and activeRunId set", () => {
    useTraceDock.setState({ activeRunId: "run_abc", mode: "post-hoc" });
    render(<TopbarModeToggle />);
    expect(screen.getByRole("button", { name: /post-hoc/i })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /live/i })).toHaveAttribute("aria-pressed", "false");
  });

  test("clicking LIVE switches mode and pulses the dot", async () => {
    useTraceDock.setState({ activeRunId: "run_abc", mode: "post-hoc" });
    render(<TopbarModeToggle />);
    await userEvent.click(screen.getByRole("button", { name: /live/i }));
    expect(useTraceDock.getState().mode).toBe("live");
  });

  test("clicking POST-HOC switches back", async () => {
    useTraceDock.setState({ activeRunId: "run_abc", mode: "live" });
    render(<TopbarModeToggle />);
    await userEvent.click(screen.getByRole("button", { name: /post-hoc/i }));
    expect(useTraceDock.getState().mode).toBe("post-hoc");
  });
});
