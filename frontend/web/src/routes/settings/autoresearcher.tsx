import { useEffect, useState } from "react";
import { Card } from "@/components/primitives/Card";
import {
  useAutoresearchConfig,
  useSetAutoresearchConfig,
  type AutoresearchConfig,
} from "@/api/autoresearch-config";

type FieldErrors = Partial<Record<keyof AutoresearchConfig, string>>;

function validateConfig(cfg: AutoresearchConfig): FieldErrors {
  const errors: FieldErrors = {};
  if (cfg.promotion_acc_floor < 0 || cfg.promotion_acc_floor > 1) {
    errors.promotion_acc_floor = "Must be between 0 and 1.";
  }
  if (cfg.promotion_epsilon < 0) {
    errors.promotion_epsilon = "Must be non-negative.";
  }
  if (cfg.promotion_min_holdout < 0) {
    errors.promotion_min_holdout = "Must be non-negative.";
  }
  if (cfg.min_cycle_count < 1) {
    errors.min_cycle_count = "Must be at least 1.";
  }
  if (cfg.train_wall_clock_sec < 30) {
    errors.train_wall_clock_sec = "Must be at least 30 seconds.";
  }
  if (cfg.price_forward_threshold < 0) {
    errors.price_forward_threshold = "Must be non-negative.";
  }
  return errors;
}

type NumericFieldProps = {
  id: string;
  label: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  error?: string;
  onChange: (v: number) => void;
};

function NumericField({
  id,
  label,
  value,
  min,
  max,
  step = 0.01,
  error,
  onChange,
}: NumericFieldProps) {
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="text-[12px] text-text-2">
        {label}
      </label>
      <input
        id={id}
        type="number"
        step={step}
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(parseFloat(e.target.value))}
        aria-describedby={error ? `${id}-error` : undefined}
        className="w-40 rounded border border-border bg-surface-elev px-2 py-1.5 text-[13px] text-text focus:outline-none focus:border-text-3"
      />
      {error ? (
        <p id={`${id}-error`} role="alert" className="m-0 text-[12px] text-danger">
          {error}
        </p>
      ) : null}
    </div>
  );
}

export function AutoresearcherSettingsRoute() {
  const configQ = useAutoresearchConfig();
  const saveMutation = useSetAutoresearchConfig();

  const [draft, setDraft] = useState<AutoresearchConfig | null>(null);
  const [errors, setErrors] = useState<FieldErrors>({});

  useEffect(() => {
    if (configQ.data && draft == null) {
      setDraft(configQ.data);
    }
  }, [configQ.data, draft]);

  if (!draft) {
    return (
      <p className="text-[12px] text-text-3">
        {configQ.isPending ? "Loading…" : "Failed to load configuration."}
      </p>
    );
  }

  function update<K extends keyof AutoresearchConfig>(
    key: K,
    value: AutoresearchConfig[K],
  ) {
    const next = { ...draft!, [key]: value };
    setDraft(next);
    setErrors(validateConfig(next));
  }

  const hasErrors = Object.keys(errors).length > 0;

  function handleSave(e: React.FormEvent) {
    e.preventDefault();
    const errs = validateConfig(draft!);
    if (Object.keys(errs).length > 0) {
      setErrors(errs);
      return;
    }
    saveMutation.mutate(draft!);
  }

  return (
    <div className="space-y-5">
      <Card className="p-5">
        <div className="mb-4">
          <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
            Autoresearcher
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
            Thresholds for checkpoint promotion and the live-approval gate.
            Changes take effect on the next autoresearch run.
          </p>
        </div>

        <form onSubmit={handleSave} className="space-y-4">
          <NumericField
            id="ar-cfg-min-precision-lift"
            label="Min precision lift (percentage points)"
            value={draft.min_precision_lift_pp}
            min={0}
            step={0.5}
            error={errors.min_precision_lift_pp}
            onChange={(v) => update("min_precision_lift_pp", v)}
          />
          <NumericField
            id="ar-cfg-max-pnl-regression"
            label="Max PnL regression (0 = non-negative)"
            value={draft.max_pnl_regression}
            step={0.1}
            error={errors.max_pnl_regression}
            onChange={(v) => update("max_pnl_regression", v)}
          />
          <NumericField
            id="ar-cfg-promotion-epsilon"
            label="Promotion epsilon (val_acc margin over current best)"
            value={draft.promotion_epsilon}
            min={0}
            step={0.001}
            error={errors.promotion_epsilon}
            onChange={(v) => update("promotion_epsilon", v)}
          />
          <NumericField
            id="ar-cfg-promotion-acc-floor"
            label="Promotion acc floor (0–1)"
            value={draft.promotion_acc_floor}
            min={0}
            max={1}
            step={0.01}
            error={errors.promotion_acc_floor}
            onChange={(v) => update("promotion_acc_floor", v)}
          />
          <NumericField
            id="ar-cfg-promotion-min-holdout"
            label="Promotion min holdout samples"
            value={draft.promotion_min_holdout}
            min={0}
            step={1}
            error={errors.promotion_min_holdout}
            onChange={(v) => update("promotion_min_holdout", v)}
          />
          <NumericField
            id="ar-cfg-min-cycle-count"
            label="Min cycle count (per run)"
            value={draft.min_cycle_count}
            min={1}
            step={1}
            error={errors.min_cycle_count}
            onChange={(v) => update("min_cycle_count", v)}
          />
          <NumericField
            id="ar-cfg-train-wall-clock"
            label="Train wall-clock budget (seconds)"
            value={draft.train_wall_clock_sec}
            min={30}
            step={30}
            error={errors.train_wall_clock_sec}
            onChange={(v) => update("train_wall_clock_sec", v)}
          />
          <NumericField
            id="ar-cfg-price-forward-threshold"
            label="Price forward threshold"
            value={draft.price_forward_threshold}
            min={0}
            step={0.001}
            error={errors.price_forward_threshold}
            onChange={(v) => update("price_forward_threshold", v)}
          />

          <div className="flex items-center gap-3 pt-2">
            <button
              type="submit"
              disabled={hasErrors || saveMutation.isPending}
              className="px-3 py-1.5 rounded text-[13px] font-medium border border-gold text-gold hover:bg-gold/10 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {saveMutation.isPending ? "Saving…" : "Save"}
            </button>
            {saveMutation.isSuccess ? (
              <span className="text-[12px] text-info">Saved.</span>
            ) : null}
            {saveMutation.isError ? (
              <span className="text-[12px] text-danger">
                {saveMutation.error instanceof Error
                  ? saveMutation.error.message
                  : "Save failed."}
              </span>
            ) : null}
          </div>
        </form>
      </Card>
    </div>
  );
}
