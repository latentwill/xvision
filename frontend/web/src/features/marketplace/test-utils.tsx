// src/features/marketplace/test-utils.tsx
// Shared test helper for all marketplace route tests.
// Per integration addendum §2: renderMarketplace wraps QueryClient + DataProvider + MemoryRouter.
import { type ReactElement, type ReactNode } from "react";
import { render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { MarketplaceDataProvider } from "./data/provider";
import { FixtureMarketplaceData } from "./data/MarketplaceData";

export function renderMarketplace(
  ui: ReactElement,
  { path = "/", route = "/" }: { path?: string; route?: string } = {},
) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
        <MemoryRouter initialEntries={[route]}>
          <Routes>
            <Route path={path} element={ui} />
          </Routes>
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>,
  );
}
