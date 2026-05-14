import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ThemeProvider } from "./ThemeProvider";
import { THEME_DARK_KEY, THEME_PREFERENCE_KEY } from "./themes";
import { useTheme } from "./useTheme";

function installMatchMedia(matches: boolean) {
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  const query = {
    matches,
    media: "(prefers-color-scheme: dark)",
    onchange: null,
    addEventListener: vi.fn(
      (_: "change", cb: (event: MediaQueryListEvent) => void) => {
        listeners.add(cb);
      },
    ),
    removeEventListener: vi.fn(
      (_: "change", cb: (event: MediaQueryListEvent) => void) => {
        listeners.delete(cb);
      },
    ),
    dispatch(next: boolean) {
      query.matches = next;
      listeners.forEach((cb) =>
        cb({ matches: next } as MediaQueryListEvent),
      );
    },
  };
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn().mockReturnValue(query),
  });
  return query;
}

function Probe() {
  const {
    preference,
    resolvedTheme,
    setDarkTheme,
    setLightTheme,
    setPreference,
  } = useTheme();
  return (
    <div>
      <div data-testid="preference">{preference}</div>
      <div data-testid="resolved">{resolvedTheme}</div>
      <button type="button" onClick={() => setPreference("black")}>
        Black
      </button>
      <button type="button" onClick={() => setPreference("auto")}>
        Auto
      </button>
      <button type="button" onClick={setLightTheme}>
        Sun
      </button>
      <button type="button" onClick={setDarkTheme}>
        Moon
      </button>
    </div>
  );
}

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.className = "";
  document
    .querySelector('meta[name="theme-color"]')
    ?.setAttribute("content", "#0F0E0C");
  vi.restoreAllMocks();
});

describe("ThemeProvider", () => {
  it("defaults to folio dark and applies DOM attributes", () => {
    installMatchMedia(true);

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    expect(screen.getByTestId("preference")).toHaveTextContent("folio-dark");
    expect(document.documentElement.dataset.theme).toBe("folio-dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("persists explicit preferences and remembers black as dark theme", () => {
    installMatchMedia(true);

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Black" }));
    expect(localStorage.getItem(THEME_PREFERENCE_KEY)).toBe("black");
    expect(localStorage.getItem(THEME_DARK_KEY)).toBe("black");
    expect(document.documentElement.dataset.theme).toBe("black");
  });

  it("uses sidebar-style sun and moon actions", () => {
    installMatchMedia(true);
    localStorage.setItem(THEME_DARK_KEY, "black");

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Sun" }));
    expect(screen.getByTestId("resolved")).toHaveTextContent("light");
    fireEvent.click(screen.getByRole("button", { name: "Moon" }));
    expect(screen.getByTestId("resolved")).toHaveTextContent("black");
  });

  it("updates auto when browser color scheme changes", () => {
    const query = installMatchMedia(false);

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Auto" }));
    expect(screen.getByTestId("resolved")).toHaveTextContent("light");
    act(() => {
      query.dispatch(true);
    });
    expect(screen.getByTestId("resolved")).toHaveTextContent("folio-dark");
  });
});
