import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
import {
  getObservability,
  setObservabilityMode,
  settingsKeys,
} from "@/api/settings";
import type { RetentionModeDto } from "@/api/types.gen";
import {
  themeDefinitions,
  themePreferenceOptions,
  type ResolvedTheme,
} from "@/theme/themes";
import { useTheme } from "@/theme/useTheme";
import { RestartTourButton } from "@/features/onboarding";

function swatchFor(value: string) {
  const id: ResolvedTheme = value === "auto" ? "dark" : (value as ResolvedTheme);
  return themeDefinitions[id].swatch;
}

type RetentionOption = {
  value: RetentionModeDto;
  label: string;
  description: string;
};

const retentionOptions: RetentionOption[] = [
  {
    value: "full_debug",
    label: "Full debug (default)",
    description:
      "Store full prompts, responses, and tool I/O so traces show every field. Best for local development and debugging.",
  },
  {
    value: "redacted",
    label: "Redacted",
    description:
      "Store prompts and responses but strip detected secrets before they hit disk. Pick this for shared or client-visible installs.",
  },
  {
    value: "hash_only",
    label: "Hash only",
    description:
      "Store only content hashes — prompts, responses, and tool payloads are dropped. Traces show shape and timing, not bodies.",
  },
];

export function SettingsGeneralRoute() {
  const { preference, setPreference } = useTheme();
  const qc = useQueryClient();

  const obs = useQuery({
    queryKey: settingsKeys.observability(),
    queryFn: getObservability,
  });

  const updateMode = useMutation({
    mutationFn: (mode: RetentionModeDto) => setObservabilityMode(mode),
    onSuccess: (report) => {
      qc.setQueryData(settingsKeys.observability(), report);
    },
  });

  const activeMode: RetentionModeDto | undefined = obs.data?.mode;

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

      <Card className="p-5">
        <div className="mb-4">
          <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
            Trace data retention
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
            Controls what the agent-run trace dock keeps on disk. Changes
            persist to{" "}
            <code className="font-mono text-[11px]">
              $XVN_HOME/config/observability.toml
            </code>{" "}
            and take effect on the next run.
          </p>
        </div>

        {obs.isPending ? (
          <div className="space-y-2 animate-pulse">
            <div className="h-14 bg-surface-elev rounded" />
            <div className="h-14 bg-surface-elev rounded" />
            <div className="h-14 bg-surface-elev rounded" />
          </div>
        ) : obs.isError || !obs.data ? (
          <div className="text-[13px] text-danger">
            Couldn't load retention setting. Try refreshing the page.
          </div>
        ) : (
          <div
            role="radiogroup"
            aria-label="Trace data retention"
            className="space-y-2"
          >
            {retentionOptions.map((option) => {
              const checked = activeMode === option.value;
              const disabled = updateMode.isPending;
              return (
                <label
                  key={option.value}
                  className={[
                    "flex items-start gap-3 rounded border px-3 py-2.5 cursor-pointer transition-colors",
                    checked
                      ? "border-gold bg-gold/[0.08] text-text"
                      : "border-border bg-surface-elev text-text-2 hover:border-text-3 hover:text-text",
                    disabled ? "opacity-60 cursor-wait" : "",
                  ].join(" ")}
                >
                  <input
                    type="radio"
                    name="retention-mode"
                    value={option.value}
                    checked={checked}
                    disabled={disabled}
                    onChange={() => updateMode.mutate(option.value)}
                    className="mt-1"
                  />
                  <span className="flex-1">
                    <span className="block text-[13px] font-medium text-text">
                      {option.label}
                    </span>
                    <span className="block mt-0.5 text-[12px] text-text-3 leading-snug">
                      {option.description}
                    </span>
                  </span>
                </label>
              );
            })}
            {updateMode.isError && (
              <p className="m-0 text-[12px] text-danger">
                Couldn't save retention mode. The setting hasn't changed.
              </p>
            )}
          </div>
        )}
      </Card>
    </div>
  );
}
