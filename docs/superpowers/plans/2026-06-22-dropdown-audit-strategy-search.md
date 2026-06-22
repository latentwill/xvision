# Dropdown Audit and Strategy Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring every xvision dropdown/menu/select/combobox into one Signal-styled control vocabulary, and make every strategy dropdown searchable by text.

**Architecture:** Extend the existing `SignalMenu` portal/fixed-position menu system instead of adding a new dependency. Add a reusable searchable select primitive, then layer a `StrategyPicker` adapter on top for `StrategyListItem` options. Migrate strategy entity selectors first, then native enum selects and bespoke comboboxes in focused batches.

**Tech Stack:** Vite, React 18, TypeScript, Tailwind, Vitest, React Testing Library, existing CSS tokens in `frontend/web/src/styles/tokens.css`.

## Global Constraints

- Follow `PRODUCT.md`: product UI, calm technical control, WCAG AA.
- Use test-first implementation for behavior changes.
- Do not add new UI dependencies unless the existing primitives cannot satisfy a11y.
- Use existing Signal tokens: `surface-elev`, `surface-card`, `border`, `text`, `text-2`, `text-3`, `gold`/`accent`.
- Every strategy dropdown must include text search over display name and stable id.
- Keep popovers portal/fixed-position; do not render dropdown panels inside scroll containers where they can be clipped.
- Preserve existing API contracts.

---

## File Structure

- Modify `frontend/web/src/components/primitives/SignalMenu.tsx`
  - Add `SignalSearchableSelectMenu` and shared keyboard/filter helpers near the existing model picker.
  - Add visible focus affordances to `MenuShell`-backed triggers while preserving existing token classes.
- Create `frontend/web/src/components/primitives/StrategyPicker.tsx`
  - Strategy-specific wrapper over `SignalSearchableSelectMenu`.
  - Maps `StrategyListItem` to searchable options.
- Create `frontend/web/src/components/primitives/StrategyPicker.test.tsx`
  - Tests search by name/id/hash and keyboard selection.
- Create or modify `frontend/web/src/components/primitives/SignalMenu.test.tsx`
  - Primitive keyboard, no-results, aria-state coverage.
- Modify strategy selector callsites:
  - `frontend/web/src/features/autooptimizer/ui/LaunchPanel.tsx`
  - `frontend/web/src/features/autooptimizer/ui/LaunchPanel.test.tsx`
  - `frontend/web/src/routes/eval-runs.tsx`
  - `frontend/web/src/routes/eval-runs.test.tsx`
- Modify strategy-adjacent agent selectors:
  - `frontend/web/src/routes/authoring.tsx`
  - `frontend/web/src/routes/authoring-risk.test.tsx`
  - `frontend/web/src/components/strategy/InlineFilterComposer.tsx`
  - `frontend/web/src/components/strategy/InlineFilterComposer.test.tsx`
- Migrate remaining native enum dropdowns in batches:
  - `frontend/web/src/components/TimeframeSelect.tsx`
  - `frontend/web/src/components/agent/SlotForm.tsx`
  - `frontend/web/src/components/eval-detail/DecisionsTable.tsx`
  - `frontend/web/src/components/primitives/useServerPagination.tsx`
  - `frontend/web/src/components/scenario/ScenarioForm.tsx`
  - `frontend/web/src/features/autooptimizer/screens/AutoresearcherTab.tsx`
  - `frontend/web/src/features/autooptimizer/ui/NanochatSlotCard.tsx`
  - `frontend/web/src/features/eval-runs/review/AgentPicker.tsx`
  - `frontend/web/src/features/memory/MemorySurface.tsx`
  - `frontend/web/src/routes/agents.tsx`
  - `frontend/web/src/routes/authoring.tsx`
  - `frontend/web/src/routes/eval-compare.tsx`
  - `frontend/web/src/routes/eval-runs.tsx`
  - `frontend/web/src/routes/scenarios-detail.tsx`
  - `frontend/web/src/routes/settings/MemorySettingsCard.tsx`
  - `frontend/web/src/routes/settings/index.tsx`
  - `frontend/web/src/routes/settings/providers.tsx`
  - `frontend/web/src/routes/settings/skills.tsx`
- Modify `frontend/web/src/components/AssetPicker.tsx`
  - Replace local absolute overlay styling with the shared searchable menu or align it to the same tokens/keyboard contract.
- Create `docs/superpowers/audits/2026-06-22-dropdown-inventory.md`
  - Final inventory table with an explicit retained-native-control section that says `None` when all native controls are migrated.

---

### Task 1: Static Dropdown Inventory

**Files:**
- Create: `docs/superpowers/audits/2026-06-22-dropdown-inventory.md`

**Interfaces:**
- Consumes: Existing source files under `frontend/web/src`.
- Produces: Inventory table with `Path`, `Component`, `Current control`, `Options source`, `Strategy-related?`, `Decision`, `Notes`.

- [ ] **Step 1: Search for all dropdown surfaces**

Run these searches and paste every real dropdown/control into the inventory table:

