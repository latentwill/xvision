// frontend/web/src/components/home/DeployReadinessStrip.test.tsx
//
// Component spec for the deploy-readiness strip (xvision-e17). Verifies the
// pass / fail / unknown rendering, the collapse-to-one-line behavior when all
// checks pass, routed fix links on failures, the literal check labels, and the
// no-popup invariant (no role="dialog").

import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

import { DeployReadinessStrip } from "./DeployReadinessStrip";
import type { ReadinessCheck } from "@/features/home/deploy-readiness";

function renderStrip(checks: ReadinessCheck[]) {
  return render(
    <MemoryRouter>
      <DeployReadinessStrip checks={checks} />
    </MemoryRouter>,
  );
}

const PASS_ALL: ReadinessCheck[] = [
  { id: "keys", label: "keys", status: "pass", detail: "provider keys present" },
  { id: "broker", label: "broker", status: "pass", detail: "configured and reachable" },
  { id: "no-blocking-eval", label: "no blocking eval", status: "pass", detail: "no blocking eval" },
];

describe("DeployReadinessStrip", () => {
  it("returns null when given no checks (nothing fetched yet)", () => {
    const { container } = renderStrip([]);
    expect(container.firstChild).toBeNull();
  });

  it("collapses to a single 'Ready to deploy' line when every check passes", () => {
    renderStrip(PASS_ALL);
    const strip = screen.getByTestId("deploy-readiness-strip");
    expect(strip.textContent).toMatch(/ready to deploy/i);
    // The collapsed line must not enumerate the individual checks.
    expect(strip.textContent).not.toMatch(/no blocking eval/);
  });

  it("renders one row per check with the literal labels when not all pass", () => {
    const checks: ReadinessCheck[] = [
      { id: "keys", label: "keys", status: "pass", detail: "provider keys present" },
      { id: "broker", label: "broker", status: "pass", detail: "configured and reachable" },
      {
        id: "no-blocking-eval",
        label: "no blocking eval",
        status: "fail",
        detail: "safety paused",
        link: { to: "/safety", label: "resume" },
      },
    ];
    renderStrip(checks);
    const strip = screen.getByTestId("deploy-readiness-strip");
    expect(strip.textContent).toContain("keys");
    expect(strip.textContent).toContain("broker");
    expect(strip.textContent).toContain("no blocking eval");
    // Literal pass / fail glyphs are present.
    expect(strip.textContent).toContain("✓");
    expect(strip.textContent).toContain("✗");
  });

  it("renders a routed fix Link for a failing check", () => {
    const checks: ReadinessCheck[] = [
      ...PASS_ALL.slice(0, 2),
      {
        id: "no-blocking-eval",
        label: "no blocking eval",
        status: "fail",
        detail: "safety paused",
        link: { to: "/safety", label: "resume" },
      },
    ];
    renderStrip(checks);
    const link = screen.getByRole("link", { name: /resume/i });
    expect(link).toHaveAttribute("href", "/safety");
  });

  it("renders an unknown check with the em-dash glyph and no crash", () => {
    const checks: ReadinessCheck[] = [
      { id: "keys", label: "keys", status: "unknown", detail: "checking provider keys…" },
      { id: "broker", label: "broker", status: "pass", detail: "configured and reachable" },
      { id: "no-blocking-eval", label: "no blocking eval", status: "pass", detail: "no blocking eval" },
    ];
    renderStrip(checks);
    const strip = screen.getByTestId("deploy-readiness-strip");
    // Em-dash glyph for unknown, not a ✓/✗.
    expect(strip.textContent).toContain("—");
  });

  it("shows the per-check detail text for a failing broker", () => {
    const checks: ReadinessCheck[] = [
      PASS_ALL[0],
      {
        id: "broker",
        label: "broker",
        status: "fail",
        detail: "unreachable — 401 unauthorized",
        link: { to: "/settings/brokers", label: "configure" },
      },
      PASS_ALL[2],
    ];
    renderStrip(checks);
    const strip = screen.getByTestId("deploy-readiness-strip");
    expect(strip.textContent).toMatch(/unreachable/);
  });

  it("never renders a focus-stealing dialog/overlay (no popups rule)", () => {
    renderStrip([
      PASS_ALL[0],
      {
        id: "broker",
        label: "broker",
        status: "fail",
        detail: "unreachable",
        link: { to: "/settings/brokers", label: "configure" },
      },
      PASS_ALL[2],
    ]);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("marks tone dots per status for the styling contract", () => {
    const checks: ReadinessCheck[] = [
      { id: "keys", label: "keys", status: "pass", detail: "ok" },
      {
        id: "broker",
        label: "broker",
        status: "fail",
        detail: "unreachable",
        link: { to: "/settings/brokers", label: "configure" },
      },
      { id: "no-blocking-eval", label: "no blocking eval", status: "unknown", detail: "checking…" },
    ];
    renderStrip(checks);
    const strip = screen.getByTestId("deploy-readiness-strip");
    expect(within(strip).getByTestId("readiness-row-keys").getAttribute("data-status")).toBe("pass");
    expect(within(strip).getByTestId("readiness-row-broker").getAttribute("data-status")).toBe("fail");
    expect(
      within(strip).getByTestId("readiness-row-no-blocking-eval").getAttribute("data-status"),
    ).toBe("unknown");
  });
});
