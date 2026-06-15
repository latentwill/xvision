import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { SettingsLayout } from "./index";

afterEach(cleanup);

describe("SettingsLayout", () => {
  it("does not expose Skills as a settings tab", () => {
    render(
      <MemoryRouter initialEntries={["/settings/providers"]}>
        <QueryClientProvider client={new QueryClient()}>
          <Routes>
            <Route path="/settings" element={<SettingsLayout />}>
              <Route path="providers" element={<div />} />
            </Route>
          </Routes>
        </QueryClientProvider>
      </MemoryRouter>,
    );

    expect(screen.queryByRole("link", { name: "Skills" })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "General" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Providers" })).toBeInTheDocument();
  });

  it("does not expose Tool policy as a settings tab", () => {
    render(
      <MemoryRouter initialEntries={["/settings/providers"]}>
        <QueryClientProvider client={new QueryClient()}>
          <Routes>
            <Route path="/settings" element={<SettingsLayout />}>
              <Route path="providers" element={<div />} />
            </Route>
          </Routes>
        </QueryClientProvider>
      </MemoryRouter>,
    );

    expect(screen.queryByRole("link", { name: "Tool policy" })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "General" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Providers" })).toBeInTheDocument();
  });

  it("does not expose Identity as a settings tab", () => {
    render(
      <MemoryRouter initialEntries={["/settings/providers"]}>
        <QueryClientProvider client={new QueryClient()}>
          <Routes>
            <Route path="/settings" element={<SettingsLayout />}>
              <Route path="providers" element={<div />} />
            </Route>
          </Routes>
        </QueryClientProvider>
      </MemoryRouter>,
    );

    expect(screen.queryByRole("link", { name: "Identity" })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Wallet" })).toBeInTheDocument();
  });

  it("does not expose Marketplace as a settings tab (QA: tab removed)", () => {
    render(
      <MemoryRouter initialEntries={["/settings/providers"]}>
        <QueryClientProvider client={new QueryClient()}>
          <Routes>
            <Route path="/settings" element={<SettingsLayout />}>
              <Route path="providers" element={<div />} />
            </Route>
          </Routes>
        </QueryClientProvider>
      </MemoryRouter>,
    );

    expect(
      screen.queryByRole("link", { name: "Marketplace" }),
    ).not.toBeInTheDocument();
    // The remaining tabs still render.
    expect(screen.getByRole("link", { name: "Brokers" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Danger zone" })).toBeInTheDocument();
  });
});