```bash
# Use the harness search tool, not grep/rg.
pattern: <select|role="combobox"|aria-haspopup|aria-expanded|<option|datalist
path: frontend/web/src
```

Expected: matches include `SignalMenu.tsx`, `AssetPicker.tsx`, `TimeframeSelect.tsx`, `SlotForm.tsx`, `ListToolbar.tsx`, `InlineFilterComposer.tsx`, `LaunchPanel.tsx`, `eval-runs.tsx`, `authoring.tsx`, settings routes, memory surfaces, and scenario/eval controls.

- [ ] **Step 2: Write the inventory file**

Use this exact shape:

```markdown
# Dropdown Inventory — 2026-06-22

## Summary

| Category | Count | Decision |
|---|---:|---|
| Signal primitives already compliant | 0 | Keep / improve primitive |
| Searchable strategy/entity pickers | 0 | Migrate first |
| Native enum selects | 0 | Migrate to SignalSelectMenu |
| Bespoke comboboxes | 0 | Align or migrate |
| Retained native controls | 0 | Must include justification |

## Surfaces

| Path | Component/function | Current control | Options source | Strategy-related? | Decision | Notes |
|---|---|---|---|---|---|---|
| `frontend/web/src/features/autooptimizer/ui/LaunchPanel.tsx` | `LaunchPanel` | native `<select id="optimizer-strategy">` | `listStrategies()` | yes | `StrategyPicker` | Parent strategy must search by name/id. |
| `frontend/web/src/routes/eval-runs.tsx` | start eval form | native `<select id="eval-start-strategy">` | `listStrategies()` | yes | `StrategyPicker` | Strategy must search by name/id. |
```

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/audits/2026-06-22-dropdown-inventory.md
git commit -m "docs: inventory dropdown surfaces"
```

---

### Task 2: Searchable Signal Menu Primitive

**Files:**
- Modify: `frontend/web/src/components/primitives/SignalMenu.tsx`
- Create: `frontend/web/src/components/primitives/SignalMenu.test.tsx`

**Interfaces:**
- Produces:
  - `export interface SearchableSelectOption { value: string; label: string; meta?: string; searchText?: string; disabled?: boolean; group?: string; }`
  - `export interface SignalSearchableSelectMenuProps { label?: string; ariaLabel: string; value: string; options: SearchableSelectOption[]; onChange: (value: string) => void; placeholder?: string; searchPlaceholder?: string; emptyHint?: string; loading?: boolean; disabled?: boolean; align?: "left" | "right"; className?: string; minWidth?: number; }`
  - `export function SignalSearchableSelectMenu(props: SignalSearchableSelectMenuProps): JSX.Element`

- [ ] **Step 1: Write failing primitive tests**

Create `frontend/web/src/components/primitives/SignalMenu.test.tsx` with these tests:

```tsx
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SignalSearchableSelectMenu } from "./SignalMenu";

afterEach(() => cleanup());

const OPTIONS = [
  { value: "strat-alpha", label: "Alpha Breakout", meta: "strat-alpha", searchText: "Alpha Breakout strat-alpha hash-a" },
  { value: "strat-beta", label: "Beta Mean Reversion", meta: "strat-beta", searchText: "Beta Mean Reversion strat-beta hash-b" },
];

describe("SignalSearchableSelectMenu", () => {
  it("filters options by search text and selects the match", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();

    render(
      <SignalSearchableSelectMenu
        ariaLabel="Strategy"
        value=""
        options={OPTIONS}
        onChange={onChange}
        placeholder="Pick strategy"
        searchPlaceholder="Search strategies…"
      />,
    );

    await user.click(screen.getByRole("button", { name: "Strategy" }));
    await user.type(screen.getByRole("textbox", { name: "Search Strategy" }), "beta");

    expect(screen.queryByRole("option", { name: /Alpha Breakout/i })).not.toBeInTheDocument();
    await user.click(screen.getByRole("option", { name: /Beta Mean Reversion/i }));

    expect(onChange).toHaveBeenCalledWith("strat-beta");
  });

  it("selects the highlighted option with ArrowDown and Enter", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();

    render(
      <SignalSearchableSelectMenu
        ariaLabel="Strategy"
        value=""
        options={OPTIONS}
        onChange={onChange}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Strategy" }));
    await user.keyboard("{ArrowDown}{Enter}");

    expect(onChange).toHaveBeenCalledWith("strat-alpha");
  });

  it("shows no-results copy without rendering stale options", async () => {
    const user = userEvent.setup();

    render(
      <SignalSearchableSelectMenu
        ariaLabel="Strategy"
        value=""
        options={OPTIONS}
        onChange={() => {}}
        emptyHint="No strategies match"
      />,
    );

    await user.click(screen.getByRole("button", { name: "Strategy" }));
    await user.type(screen.getByRole("textbox", { name: "Search Strategy" }), "zzzz");

    expect(screen.getByText("No strategies match")).toBeInTheDocument();
    expect(screen.queryByRole("option")).not.toBeInTheDocument();
  });

  it("exposes expanded state and listbox relationship", async () => {
    const user = userEvent.setup();

    render(
      <SignalSearchableSelectMenu
        ariaLabel="Strategy"
        value=""
        options={OPTIONS}
        onChange={() => {}}
      />,
    );

    const trigger = screen.getByRole("button", { name: "Strategy" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");

    await user.click(trigger);

    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("listbox", { name: "Strategy options" })).toBeInTheDocument();
    expect(within(screen.getByRole("listbox", { name: "Strategy options" })).getAllByRole("option")).toHaveLength(2);
  });
});
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```bash
pnpm vitest run src/components/primitives/SignalMenu.test.tsx
```

