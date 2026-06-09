import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MarketplaceLayout } from "./MarketplaceLayout";

afterEach(() => {
  cleanup();
  localStorage.clear();
});

function renderAt(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/marketplace" element={<MarketplaceLayout />}>
            <Route index element={<div>browse-content</div>} />
          </Route>
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("MarketplaceLayout", () => {
  it("renders marketplace content unconditionally", async () => {
    renderAt("/marketplace");
    expect(await screen.findByText("browse-content")).toBeInTheDocument();
  });

  it("renders the testnet banner", async () => {
    renderAt("/marketplace");
    expect(screen.getByText(/Mantle Sepolia testnet/i)).toBeInTheDocument();
  });
});
