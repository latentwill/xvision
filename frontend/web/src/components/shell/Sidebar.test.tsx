import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { ThemeProvider } from "@/theme/ThemeProvider";
import { THEME_DARK_KEY } from "@/theme/themes";

function renderSidebar() {
  return render(
    <ThemeProvider>
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    </ThemeProvider>,
  );
}

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.className = "";
  // Reset chart-v2 cookie between tests
  document.cookie =
    "xvn.chartv2=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/";
});

describe("Sidebar theme toggle", () => {
  it("uses Dashboard for the root navigation label", () => {
    renderSidebar();

    expect(screen.getByRole("link", { name: /Dashboard/ })).toHaveAttribute(
      "href",
      "/",
    );
  });

  it("does not render Memory as a sidebar menu item", () => {
    renderSidebar();

    expect(screen.queryByRole("link", { name: /^Memory$/ })).toBeNull();
  });

  it("switches to light with the sun button", () => {
    renderSidebar();

    fireEvent.click(screen.getByRole("button", { name: "Switch to light theme" }));

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("switches to the remembered dark theme with the moon button", () => {
    localStorage.setItem(THEME_DARK_KEY, "black");
    renderSidebar();

    fireEvent.click(screen.getByRole("button", { name: "Switch to light theme" }));
    fireEvent.click(screen.getByRole("button", { name: "Switch to dark theme" }));

    expect(document.documentElement.dataset.theme).toBe("black");
  });
});

// chart-rework spec Track B B0 — Charts entry (cookie-gated, between
// Scenarios and Eval per §11.1 / §11.4 resolutions).
describe("Sidebar Charts entry (chart-rework Track B)", () => {
  it("hides Charts entry when xvn.chartv2 cookie is absent", () => {
    renderSidebar();
    expect(screen.queryByRole("link", { name: /^Charts$/ })).toBeNull();
  });

  it("shows Charts entry between Scenarios and Eval when cookie is set", () => {
    document.cookie = "xvn.chartv2=1; path=/";
    renderSidebar();

    const labels = screen
      .getAllByRole("link")
      .map((a) => a.textContent?.trim() ?? "");

    const scenariosIdx = labels.indexOf("Scenarios");
    const chartsIdx = labels.indexOf("Charts");
    const evalIdx = labels.indexOf("Eval");

    expect(scenariosIdx).toBeGreaterThanOrEqual(0);
    expect(chartsIdx).toBeGreaterThanOrEqual(0);
    expect(evalIdx).toBeGreaterThanOrEqual(0);
    expect(chartsIdx).toBeGreaterThan(scenariosIdx);
    expect(chartsIdx).toBeLessThan(evalIdx);

    const chartsLink = screen.getByRole("link", { name: /^Charts$/ });
    expect(chartsLink).toHaveAttribute("href", "/charts");
  });
});
