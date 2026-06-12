import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";

import { SettingsDangerRoute } from "./danger";

function renderRoute() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <SettingsDangerRoute />
    </QueryClientProvider>,
  );
}

describe("SettingsDangerRoute", () => {
  it("does not mention four canonical seed scenarios in the danger zone", () => {
    renderRoute();

    expect(screen.getByText(/Preserved:/)).toBeInTheDocument();
    expect(screen.queryByText(/four canonical/i)).not.toBeInTheDocument();
  });
});
