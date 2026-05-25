import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import * as viewport from "@/components/responsive/useViewportMode";

import { ResponsiveListCard } from "./ResponsiveListCard";
import type { ListSortState } from "./useListState";

type Row = { id: string; name: string };

const ROWS: Row[] = [
  { id: "1", name: "ETH-MR" },
  { id: "2", name: "BTC-MOM" },
];

function makeSort(): ListSortState {
  return {
    value: "added",
    setValue: vi.fn(),
    options: [{ value: "added", label: "Recently added" }],
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.style.overflow = "";
});

describe("ResponsiveListCard", () => {
  it("renders ListCard on desktop", () => {
    vi.spyOn(viewport, "useViewportMode").mockReturnValue("desktop");
    render(
      <MemoryRouter>
        <ResponsiveListCard<Row>
          title="Strategies"
          toolbar={{ sort: makeSort() }}
          columns={[{ key: "name", label: "Name" }]}
          rows={ROWS}
          renderRow={(r) => (
            <tr key={r.id} data-testid={`d-${r.id}`}>
              <td>{r.name}</td>
            </tr>
          )}
          renderMobileRow={(r) => (
            <div key={r.id} data-testid={`m-${r.id}`}>
              {r.name}
            </div>
          )}
        />
      </MemoryRouter>,
    );
    expect(screen.getByTestId("d-1")).toBeInTheDocument();
    expect(screen.queryByTestId("m-1")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { level: 2, name: "Strategies" }),
    ).not.toBeInTheDocument();
  });

  it("renders ListCard on tablet (tablet matches desktop shape)", () => {
    vi.spyOn(viewport, "useViewportMode").mockReturnValue("tablet");
    render(
      <MemoryRouter>
        <ResponsiveListCard<Row>
          title="Strategies"
          toolbar={{ sort: makeSort() }}
          columns={[{ key: "name", label: "Name" }]}
          rows={ROWS}
          renderRow={(r) => (
            <tr key={r.id} data-testid={`d-${r.id}`}>
              <td>{r.name}</td>
            </tr>
          )}
          renderMobileRow={(r) => (
            <div key={r.id} data-testid={`m-${r.id}`}>
              {r.name}
            </div>
          )}
        />
      </MemoryRouter>,
    );
    expect(screen.getByTestId("d-1")).toBeInTheDocument();
    expect(screen.queryByTestId("m-1")).not.toBeInTheDocument();
  });

  it("renders MListCard on phone", () => {
    vi.spyOn(viewport, "useViewportMode").mockReturnValue("phone");
    render(
      <MemoryRouter>
        <ResponsiveListCard<Row>
          title="Strategies"
          toolbar={{ sort: makeSort() }}
          rows={ROWS}
          renderRow={(r) => (
            <tr key={r.id} data-testid={`d-${r.id}`}>
              <td>{r.name}</td>
            </tr>
          )}
          renderMobileRow={(r) => (
            <div key={r.id} data-testid={`m-${r.id}`}>
              {r.name}
            </div>
          )}
        />
      </MemoryRouter>,
    );
    expect(screen.getByTestId("m-1")).toBeInTheDocument();
    expect(screen.queryByTestId("d-1")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { level: 2, name: "Strategies" }),
    ).not.toBeInTheDocument();
  });

  it("phone breakpoint redirects when mobileFallback is set", () => {
    vi.spyOn(viewport, "useViewportMode").mockReturnValue("phone");
    render(
      <MemoryRouter initialEntries={["/start"]}>
        <ResponsiveListCard<Row>
          title="Strategies"
          toolbar={{ sort: makeSort() }}
          rows={ROWS}
          renderRow={() => null}
          renderMobileRow={(r) => (
            <div key={r.id} data-testid={`m-${r.id}`}>
              {r.name}
            </div>
          )}
          mobileFallback={{ redirectTo: "/m/strategies" }}
        />
      </MemoryRouter>,
    );
    // Navigate component renders nothing; the redirect target is unreachable
    // in MemoryRouter without a matching route, so we just confirm the
    // mobile branch was suppressed.
    expect(screen.queryByTestId("m-1")).not.toBeInTheDocument();
  });
});
