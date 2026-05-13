import { describe, expect, it } from "vitest";

import { STATIC_ACTIONS } from "./CommandPalette";

describe("CommandPalette static actions", () => {
  it("names the root route as Dashboard", () => {
    const home = STATIC_ACTIONS.find((a) => a.artifact_id === "nav:home");

    expect(home).toMatchObject({
      title: "Dashboard",
      summary: "Workspace status at a glance",
      href: "/",
    });
  });
});
