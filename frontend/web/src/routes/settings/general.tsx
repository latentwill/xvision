import { Card } from "@/components/primitives/Card";
import {
  themeDefinitions,
  themePreferenceOptions,
  type ResolvedTheme,
} from "@/theme/themes";
import { useTheme } from "@/theme/useTheme";
import { RestartTourButton } from "@/features/onboarding";

function swatchFor(value: string) {
  const id: ResolvedTheme =
    value === "auto" ? "folio-dark" : (value as ResolvedTheme);
  return themeDefinitions[id].swatch;
}

export function SettingsGeneralRoute() {
  const { preference, setPreference } = useTheme();

  return (
    <div className="space-y-5">
      <Card className="p-5">
        <div className="mb-4">
          <h3 className="m-0 font-serif font-medium text-[20px] tracking-tight">
            Appearance
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
            Choose the dashboard color theme. This changes colors only.
          </p>
        </div>
        <div
          role="radiogroup"
          aria-label="Theme preference"
          className="grid gap-2 sm:grid-cols-2 xl:grid-cols-4"
        >
          {themePreferenceOptions.map((option) => {
            const swatch = swatchFor(option.value);
            const checked = preference === option.value;
            return (
              <label
                key={option.value}
                className={[
                  "flex items-center gap-3 rounded border px-3 py-2 cursor-pointer transition-colors",
                  checked
                    ? "border-gold bg-gold/[0.08] text-text"
                    : "border-border bg-surface-elev text-text-2 hover:border-text-3 hover:text-text",
                ].join(" ")}
              >
                <input
                  type="radio"
                  name="theme"
                  value={option.value}
                  checked={checked}
                  onChange={() => setPreference(option.value)}
                  className="sr-only"
                />
                <span
                  aria-hidden
                  className="grid h-7 w-7 grid-cols-2 overflow-hidden rounded border border-border"
                  style={{ background: swatch.bg }}
                >
                  <span style={{ background: swatch.surface }} />
                  <span style={{ background: swatch.accent }} />
                  <span style={{ background: swatch.border }} />
                  <span style={{ background: swatch.text }} />
                </span>
                <span className="text-[13px]">{option.label}</span>
              </label>
            );
          })}
        </div>
      </Card>

      <Card className="p-5">
        <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 className="m-0 font-serif font-medium text-[20px] tracking-tight">
              Guided tour
            </h3>
            <p className="m-0 mt-1 max-w-2xl text-[12px] leading-snug text-text-3">
              Replay the first-run walkthrough across Strategies, Scenarios,
              and Eval Runs.
            </p>
          </div>
          <RestartTourButton />
        </div>
      </Card>

      {/*
        Retention-mode UI intentionally disabled for now. The backend
        observability contract is left intact so this card can be restored
        from git history when the product decision reverses.
      */}
    </div>
  );
}
