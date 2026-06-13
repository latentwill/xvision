import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import {
  MemoryRouter,
  Route,
  Routes,
  useLocation,
  useParams,
} from "react-router-dom";

import { LegacyDiffRedirect, OptimizerRunRedirect } from "./routes";

function HomeProbe() {
  const location = useLocation();
  return <div data-testid="home">{location.search}</div>;
}

// Standalone experiment-report landing. As of lane a1t,
// `/optimizer/experiment/:hash` renders the ExperimentDetail page directly
// instead of redirecting into the owning cycle with `?exp=`. This probe stands
// in for that page so the redirect tests can assert the final landing hash.
function ExperimentProbe() {
  const { hash } = useParams();
  return <div data-testid="experiment">{hash}</div>;
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/optimizer" element={<HomeProbe />} />
        <Route path="/optimizer/experiment/:hash" element={<ExperimentProbe />} />
        <Route path="/optimizer/run/:sessionId" element={<OptimizerRunRedirect />} />
        <Route path="/autooptimizer/diff/:hash" element={<LegacyDiffRedirect />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("optimizer redirects", () => {
  it("redirects /optimizer/run/:sessionId to /optimizer?session=<id>", async () => {
    renderAt("/optimizer/run/sess-1");
    const home = await screen.findByTestId("home");
    expect(home).toHaveTextContent("?session=sess-1");
  });

  it("lands /optimizer/experiment/:hash on the standalone experiment report", async () => {
    renderAt("/optimizer/experiment/abcd1234ef");
    const experiment = await screen.findByTestId("experiment");
    expect(experiment).toHaveTextContent("abcd1234ef");
  });

  it("resolves legacy /autooptimizer/diff/:hash to the experiment report", async () => {
    renderAt("/autooptimizer/diff/feedface01");
    await waitFor(() => {
      const experiment = screen.getByTestId("experiment");
      expect(experiment).toHaveTextContent("feedface01");
    });
  });
});
