import { Card } from "@/components/primitives/Card";
import {
  themeDefinitions,
  themePreferenceOptions,
  ACCENT_PRESETS,
  type ResolvedTheme,
  type AccentKey,
} from "@/theme/themes";
import { useTheme } from "@/theme/useTheme";
import { useAccent } from "@/theme/useAccent";
import { RestartTourButton } from "@/features/onboarding";
import { MemorySettingsCard } from "./MemorySettingsCard";

function swatchFor(value: string) {
  const id: ResolvedTheme = value === "auto" ? "dark" : (value as ResolvedTheme);
  return themeDefinitions[id].swatch;
}

export function SettingsGeneralRoute() {
  const { preference, setPreference } = useTheme();
  const { accentKey, setAccent } = useAccent();

  return (
    <div className="space-y-5">
      <Card className="p-5">
        <div className="mb-4">
          <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
            Appearance
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
            Signal theme. Auto follows your system; Light is the cool-white variant; Dark is pure-black Signal.
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
                  className="flex h-7 w-10 overflow-hidden rounded-sm border border-border"
                  style={{ background: swatch.bg }}
                >
                  <span className="flex-1" style={{ background: swatch.surface }} />
                  <span className="flex-1" style={{ background: swatch.accent }} />
                  <span
                    className="flex-1 grid place-items-center"
                    style={{ background: swatch.bg }}
                  >
                    <span
                      className="h-2 w-2 rounded-full"
                      style={{ background: swatch.text }}
                    />
                  </span>
                </span>
                <span className="text-[13px]">{option.label}</span>
              </label>
            );
          })}
        </div>

        <div className="mt-5 pt-4 border-t border-border">
          <p className="text-[11px] font-medium text-text-3 uppercase tracking-[0.08em] mb-3">
            Accent color
          </p>
          <div className="flex items-center gap-1.5 flex-wrap">
            {(Object.keys(ACCENT_PRESETS) as AccentKey[]).map((key) => {
              const preset = ACCENT_PRESETS[key];
              const selected = accentKey === key;
              return (
                <button
                  key={key}
                  type="button"
                  aria-label={`${preset.label} accent`}
                  aria-pressed={selected}
                  title={preset.label}
                  onClick={() => setAccent(key)}
                  className={[
                    "flex flex-col items-center gap-1 rounded px-2 py-1.5 transition-colors",
                    selected ? "bg-surface-panel" : "hover:bg-surface-elev",
                  ].join(" ")}
                >
                  <span
                    className={[
                      "w-5 h-5 rounded-full",
                      selected
                        ? "ring-2 ring-offset-1 ring-offset-surface-card ring-text-2"
                        : "ring-1 ring-border",
                    ].join(" ")}
                    style={{ background: preset.dark }}
                  />
                  <span
                    className={`text-[10px] font-mono ${selected ? "text-text" : "text-text-3"}`}
                  >
                    {preset.label}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </Card>

      <Card className="p-5">
        <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
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

      <MemorySettingsCard />

      {/*
        Retention-mode UI intentionally disabled for now. The backend
        observability contract is left intact so this card can be restored
        from git history when the product decision reverses.
      */}
    </div>
  );
}
