import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

import { AgentDetailTabs } from "./AgentDetailTabs";

vi.mock("./AgentForm", () => ({
  AgentForm: () => <div>configuration-panel</div>,
}));

vi.mock("./MemoryTab", () => ({
  MemoryTab: () => <div>memory-panel</div>,
}));

vi.mock("@/components/diagnostics/AgentDiagnosticsView", () => ({
  AgentDiagnosticsView: () => <div>diagnostics-panel</div>,
}));

function renderTabs(path = "/agents/ag-1") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <AgentDetailTabs agentId="ag-1" />
    </MemoryRouter>,
  );
}

describe("AgentDetailTabs", () => {
  it("does not expose Diagnostics as an agent detail tab", () => {
    renderTabs();

    expect(
      screen.getByRole("tab", { name: "Configuration" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Memory" })).toBeInTheDocument();
    expect(
      screen.queryByRole("tab", { name: "Diagnostics" }),
    ).not.toBeInTheDocument();
  });

  it("does not render Diagnostics from the legacy query tab", () => {
    renderTabs("/agents/ag-1?tab=diagnostics");

    expect(screen.getByText("configuration-panel")).toBeInTheDocument();
    expect(screen.queryByText("diagnostics-panel")).not.toBeInTheDocument();
  });
});
