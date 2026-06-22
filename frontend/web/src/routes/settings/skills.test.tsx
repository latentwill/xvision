// Smoke tests for the Skills settings route after the migration to
// `<ResponsiveListCard>` + `useListState` + `useListUrlState`
// (cli-operator-safety wave / docs-lists-metric-polish track
// `list-search-filter-missing-surfaces`).
//
// Coverage focus:
//  - empty state renders the "No skills yet" message
//  - populated state lists every skill row
//  - search box filters the visible rows
//  - kind filter chip narrows to one kind

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

import { ThemeProvider } from "@/theme/ThemeProvider";
import { SettingsSkillsRoute } from "./skills";
import type { Skill } from "@/api/skills";
import type * as SkillsApiModule from "@/api/skills";

vi.mock("@/api/skills", async () => {
  const actual = await vi.importActual<typeof SkillsApiModule>(
    "@/api/skills",
  );
  return {
    ...actual,
    listSkills: vi.fn(),
  };
});

const skillsApi = await import("@/api/skills");

function mkSkill(overrides: Partial<Skill>): Skill {
  return {
    skill_id: "sk_default",
    name: "default-skill",
    description: "default description",
    kind: "tool",
    config: {},
    archived: false,
    created_at: "2026-05-20T10:00:00Z",
    updated_at: "2026-05-20T10:00:00Z",
    ...overrides,
  };
}

// `<ResponsiveListCard>` reads `useViewportMode()` which calls
// `window.matchMedia`. jsdom doesn't provide it; install a desktop
// stub so the route mounts.
function stubMatchMediaDesktop() {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => ({
      matches: query.includes("min-width: 1280px"),
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

function renderRoute() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <ThemeProvider>
          <SettingsSkillsRoute />
        </ThemeProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("SettingsSkillsRoute", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    stubMatchMediaDesktop();
  });
  afterEach(() => {
    cleanup();
  });

  it("renders the empty state when listSkills returns []", async () => {
    vi.mocked(skillsApi.listSkills).mockResolvedValue([]);
    renderRoute();
    expect(
      await screen.findByText(/No skills yet/i),
    ).toBeInTheDocument();
  });

  it("selects the create form skill kind through the styled menu", async () => {
    const user = userEvent.setup();
    vi.mocked(skillsApi.listSkills).mockResolvedValue([]);

    renderRoute();
    await user.click(await screen.findByRole("button", { name: "+ Add skill" }));
    const form = screen.getByText("Create skill").closest("div")!.parentElement!;
    await user.click(within(form).getByRole("button", { name: /kind/i }));
    await user.click(await screen.findByRole("option", { name: /Evaluator/i }));

    expect(within(form).getByText("Evaluator")).toBeInTheDocument();
  });

  it("lists every returned skill by name", async () => {
    vi.mocked(skillsApi.listSkills).mockResolvedValue([
      mkSkill({ skill_id: "sk_one", name: "rsi-tool", kind: "tool" }),
      mkSkill({
        skill_id: "sk_two",
        name: "macd-fragment",
        kind: "prompt_fragment",
      }),
      mkSkill({
        skill_id: "sk_three",
        name: "risk-evaluator",
        kind: "evaluator",
      }),
    ]);
    renderRoute();
    expect(await screen.findByText("rsi-tool")).toBeInTheDocument();
    expect(screen.getByText("macd-fragment")).toBeInTheDocument();
    expect(screen.getByText("risk-evaluator")).toBeInTheDocument();
  });

  it("filters the visible list via the search box", async () => {
    vi.mocked(skillsApi.listSkills).mockResolvedValue([
      mkSkill({ skill_id: "sk_one", name: "rsi-tool", kind: "tool" }),
      mkSkill({
        skill_id: "sk_two",
        name: "macd-fragment",
        kind: "prompt_fragment",
      }),
    ]);
    renderRoute();
    await screen.findByText("rsi-tool");

    const search = screen.getByPlaceholderText(/Search name or description/i);
    fireEvent.change(search, { target: { value: "macd" } });

    expect(screen.queryByText("rsi-tool")).not.toBeInTheDocument();
    expect(screen.getByText("macd-fragment")).toBeInTheDocument();
  });

  // Regression: toggling a column off must hide BOTH its header and its body
  // cells. The strategies fix (0bc34fa) threaded `visibleKeys` into row
  // renderers but missed this Skills route — so the body cell stayed visible
  // while only the title disappeared (the EJ QA finding).
  it("hides body cells for a column toggled off via columnState", async () => {
    // useListColumns persists visible keys at `xvn:list:<listId>:columns`.
    // Omit "description" so that column is hidden; essential keys (name,
    // actions) are always re-added by the hook.
    window.localStorage.setItem(
      "xvn:list:settings-skills:columns",
      JSON.stringify(["name", "kind", "actions"]),
    );

    vi.mocked(skillsApi.listSkills).mockResolvedValue([
      mkSkill({
        skill_id: "sk_one",
        name: "rsi-tool",
        kind: "tool",
        description: "uniq-desc-xyz",
      }),
    ]);

    renderRoute();
    await screen.findByText("rsi-tool");

    // The "Description" header must NOT be rendered.
    expect(
      screen.queryByRole("columnheader", { name: /^Description$/i }),
    ).not.toBeInTheDocument();

    // …and neither must the description value as a body cell.
    expect(screen.queryByText("uniq-desc-xyz")).not.toBeInTheDocument();

    window.localStorage.removeItem("xvn:list:settings-skills:columns");
  });
});
