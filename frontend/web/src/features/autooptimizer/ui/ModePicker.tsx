/**
 * ModePicker — inline run-mode selector for the Optimizer.
 *
 * Three options (radio-style):
 *   - "once"          → Single experiment
 *   - "n_experiments" → N experiments  + count input (≥1)
 *   - "until_budget"  → Until budget   + budget field in USD (>0)
 *
 * No popup. Fully inline form, honoring the no-popups UI rule.
 */
import { useState, useId } from "react";

export type RunMode = "once" | "n_experiments" | "until_budget";

export interface ModePickerProps {
  value: RunMode;
  onChange: (mode: RunMode, count?: number, budget?: number) => void;
}

interface RadioOptionProps {
  id: string;
  name: string;
  value: RunMode;
  currentValue: RunMode;
  label: string;
  onChange: (mode: RunMode) => void;
}

function RadioOption({ id, name, value, currentValue, label, onChange }: RadioOptionProps) {
  return (
    <label
      htmlFor={id}
      className="flex items-center gap-2 cursor-pointer select-none"
    >
      <input
        id={id}
        type="radio"
        name={name}
        value={value}
        checked={currentValue === value}
        onChange={() => onChange(value)}
        className="accent-accent h-3.5 w-3.5 cursor-pointer"
      />
      <span className="text-[13px] text-text-2">{label}</span>
    </label>
  );
}

export function ModePicker({ value: initialValue, onChange }: ModePickerProps) {
  const uid = useId();
  const groupName = `${uid}-run-mode`;

  // Local state so the component manages its own selection (uncontrolled mode
  // display). The parent sets the initial value; after that, local state drives
  // which sub-fields are shown. onChange notifies the parent on every change.
  const [localMode, setLocalMode] = useState<RunMode>(initialValue);

  // Local state for the sub-fields; synced out through onChange on each change
  const [count, setCount] = useState<string>("3");
  const [budget, setBudget] = useState<string>("1.00");
  const [countError, setCountError] = useState<string | null>(null);
  const [budgetError, setBudgetError] = useState<string | null>(null);

  // When the parent changes value externally, sync local mode.
  // (useState initial only runs once, so we track prev value.)
  const [prevInitial, setPrevInitial] = useState(initialValue);
  if (initialValue !== prevInitial) {
    setPrevInitial(initialValue);
    setLocalMode(initialValue);
  }

  function handleModeChange(mode: RunMode) {
    setLocalMode(mode);
    // Clear errors on mode switch
    setCountError(null);
    setBudgetError(null);
    if (mode === "once") {
      onChange("once", undefined, undefined);
    } else if (mode === "n_experiments") {
      const parsed = parseInt(count, 10);
      if (Number.isFinite(parsed) && parsed >= 1) {
        onChange("n_experiments", parsed, undefined);
      } else {
        onChange("n_experiments", undefined, undefined);
      }
    } else if (mode === "until_budget") {
      const parsed = parseFloat(budget);
      if (Number.isFinite(parsed) && parsed > 0) {
        onChange("until_budget", undefined, parsed);
      } else {
        onChange("until_budget", undefined, undefined);
      }
    }
  }

  function handleCountChange(raw: string) {
    setCount(raw);
    setCountError(null);
    const parsed = parseInt(raw, 10);
    if (!raw.trim() || !Number.isFinite(parsed) || parsed < 1) {
      setCountError("Count must be ≥ 1");
      onChange("n_experiments", undefined, undefined);
    } else {
      onChange("n_experiments", parsed, undefined);
    }
  }

  function handleCountBlur() {
    const parsed = parseInt(count, 10);
    if (!count.trim() || !Number.isFinite(parsed) || parsed < 1) {
      setCountError("Count must be ≥ 1");
    }
  }

  function handleBudgetChange(raw: string) {
    setBudget(raw);
    setBudgetError(null);
    const parsed = parseFloat(raw);
    if (!raw.trim() || !Number.isFinite(parsed) || parsed <= 0) {
      setBudgetError("Budget must be > $0");
      onChange("until_budget", undefined, undefined);
    } else {
      onChange("until_budget", undefined, parsed);
    }
  }

  function handleBudgetBlur() {
    const parsed = parseFloat(budget);
    if (!budget.trim() || !Number.isFinite(parsed) || parsed <= 0) {
      setBudgetError("Budget must be > $0");
    }
  }

  return (
    <div
      className="flex flex-wrap items-start gap-x-6 gap-y-3"
      data-testid="mode-picker"
    >
      {/* Single experiment */}
      <RadioOption
        id={`${uid}-once`}
        name={groupName}
        value="once"
        currentValue={localMode}
        label="Single experiment"
        onChange={handleModeChange}
      />

      {/* N experiments */}
      <div className="flex flex-col gap-1">
        <RadioOption
          id={`${uid}-n_experiments`}
          name={groupName}
          value="n_experiments"
          currentValue={localMode}
          label="N experiments"
          onChange={handleModeChange}
        />
        {localMode === "n_experiments" && (
          <div className="ml-5 flex flex-col gap-0.5">
            <label
              htmlFor={`${uid}-count`}
              className="sr-only"
            >
              Count
            </label>
            <input
              id={`${uid}-count`}
              type="number"
              min={1}
              step={1}
              value={count}
              onChange={(e) => handleCountChange(e.target.value)}
              onBlur={handleCountBlur}
              aria-label="Count"
              className="w-20 rounded border border-border bg-surface-elev px-2 py-1 font-mono text-[12px] text-text focus:outline-none focus:ring-1 focus:ring-accent/50"
            />
            {countError && (
              <p className="text-[11px] text-danger">{countError}</p>
            )}
          </div>
        )}
      </div>

      {/* Until budget */}
      <div className="flex flex-col gap-1">
        <RadioOption
          id={`${uid}-until_budget`}
          name={groupName}
          value="until_budget"
          currentValue={localMode}
          label="Until budget"
          onChange={handleModeChange}
        />
        {localMode === "until_budget" && (
          <div className="ml-5 flex flex-col gap-0.5">
            <label
              htmlFor={`${uid}-budget`}
              className="sr-only"
            >
              Budget (USD)
            </label>
            <div className="flex items-center gap-1">
              <span className="text-[12px] text-text-3">$</span>
              <input
                id={`${uid}-budget`}
                type="number"
                min={0.01}
                step={0.01}
                value={budget}
                onChange={(e) => handleBudgetChange(e.target.value)}
                onBlur={handleBudgetBlur}
                aria-label="Budget (USD)"
                className="w-24 rounded border border-border bg-surface-elev px-2 py-1 font-mono text-[12px] text-text focus:outline-none focus:ring-1 focus:ring-accent/50"
              />
            </div>
            {budgetError && (
              <p className="text-[11px] text-danger">{budgetError}</p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
