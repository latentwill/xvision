/**
 * Regression guard for Finding #11: the driver.js overlay must NOT intercept
 * clicks on non-highlighted elements.
 *
 * jsdom does not apply real CSS stylesheets, so we use two complementary
 * assertions:
 *
 *   1. Config-level: the `tourThemeConfig` object passed to `driver()` does
 *      not set options that block background interaction
 *      (e.g. `disableActiveInteraction` traps clicks on the active element
 *      but the overlay itself remains blocking unless CSS overrides it).
 *
 *   2. CSS-level: `tour-theme.css` contains an explicit `pointer-events: none`
 *      rule for `.driver-overlay` (the native driver.js backdrop that would
 *      otherwise swallow all pointer events).  The rule must also NOT suppress
 *      pointer-events on `.driver-popover` (the coachmark, which must stay
 *      clickable for X / Next / Prev / Esc to work).
 *
 * These two assertions together guarantee:
 *   - the config alone is not hiding a blocking flag, AND
 *   - the CSS ships the passthrough rule that a real browser needs to stop
 *     the overlay from intercepting clicks.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { firstRunTourSteps } from "./steps";
import { tourThemeConfig } from "./tour-theme";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const CSS_PATH = resolve(__dirname, "tour-theme.css");
const cssText = readFileSync(CSS_PATH, "utf8");

// ---------------------------------------------------------------------------
// 1. Config-level: tourThemeConfig must not set click-blocking driver options
// ---------------------------------------------------------------------------

describe("tourThemeConfig — overlay passthrough (config-level)", () => {
  it("does not set disableActiveInteraction (which traps pointer events on the active element)", () => {
    const config = tourThemeConfig(firstRunTourSteps);
    // Teardown: clean up any DOM side-effects from tourThemeConfig
    config.__teardown();
    expect(
      (config as Record<string, unknown>).disableActiveInteraction,
    ).toBeFalsy();
  });

  it("keeps allowClose true so the X button and Esc remain functional", () => {
    const config = tourThemeConfig(firstRunTourSteps);
    config.__teardown();
    // allowClose is set in useFirstRunTour.ts, not in tourThemeConfig itself,
    // so we verify tourThemeConfig does NOT override it to false.
    const val = (config as Record<string, unknown>).allowClose;
    // Either unset (undefined — caller supplies it) or explicitly true.
    expect(val === undefined || val === true).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// 2. CSS-level: .driver-overlay must carry pointer-events: none
// ---------------------------------------------------------------------------

describe("tour-theme.css — overlay passthrough (CSS-level)", () => {
  it("sets pointer-events: none on .driver-overlay so the backdrop does not swallow clicks", () => {
    // Match a .driver-overlay rule block that contains pointer-events: none.
    // We look for the selector and the property anywhere in the file (order-
    // independent), but require they appear inside the same rule block.
    // Simple heuristic: find `.driver-overlay {` then scan forward for the
    // property before the closing `}`.
    const overlayBlockMatch = cssText.match(
      /\.driver-overlay\s*\{([^}]*)\}/s,
    );
    expect(
      overlayBlockMatch,
      ".driver-overlay rule block not found in tour-theme.css",
    ).not.toBeNull();

    const blockBody = overlayBlockMatch![1];
    expect(
      blockBody,
      ".driver-overlay rule block does not contain 'pointer-events: none'",
    ).toMatch(/pointer-events\s*:\s*none/);
  });

  it("sets pointer-events: none !important on the .driver-overlay path (overrides driver.js's inline style on the SVG backdrop)", () => {
    // driver.js sets `pointer-events: auto` as an INLINE style on the <path>
    // backdrop. An inline style outranks a class selector, so ONLY an
    // `!important` rule targeting the path overrides it. Without this rule the
    // backdrop stays click-blocking despite `.driver-overlay { pointer-events:
    // none }`. This is the rule that actually fixes Finding #11 — guard it.
    const pathBlockMatch = cssText.match(
      /\.driver-overlay\s+path\s*\{([^}]*)\}/s,
    );
    expect(
      pathBlockMatch,
      ".driver-overlay path rule block not found in tour-theme.css",
    ).not.toBeNull();

    const pathBody = pathBlockMatch![1];
    expect(
      pathBody,
      ".driver-overlay path rule must set 'pointer-events: none !important' to beat the inline style",
    ).toMatch(/pointer-events\s*:\s*none\s*!important/);
  });

  it("does NOT suppress pointer-events on the .driver-popover root (close/next/prev buttons must stay clickable)", () => {
    // Find only the .driver-popover root rule (not nested selectors like
    // .driver-popover-footer button:disabled which correctly have
    // pointer-events: none for disabled states).
    // We match `.driver-popover` as the COMPLETE selector (possibly with
    // a class qualifier like .xvn-tour.driver-popover) — but NOT rules whose
    // selector contains an additional descendant element (space + word).
    //
    // The .driver-popover-footer button:disabled rule intentionally has
    // pointer-events: none (correct UX for disabled buttons); that is fine.
    // What we guard against is a blanket pointer-events: none on the popover
    // container itself, which would prevent any click on the coachmark.
    const directPopoverBlocks = [
      ...cssText.matchAll(/(?:^|[\s,])\.(?:[\w-]+\.)?driver-popover\s*\{([^}]*)\}/gms),
    ];

    for (const match of directPopoverBlocks) {
      const blockBody = match[1];
      expect(
        blockBody,
        "A .driver-popover root rule block sets pointer-events: none — this would break the close/next/prev buttons",
      ).not.toMatch(/pointer-events\s*:\s*none/);
    }
  });

  it("the existing #xvn-spotlight rule already carries pointer-events: none (non-regression)", () => {
    // This was already correct; verify we did not accidentally remove it.
    const spotlightBlockMatch = cssText.match(
      /#xvn-spotlight\s*\{([^}]*)\}/s,
    );
    expect(
      spotlightBlockMatch,
      "#xvn-spotlight rule block not found",
    ).not.toBeNull();
    expect(spotlightBlockMatch![1]).toMatch(/pointer-events\s*:\s*none/);
  });
});
