import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import {
  MemoryRouter,
  Route,
  Routes,
  useLocation,
  useParams,
} from "react-router-dom";

import { useLineageNode } from "@/features/autooptimizer/api";
import {
  ExperimentRedirect,
  LegacyDiffRedirect,
  OptimizerRunRedirect,
} from "./routes";

vi.mock("@/features/autooptimizer/api", () => ({
  useLineageNode: vi.fn(),
}));

function HomeProbe() {
  const location = useLocation();
  return <div data-testid="home">{location.search}</div>;
}

function CycleProbe() {
  const { cycleId } = useParams();
  const location = useLocation();
  return (
    <div data-testid="cycle">
      {cycleId}
      {location.search}
    </div>
  );
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/optimizer" element={<HomeProbe />} />
        <Route path="/optimizer/cycle/:cycleId" element={<CycleProbe />} />
        <Route path="/optimizer/experiment/:hash" element={<ExperimentRedirect />} />
        <Route path="/optimizer/run/:sessionId" element={<OptimizerRunRedirect />} />
        <Route path="/autooptimizer/diff/:hash" element={<LegacyDiffRedirect />} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.mocked(useLineageNode).mockReset();
});

describe("optimizer redirects", () => {
  it("redirects /optimizer/run/:sessionId to /optimizer?session=<id>", async () => {
    renderAt("/optimizer/run/sess-1");
    const home = await screen.findByTestId("home");
    expect(home).toHaveTextContent("?session=sess-1");
  });

  it("redirects /optimizer/experiment/:hash to the owning cycle with ?exp=", async () => {
    vi.mocked(useLineageNode).mockReturnValue({
      isLoading: false,
      data: { cycle_id: "cyc-1" },
    } as ReturnType<typeof useLineageNode>);

    renderAt("/optimizer/experiment/abcd1234ef");
    const cycle = await screen.findByTestId("cycle");
    expect(cycle).toHaveTextContent("cyc-1");
    expect(cycle).toHaveTextContent("?exp=abcd1234ef");
    expect(vi.mocked(useLineageNode)).toHaveBeenCalledWith("abcd1234ef");
  });

  it("shows a locating row while the experiment lookup is loading", () => {
    vi.mocked(useLineageNode).mockReturnValue({
      isLoading: true,
      data: undefined,
    } as ReturnType<typeof useLineageNode>);

    renderAt("/optimizer/experiment/abcd1234ef");
    expect(screen.getByText(/Locating experiment…/)).toBeInTheDocument();
  });

  it("falls back to /optimizer when the experiment is unknown", async () => {
    vi.mocked(useLineageNode).mockReturnValue({
      isLoading: false,
      data: undefined,
    } as ReturnType<typeof useLineageNode>);

    renderAt("/optimizer/experiment/deadbeef00");
    expect(await screen.findByTestId("home")).toBeInTheDocument();
  });

  it("resolves legacy /autooptimizer/diff/:hash through the experiment redirect", async () => {
    vi.mocked(useLineageNode).mockReturnValue({
      isLoading: false,
      data: { cycle_id: "cyc-9" },
    } as ReturnType<typeof useLineageNode>);

    renderAt("/autooptimizer/diff/feedface01");
    await waitFor(() => {
      const cycle = screen.getByTestId("cycle");
      expect(cycle).toHaveTextContent("cyc-9");
      expect(cycle).toHaveTextContent("?exp=feedface01");
    });
  });
});