Expected: FAIL because `SignalSearchableSelectMenu` is not exported.

- [ ] **Step 3: Implement minimal primitive**

In `SignalMenu.tsx`, add the exported interfaces and component after `SignalSelectMenu` or near `SignalModelPickerMenu`. Use the existing `useSignalMenu`, `MenuShell`, `SignalMenuLabel`, and `Icon`.

Core implementation shape:

```tsx
export interface SearchableSelectOption {
  value: string;
  label: string;
  meta?: string;
  searchText?: string;
  disabled?: boolean;
  group?: string;
}

export interface SignalSearchableSelectMenuProps {
  label?: string;
  ariaLabel: string;
  value: string;
  options: SearchableSelectOption[];
  onChange: (value: string) => void;
  placeholder?: string;
  searchPlaceholder?: string;
  emptyHint?: string;
  loading?: boolean;
  disabled?: boolean;
  align?: "left" | "right";
  className?: string;
  minWidth?: number;
}

export function SignalSearchableSelectMenu({
  label,
  ariaLabel,
  value,
  options,
  onChange,
  placeholder = "Select…",
  searchPlaceholder,
  emptyHint = "No options match",
  loading = false,
  disabled = false,
  align = "left",
  className,
  minWidth = 280,
}: SignalSearchableSelectMenuProps) {
  const { open, setOpen, toggle, triggerRef, menuRef, pos } = useSignalMenu(align);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const selected = options.find((option) => option.value === value);
  const normalized = query.trim().toLowerCase();
  const filtered = normalized
    ? options.filter((option) =>
        (option.searchText ?? `${option.label} ${option.meta ?? ""} ${option.value}`)
          .toLowerCase()
          .includes(normalized),
      )
    : options;
  const enabled = filtered.filter((option) => !option.disabled);
  const listboxId = `signal-searchable-${ariaLabel.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-listbox`;

  useEffect(() => {
    if (open) window.requestAnimationFrame(() => inputRef.current?.focus());
    if (!open) {
      setQuery("");
      setActiveIndex(0);
    }
  }, [open]);

  useEffect(() => setActiveIndex(0), [query]);

  function choose(option: SearchableSelectOption) {
    if (option.disabled) return;
    onChange(option.value);
    setOpen(false);
  }

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((idx) => Math.min(idx + 1, Math.max(enabled.length - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((idx) => Math.max(idx - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const option = enabled[activeIndex];
      if (option) choose(option);
    } else if (e.key === "Escape") {
      setOpen(false);
    }
  }

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        aria-label={ariaLabel}
        disabled={disabled || loading}
        onClick={toggle}
        className={[
          "inline-flex h-8 min-w-0 items-center gap-1.5 rounded-sm border border-border bg-surface-elev px-2.5 text-[12.5px] text-text transition-colors hover:border-text-3 disabled:cursor-not-allowed disabled:opacity-50",
          className ?? "",
        ].join(" ")}
      >
        {label ? <span className="text-[11.5px] tracking-wide text-text-3">{label}</span> : null}
        <span className="min-w-0 flex-1 truncate text-left font-mono text-[12px]">
          {loading ? "Loading…" : selected?.label ?? placeholder}
        </span>
        <Icon name="chevR" size={11} className="text-text-3" />
      </button>
      <MenuShell open={open} menuRef={menuRef as RefObject<HTMLDivElement>} pos={pos} minWidth={minWidth} role="listbox">
        <div className="border-b border-border px-2 pb-1 pt-2">
          <div className="flex h-8 items-center gap-2 rounded-sm border border-border bg-surface-elev px-2 focus-within:border-gold-soft">
            <Icon name="search" size={13} className="shrink-0 text-text-3" />
            <input
              ref={inputRef}
              role="textbox"
              aria-label={`Search ${ariaLabel}`}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={onKeyDown}
              placeholder={searchPlaceholder ?? `Search ${ariaLabel.toLowerCase()}…`}
              spellCheck={false}
              className="min-w-0 flex-1 border-none bg-transparent p-0 font-mono text-[12px] text-text outline-none placeholder:text-text-3"
            />
          </div>
        </div>
        <div id={listboxId} role="listbox" aria-label={`${ariaLabel} options`} className="max-h-[320px] overflow-y-auto py-1">
          {filtered.length === 0 ? (
            <div className="px-3 py-3 font-mono text-[12px] text-text-3">{emptyHint}</div>
          ) : (
            filtered.map((option) => {
              const selectedOption = option.value === value;
              const active = enabled[activeIndex]?.value === option.value;
              return (
                <button
                  key={option.value}
                  type="button"
                  role="option"
                  aria-selected={selectedOption}
                  disabled={option.disabled}
                  onMouseEnter={() => setActiveIndex(Math.max(enabled.findIndex((item) => item.value === option.value), 0))}
                  onClick={() => choose(option)}
                  className={[
                    "flex min-h-[34px] w-full items-center gap-2 px-3 text-left text-[13px] transition-colors disabled:opacity-50",
                    selectedOption ? "bg-gold/10" : active ? "bg-surface-hover" : "hover:bg-surface-hover",
                  ].join(" ")}
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-text">{option.label}</span>
                    {option.meta ? <span className="block truncate font-mono text-[11px] text-text-3">{option.meta}</span> : null}
                  </span>
                  {selectedOption ? <Icon name="check" size={12} className="shrink-0 text-gold" /> : null}
                </button>
              );
            })
          )}
        </div>
      </MenuShell>
    </>
  );
}
```

If `Icon` has no `check` icon, use the inline check SVG already used in `SignalSelectMenu`.

- [ ] **Step 4: Run tests and verify they pass**

Run:

```bash
pnpm vitest run src/components/primitives/SignalMenu.test.tsx
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/components/primitives/SignalMenu.tsx frontend/web/src/components/primitives/SignalMenu.test.tsx
git commit -m "feat: add searchable Signal menu"
```

---

### Task 3: StrategyPicker Adapter

**Files:**
- Create: `frontend/web/src/components/primitives/StrategyPicker.tsx`
- Create: `frontend/web/src/components/primitives/StrategyPicker.test.tsx`

**Interfaces:**
- Consumes: `StrategyListItem` from `@/api/strategies` and `SignalSearchableSelectMenu`.
- Produces:
  - `export interface StrategyPickerProps { strategies: StrategyListItem[]; value: string; onChange: (strategyId: string) => void; ariaLabel?: string; label?: string; placeholder?: string; loading?: boolean; disabled?: boolean; className?: string; }`
  - `export function strategySearchText(strategy: StrategyListItem): string`
  - `export function StrategyPicker(props: StrategyPickerProps): JSX.Element`

- [ ] **Step 1: Write failing adapter tests**

Create `StrategyPicker.test.tsx`:

```tsx
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { StrategyListItem } from "@/api/strategies";
import { StrategyPicker, strategySearchText } from "./StrategyPicker";

afterEach(() => cleanup());

function strategy(overrides: Partial<StrategyListItem>): StrategyListItem {
  return {
    agent_id: "strat-alpha",
    display_name: "Alpha Breakout",
    template: "momentum",
    decision_cadence_minutes: 60,
    tags: ["btc", "trend"],
    bundle_hash: "hash-alpha",
    origin: "user",
    ...overrides,
  };
}

describe("StrategyPicker", () => {
  it("builds search text from name, id, hash, tags, template, and origin", () => {
    expect(strategySearchText(strategy({}))).toContain("Alpha Breakout");
    expect(strategySearchText(strategy({}))).toContain("strat-alpha");
    expect(strategySearchText(strategy({}))).toContain("hash-alpha");
    expect(strategySearchText(strategy({}))).toContain("btc");
    expect(strategySearchText(strategy({}))).toContain("momentum");
    expect(strategySearchText(strategy({}))).toContain("user");
  });

  it("filters by stable id and selects the strategy", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();

    render(
      <StrategyPicker
        strategies={[
          strategy({ agent_id: "strat-alpha", display_name: "Alpha Breakout" }),
          strategy({ agent_id: "strat-beta", display_name: "Beta Mean Reversion", bundle_hash: "bundle-beta" }),
        ]}
        value=""
        onChange={onChange}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Strategy" }));
    await user.type(screen.getByRole("textbox", { name: "Search Strategy" }), "bundle-beta");
    await user.click(screen.getByRole("option", { name: /Beta Mean Reversion/i }));

    expect(onChange).toHaveBeenCalledWith("strat-beta");
  });

  it("shows loading and no-strategies states", async () => {
    const { rerender } = render(
      <StrategyPicker strategies={[]} value="" onChange={() => {}} loading />,
    );

    expect(screen.getByRole("button", { name: "Strategy" })).toHaveTextContent("Loading");

    rerender(<StrategyPicker strategies={[]} value="" onChange={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: "Strategy" }));
    expect(screen.getByText("No strategies available")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```bash
pnpm vitest run src/components/primitives/StrategyPicker.test.tsx
```

Expected: FAIL because `StrategyPicker.tsx` does not exist.

- [ ] **Step 3: Implement adapter**

Create `StrategyPicker.tsx`:

```tsx
import type { StrategyListItem } from "@/api/strategies";
import { SignalSearchableSelectMenu, type SearchableSelectOption } from "./SignalMenu";

export interface StrategyPickerProps {
  strategies: StrategyListItem[];
  value: string;
  onChange: (strategyId: string) => void;
  ariaLabel?: string;
  label?: string;
  placeholder?: string;
  loading?: boolean;
  disabled?: boolean;
  className?: string;
}

export function strategySearchText(strategy: StrategyListItem): string {
  return [
    strategy.display_name,
    strategy.agent_id,
    strategy.bundle_hash,
    strategy.template,
    strategy.origin,
    ...(strategy.tags ?? []),
    ...(strategy.providers ?? []),
    ...(strategy.models ?? []),
    ...(strategy.capabilities ?? []),
    ...(strategy.asset_universe ?? []),
  ]
    .filter(Boolean)
    .join(" ");
}

function toOption(strategy: StrategyListItem): SearchableSelectOption {
  const id = strategy.agent_id;
  const hash = strategy.bundle_hash ? ` · ${strategy.bundle_hash.slice(0, 12)}` : "";
  const origin = strategy.origin ? ` · ${strategy.origin}` : "";
  return {
    value: id,
    label: strategy.display_name || "Untitled strategy",
    meta: `${id}${hash}${origin}`,
    searchText: strategySearchText(strategy),
  };
}

export function StrategyPicker({
  strategies,
  value,
  onChange,
  ariaLabel = "Strategy",
  label,
  placeholder = "— pick a strategy —",
  loading = false,
  disabled = false,
  className,
}: StrategyPickerProps) {
  return (
    <SignalSearchableSelectMenu
      ariaLabel={ariaLabel}
      label={label}
      value={value}
      options={strategies.map(toOption)}
      onChange={onChange}
      placeholder={placeholder}
      searchPlaceholder="Search strategies…"
      emptyHint={strategies.length === 0 ? "No strategies available" : "No strategies match"}
      loading={loading}
      disabled={disabled}
      className={className}
      minWidth={320}
    />
  );
}
```

- [ ] **Step 4: Run tests and verify they pass**

Run:

```bash
pnpm vitest run src/components/primitives/StrategyPicker.test.tsx src/components/primitives/SignalMenu.test.tsx
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/components/primitives/StrategyPicker.tsx frontend/web/src/components/primitives/StrategyPicker.test.tsx
git commit -m "feat: add searchable strategy picker"
```

---

### Task 4: Migrate Strategy Entity Dropdowns

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/ui/LaunchPanel.tsx`
- Modify: `frontend/web/src/features/autooptimizer/ui/LaunchPanel.test.tsx`
- Modify: `frontend/web/src/routes/eval-runs.tsx`
- Modify: `frontend/web/src/routes/eval-runs.test.tsx`

**Interfaces:**
- Consumes: `StrategyPicker` from `@/components/primitives/StrategyPicker`.
- Produces: Searchable strategy selectors for optimizer parent strategy and eval start strategy.

- [ ] **Step 1: Write failing LaunchPanel strategy search test**

In `LaunchPanel.test.tsx`, add a test that opens the parent strategy picker and filters by id:

```tsx
it("searches parent strategies by id before launching", async () => {
  vi.mocked(listStrategies).mockResolvedValue([
    { agent_id: "strategy-1", display_name: "Trend follower", template: "", decision_cadence_minutes: 60 },
    { agent_id: "strategy-2", display_name: "Mean reversion", template: "", decision_cadence_minutes: 60 },
  ]);
  const user = userEvent.setup();
  render(<LaunchPanel />);

  const picker = await screen.findByRole("button", { name: /strategy/i });
  await user.click(picker);
  await user.type(screen.getByRole("textbox", { name: /search strategy/i }), "strategy-2");
  await user.click(screen.getByRole("option", { name: /Mean reversion/i }));

  expect(screen.getByRole("button", { name: /strategy/i })).toHaveTextContent("Mean reversion");
});
```

- [ ] **Step 2: Run LaunchPanel test and verify it fails**

Run:

```bash
pnpm vitest run src/features/autooptimizer/ui/LaunchPanel.test.tsx
```

Expected: FAIL because native select does not expose searchable textbox.

- [ ] **Step 3: Replace LaunchPanel native select**

In `LaunchPanel.tsx`, import the picker:

```tsx
import { StrategyPicker } from "@/components/primitives/StrategyPicker";
```

Replace lines around `id="optimizer-strategy"` with:

```tsx
<StrategyPicker
  strategies={strategies ?? []}
  value={strategyId}
  onChange={setStrategyId}
  loading={strategiesLoading}
  disabled={isRunning || strategiesLoading || noStrategies}
  className="w-full justify-between"
/>
```

Keep the existing label text `Parent strategy`, budget inputs, and disabled launch logic.

- [ ] **Step 4: Write failing eval start strategy search test**

In `eval-runs.test.tsx`, add or update the start-eval test:

```tsx
it("searches the start-eval strategy picker by strategy id", async () => {
  const user = userEvent.setup();
  render(<EvalRunsRoute />);

  const picker = await screen.findByRole("button", { name: "Strategy" });
  await user.click(picker);
  await user.type(screen.getByRole("textbox", { name: "Search Strategy" }), "agent-two");
  await user.click(await screen.findByRole("option", { name: /Second strategy/i }));

  expect(screen.getByRole("button", { name: "Strategy" })).toHaveTextContent("Second strategy");
});
```

Use fixture rows named `First strategy` (`agent-one`) and `Second strategy` (`agent-two`) in the test setup; do not mock the picker.

- [ ] **Step 5: Replace eval start strategy select**

In `eval-runs.tsx`, import `StrategyPicker` and replace `select#eval-start-strategy` with:

```tsx
<StrategyPicker
  strategies={strategies.data ?? []}
  value={agentId}
  onChange={(next) => {
    setAgentId(next);
    setPreflightError(null);
  }}
  loading={strategies.isPending}
  disabled={strategies.isPending}
  className="w-full justify-between"
/>
```

Keep scenario select unchanged in this task.

- [ ] **Step 6: Run targeted tests**

Run:

```bash
pnpm vitest run src/features/autooptimizer/ui/LaunchPanel.test.tsx src/routes/eval-runs.test.tsx src/components/primitives/StrategyPicker.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/features/autooptimizer/ui/LaunchPanel.tsx frontend/web/src/features/autooptimizer/ui/LaunchPanel.test.tsx frontend/web/src/routes/eval-runs.tsx frontend/web/src/routes/eval-runs.test.tsx
git commit -m "feat: search strategy dropdowns"
```

---

### Task 5: Migrate Strategy-Adjacent Agent Pickers

**Files:**
- Modify: `frontend/web/src/routes/authoring.tsx`
- Modify: `frontend/web/src/routes/authoring-risk.test.tsx`
- Modify: `frontend/web/src/components/strategy/InlineFilterComposer.tsx`
- Create: `frontend/web/src/components/strategy/InlineFilterComposer.test.tsx`

**Interfaces:**
- Consumes: `SignalSearchableSelectMenu`.
- Produces: Searchable agent/entity selectors for strategy authoring flows.

- [ ] **Step 1: Write failing authoring existing-agent search test**

In `authoring-risk.test.tsx`, replace native `fireEvent.change` expectations for `Existing agent` with user interaction:

```tsx
const picker = await screen.findByRole("button", { name: /existing agent/i });
await user.click(picker);
await user.type(screen.getByRole("textbox", { name: /search existing agent/i }), "01DEEPSEEK");
await user.click(await screen.findByRole("option", { name: /DeepSeek trader/i }));
```

Expected before implementation: FAIL because the native select has no textbox.

- [ ] **Step 2: Replace authoring existing-agent select**

In `authoring.tsx`, import `SignalSearchableSelectMenu` and replace the `Existing agent` `<select>` with:

```tsx
<SignalSearchableSelectMenu
  ariaLabel="Existing agent"
  value={props.newAgentId}
  onChange={props.setNewAgentId}
  placeholder="Select agent…"
  searchPlaceholder="Search existing agents…"
  emptyHint="No agents match"
  className="w-full justify-between"
  options={props.available.map((agent) => ({
    value: agent.agent_id,
    label: agent.name,
    meta: agent.agent_id,
    searchText: `${agent.name} ${agent.agent_id}`,
  }))}
/>
```

- [ ] **Step 3: Write failing inline filter-agent search test**

Add a test for `InlineFilterComposer` pick mode:

```tsx
it("searches filter-capable agents by name and id", async () => {
  const user = userEvent.setup();
  render(<InlineFilterComposer {...propsWithTwoFilterCandidates} />);

  await user.click(screen.getByRole("button", { name: /filter agent/i }));
  await user.type(screen.getByRole("textbox", { name: /search filter agent/i }), "regime");
  await user.click(screen.getByRole("option", { name: /regime-detector/i }));

  expect(onPickExisting).toHaveBeenCalledWith(expect.objectContaining({ agent_id: "regime-agent" }));
});
```

Use the component's actual callback props; if there is no direct callback until submit, assert the selected description text changes.

- [ ] **Step 4: Replace InlineFilterComposer filter-agent select**

In `InlineFilterComposer.tsx`, replace the pick-mode `<select>` with:

```tsx
<SignalSearchableSelectMenu
  ariaLabel="Filter agent"
  value={pickedAgentId}
  onChange={setPickedAgentId}
  placeholder="Select filter agent…"
  searchPlaceholder="Search filter agents…"
  emptyHint="No filter agents match"
  disabled={editing}
  className="w-full justify-between"
  options={filterCandidates.map((agent) => ({
    value: agent.agent_id,
    label: agent.name,
    meta: `${agent.agent_id}${agent.scope_strategy_id ? " · scoped" : ""}`,
    searchText: `${agent.name} ${agent.agent_id} ${agent.description ?? ""} ${agent.scope_strategy_id ?? ""}`,
  }))}
/>
```

- [ ] **Step 5: Run targeted tests**

Run:

```bash
pnpm vitest run src/routes/authoring-risk.test.tsx src/components/strategy/InlineFilterComposer.test.tsx src/components/primitives/SignalMenu.test.tsx
```

If the inline composer test lives in another file, run that file instead.

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/routes/authoring.tsx frontend/web/src/routes/authoring-risk.test.tsx frontend/web/src/components/strategy/InlineFilterComposer.tsx frontend/web/src/components/strategy/InlineFilterComposer.test.tsx
git commit -m "feat: search strategy authoring entity pickers"
```

---

### Task 6: Migrate Static Native Selects to SignalSelectMenu

**Files:**
- Modify all static enum select files identified in the inventory.
- Do not modify strategy/entity selectors already covered by Tasks 4-5.

**Interfaces:**
- Consumes: `SignalSelectMenu`.
- Produces: Token-styled Signal menus for static enum options.

- [ ] **Step 1: Write one regression test per migrated family**

Use the existing test files listed below. Add user-event menu interaction instead of `fireEvent.change` for these families:

```tsx
await user.click(screen.getByRole("button", { name: /sort/i }));
await user.click(await screen.findByRole("option", { name: /newest/i }));
expect(onChange).toHaveBeenCalledWith("newest");
```

Minimum targeted test files:

```bash
pnpm vitest run \
  src/components/scenario/ScenarioForm.test.tsx \
  src/routes/scenarios-detail.test.tsx \
  src/routes/eval-compare.test.tsx \
  src/components/eval-detail/DecisionsTable.test.tsx \
  src/routes/settings/providers.test.tsx \
  src/routes/settings/skills.test.tsx
```

Expected before implementation: tests that expect Signal menu behavior fail on native selects.

- [ ] **Step 2: Replace native enum selects with SignalSelectMenu**

For each simple string enum select, replace:

```tsx
<select value={value} onChange={(e) => setValue(e.target.value)} className="...">
  <option value="a">A</option>
  <option value="b">B</option>
</select>
```

with:

```tsx
<SignalSelectMenu
  label="Label"
  value={value}
  options={[
    { value: "a", label: "A" },
    { value: "b", label: "B" },
  ]}
  onChange={(next) => setValue(next as typeof value)}
/>
```

Apply this to static enum controls in:

- `components/TimeframeSelect.tsx` — expose the same props, render `SignalSelectMenu` with `STANDARD_TIMEFRAMES`.
- `components/agent/SlotForm.tsx` — provider remains searchable/model-specific where applicable; memory mode becomes `SignalSelectMenu`.
- `components/eval-detail/DecisionsTable.tsx` — sort menu.
- `components/primitives/useServerPagination.tsx` — page size menu.
- `components/scenario/ScenarioForm.tsx` — calendar kind menu.
- `features/autooptimizer/screens/AutoresearcherTab.tsx` — label strategy enum.
- `features/autooptimizer/ui/NanochatSlotCard.tsx` — checkpoint picker may use `SignalSearchableSelectMenu` if checkpoint count can grow; otherwise `SignalSelectMenu`.
- `features/eval-runs/review/AgentPicker.tsx` — review prompt preset enum.
- `routes/authoring.tsx` — pipeline kind, risk direction, close policy kind, timeframe via `TimeframeSelect`.
- `routes/eval-compare.tsx` — compare sort.
- `routes/eval-runs.tsx` — scenario, review provider/model where not using `ModelPicker`.
- `routes/scenarios-detail.tsx` — chart granularity.
- `routes/settings/MemorySettingsCard.tsx` — embedder source and curated embedder model.
- `routes/settings/index.tsx` — degen network.
- `routes/settings/providers.tsx` — provider kind selectors.
- `routes/settings/skills.tsx` — skill kind.

- [ ] **Step 3: Keep or migrate complex native selects**

For controls with dynamic long lists but not strategies, use `SignalSearchableSelectMenu` instead of `SignalSelectMenu` when more than five options are possible:

- `routes/agents.tsx` tools picker.
- `features/memory/MemorySurface.tsx` namespace/lifecycle controls if option set stays short; keep as `SignalSelectMenu`.

If a native select is retained, add a row in `docs/superpowers/audits/2026-06-22-dropdown-inventory.md` with an explicit reason.

- [ ] **Step 4: Run targeted tests**

Run this exact command after migrating the batch:

```bash
pnpm vitest run \
  src/components/scenario/ScenarioForm.test.tsx \
  src/routes/scenarios-detail.test.tsx \
  src/routes/eval-compare.test.tsx \
  src/components/eval-detail/DecisionsTable.test.tsx \
  src/routes/settings/providers.test.tsx \
  src/routes/settings/skills.test.tsx \
  src/components/primitives/SignalMenu.test.tsx
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src docs/superpowers/audits/2026-06-22-dropdown-inventory.md
git commit -m "refactor: standardize static dropdown menus"
```

---

### Task 7: Align Bespoke Comboboxes and Menu A11y

**Files:**
- Modify: `frontend/web/src/components/AssetPicker.tsx`
- Modify: `frontend/web/src/components/primitives/SignalMenu.tsx`
- Modify tests for affected components.

**Interfaces:**
- Consumes: `SignalSearchableSelectMenu` or shared `MenuShell` improvements.
- Produces: consistent keyboard/focus behavior and non-clipped menu surfaces.

- [ ] **Step 1: Write failing AssetPicker keyboard test**

In the existing scenario/detail test or a new `AssetPicker.test.tsx`, add:

```tsx
it("filters assets and selects the highlighted asset with keyboard", async () => {
  const onChange = vi.fn();
  const user = userEvent.setup();
  render(
    <AssetPicker
      assets={[
        { symbol: "BTC/USD", category: "crypto", data: "alpaca" },
        { symbol: "ETH/USD", category: "crypto", data: "alpaca" },
      ]}
      value=""
      onChange={onChange}
    />,
  );

  await user.click(screen.getByRole("combobox", { name: "Asset picker" }));
  await user.type(screen.getByRole("textbox"), "eth");
  await user.keyboard("{ArrowDown}{Enter}");

  expect(onChange).toHaveBeenCalledWith("ETH/USD");
});
```

- [ ] **Step 2: Fix AssetPicker or migrate it**

Preferred: map assets into `SignalSearchableSelectMenu` options and keep the public `AssetPicker` props unchanged:

```tsx
const options = assets.map((asset) => ({
  value: asset.symbol,
  label: asset.symbol,
  meta: asset.category,
  searchText: `${asset.symbol} ${asset.category}`,
}));

return (
  <SignalSearchableSelectMenu
    ariaLabel="Asset picker"
    value={value}
    options={options}
    onChange={onChange}
    placeholder={placeholder}
    searchPlaceholder={placeholder}
    emptyHint="No assets found"
    className={className}
  />
);
```

If the orderly-only badge is required in option rendering, extend `SignalSearchableSelectMenu` with an optional `badge` field instead of keeping a separate overlay implementation.

- [ ] **Step 3: Improve primitive focus states**

Ensure all `SignalMenu` triggers include visible focus styling:

```tsx
"focus:outline-none focus:border-gold/45 focus-visible:ring-1 focus-visible:ring-gold/45"
```

Apply to `SignalActionMenu`, `SignalSelectMenu`, `SignalCheckboxMenu`, `SignalModelPickerMenu`, and `SignalSearchableSelectMenu` triggers.

- [ ] **Step 4: Run targeted tests**

Run:

```bash
pnpm vitest run src/components/AssetPicker.test.tsx src/routes/scenarios-detail.test.tsx src/components/primitives/SignalMenu.test.tsx
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/components/AssetPicker.tsx frontend/web/src/components/AssetPicker.test.tsx frontend/web/src/components/primitives/SignalMenu.tsx
git commit -m "fix: align searchable combobox behavior"
```

---

### Task 8: Visual Verification and Final Cleanup

**Files:**
- Modify: `docs/superpowers/audits/2026-06-22-dropdown-inventory.md`
- No production code changes unless screenshots reveal a concrete issue.

**Interfaces:**
- Consumes: implemented dropdown system.
- Produces: screenshot evidence and final inventory status.

- [ ] **Step 1: Run typecheck and targeted tests**

Run from `frontend/web`:

```bash
pnpm typecheck
pnpm vitest run \
  src/components/primitives/SignalMenu.test.tsx \
  src/components/primitives/StrategyPicker.test.tsx \
  src/features/autooptimizer/ui/LaunchPanel.test.tsx \
  src/routes/eval-runs.test.tsx \
  src/routes/authoring-risk.test.tsx \
  src/routes/scenarios-detail.test.tsx \
  src/routes/settings/providers.test.tsx \
  src/routes/settings/skills.test.tsx
```

Expected: PASS.

- [ ] **Step 2: Start the dev server**

Run from `frontend/web`:

```bash
pnpm dev -- --host 127.0.0.1
```

Expected: Vite serves the app at `http://127.0.0.1:5173/`.

- [ ] **Step 3: Capture representative open-menu screenshots**

Use browser automation to open and screenshot these routes:

- `/strategies` — list toolbar sort/filter menu.
- `/eval/runs` — start eval strategy picker, scenario picker, review provider/model controls.
- `/authoring/<first-strategy-id-visible-in-/strategies>` — existing agent picker, model picker, pipeline/risk menus; record the chosen id in the audit note.
- `/scenarios/<first-scenario-id-visible-in-/scenarios>` — asset picker and granularity menu; record the chosen id in the audit note.
- `/settings/providers` — provider kind menus.
- `/settings/skills` — skill kind menu.

Save screenshots under `/tmp/visual-review/xvision-dropdowns/`.

- [ ] **Step 4: Fix only verified visual defects**

If screenshots show clipping, unreadable contrast, missing focus, or inconsistent radius/surface tokens, fix the shared primitive first. Only patch a callsite when the defect is local to that callsite.

- [ ] **Step 5: Update inventory status**

In `docs/superpowers/audits/2026-06-22-dropdown-inventory.md`, add:

```markdown
## Final Verification

- Typecheck: `pnpm typecheck` — PASS
- Targeted tests: `[command]` — PASS
- Visual screenshots: `/tmp/visual-review/xvision-dropdowns/`
- Retained native controls: none
```

If a native control is retained, list it with the exact reason.

- [ ] **Step 6: Commit**

```bash
git add docs/superpowers/audits/2026-06-22-dropdown-inventory.md frontend/web/src
git commit -m "test: verify dropdown audit"
```
