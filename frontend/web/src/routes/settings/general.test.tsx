import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ThemeProvider } from "@/theme/ThemeProvider";
import { SettingsGeneralRoute } from "./general";

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.className = "";
});

describe("SettingsGeneralRoute", () => {
  it("renders all appearance choices and persists selection", () => {
    render(
      <ThemeProvider>
        <SettingsGeneralRoute />
      </ThemeProvider>,
    );

    expect(
      screen.getByRole("heading", { name: "Appearance" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Auto" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Light" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Folio dark" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Black" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: "Black" }));
    expect(document.documentElement.dataset.theme).toBe("black");
  });
});
