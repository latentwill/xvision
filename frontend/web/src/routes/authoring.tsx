import { useEffect, useState } from "react";
import { Link, Navigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ModelPicker } from "@/components/ModelPicker";
import { ApiError } from "@/api/client";
import {
  getStrategy,
  setRiskConfig,
  strategyKeys,
  updateSlot,
  validateDraft,
  type LLMSlot,
  type Strategy,
  type UpdateSlotBody,
  type ValidateDraftOut,
} from "@/api/strategies";
import { getStrategyChart, strategyChartKeys } from "@/api/chart";
import { StrategyChart } from "@/components/chart/StrategyChart";
import { listProviders, settingsKeys } from "@/api/settings";

const RISK_PRESETS: { key: string; label: string }[] = [
  { key: "conservative", label: "Conservative" },
  { key: "balanced", label: "Balanced" },
  { key: "aggressive", label: "Aggressive" },
];

const SLOT_ROLES: {
  role: "regime" | "intern" | "trader";
  label: string;
  required: boolean;
}[] = [
  { role: "regime", label: "Regime", required: false },
  { role: "intern", label: "Intern", required: false },
  { role: "trader", label: "Trader", required: true },
];

export function AuthoringRoute() {
  const params = useParams<{ id?: string }>();

  if (!params.id) {
    return <Navigate to="/strategies" replace />;
  }

  return <InspectorPage id={params.id} />;
}

function InspectorPage({ id }: { id: string }) {
  const qc = useQueryClient();
  const bundleQ = useQuery({
    queryKey: strategyKeys.detail(id),
    queryFn: () => getStrategy(id),
  });
  const validateQ = useQuery({
    queryKey: strategyKeys.validate(id),
    queryFn: () => validateDraft(id),
    enabled: bundleQ.isSuccess,
  });

  // Re-validate after the bundle changes (e.g. after a slot save).
  useEffect(() => {
    if (bundleQ.dataUpdatedAt > 0) {
      qc.invalidateQueries({ queryKey: strategyKeys.validate(id) });
    }
  }, [bundleQ.dataUpdatedAt, id, qc]);

  return (
    <>
      <Topbar
        title="Inspector"
        sub={
          bundleQ.data
            ? `${bundleQ.data.manifest.display_name} · ${id}`
            : id
        }
      />

      <InspectorActions strategyId={id} />

      <div className="grid grid-cols-1 lg:grid-cols-[1fr_320px] gap-5">
        <div className="space-y-5">
          {bundleQ.isPending ? (
            <Card>
              <LoadingSkeleton />
            </Card>
          ) : bundleQ.isError ? (
            <Card>
              <ErrorState
                err={bundleQ.error}
                onRetry={() => bundleQ.refetch()}
              />
            </Card>
          ) : bundleQ.data ? (
            <BundleEditor bundle={bundleQ.data} />
          ) : null}
          <PerformanceHistoryCard strategyId={id} />
        </div>

        <aside className="space-y-5">
          <ValidationCard query={validateQ} />
          <RunEvalCard agentId={id} />
          <BackLinkCard />
        </aside>
      </div>
    </>
  );
}

function PerformanceHistoryCard({ strategyId }: { strategyId: string }) {
  const chart = useQuery({
    queryKey: strategyChartKeys.strategy(strategyId),
    queryFn: () => getStrategyChart(strategyId),
  });

  return (
    <Card>
      <SectionHeader
        label="Performance history"
        hint="Equity curves from all completed eval runs, colour-coded by scenario."
      />
      <div className="px-5 pb-5">
        {chart.isPending && (
          <div className="text-text-3 text-[13px] py-4">Loading history…</div>
        )}
        {chart.isError && (
          <div className="text-danger text-[13px] py-4">
            Could not load chart.
          </div>
        )}
        {chart.data && <StrategyChart payload={chart.data} />}
      </div>
    </Card>
  );
}

function BundleEditor({ bundle }: { bundle: Strategy }) {
  return (
    <>
      <ManifestCard bundle={bundle} />
      {SLOT_ROLES.map(({ role, label, required }) => (
        <SlotCard
          key={role}
          id={bundle.manifest.id}
          role={role}
          label={label}
          required={required}
          slot={
            role === "regime"
              ? bundle.regime_slot
              : role === "intern"
                ? bundle.intern_slot
                : bundle.trader_slot
          }
        />
      ))}
      <RiskCard bundle={bundle} />
      <MechanicalParamsCard bundle={bundle} />
    </>
  );
}

function ManifestCard({ bundle }: { bundle: Strategy }) {
  const m = bundle.manifest;
  return (
    <Card>
      <SectionHeader label="Manifest" hint="Read-only metadata for v1." />
      <dl className="grid grid-cols-[160px_1fr] gap-y-2 px-5 pb-4 text-[13px]">
        <DT>Display name</DT>
        <DD>{m.display_name}</DD>
        <DT>Template</DT>
        <DD className="font-mono text-text-2">{m.template}</DD>
        <DT>Creator</DT>
        <DD className="font-mono text-text-2">{m.creator}</DD>
        <DT>Asset universe</DT>
        <DD>
          {m.asset_universe.length > 0
            ? m.asset_universe.map((a) => (
                <span
                  key={a}
                  className="inline-block mr-1.5 px-1.5 py-0.5 bg-surface-elev border border-border-soft rounded text-[12px] font-mono"
                >
                  {a}
                </span>
              ))
            : "(none)"}
        </DD>
        <DT>Cadence</DT>
        <DD>
          every <strong>{m.decision_cadence_minutes}</strong> min
        </DD>
        <DT>Risk basis</DT>
        <DD>{m.risk_preset_or_config}</DD>
      </dl>
    </Card>
  );
}

function SlotCard({
  id,
  role,
  label,
  required,
  slot,
}: {
  id: string;
  role: "regime" | "intern" | "trader";
  label: string;
  required: boolean;
  slot: LLMSlot | null;
}) {
  const qc = useQueryClient();
  const providers = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
  });
  const [prompt, setPrompt] = useState(slot?.prompt ?? "");
  const [model, setModel] = useState(slot?.model_requirement ?? "");
  const [tools, setTools] = useState((slot?.allowed_tools ?? []).join(", "));
  const [savedFlash, setSavedFlash] = useState(false);

  // Reset when the underlying slot's saved values change.
  useEffect(() => {
    setPrompt(slot?.prompt ?? "");
    setModel(slot?.model_requirement ?? "");
    setTools((slot?.allowed_tools ?? []).join(", "));
  }, [slot?.prompt, slot?.model_requirement, slot?.allowed_tools]);

  const dirty =
    prompt !== (slot?.prompt ?? "") ||
    model !== (slot?.model_requirement ?? "") ||
    tools !== (slot?.allowed_tools ?? []).join(", ");

  const save = useMutation({
    mutationFn: (body: UpdateSlotBody) => updateSlot(id, role, body),
    onSuccess: () => {
      setSavedFlash(true);
      window.setTimeout(() => setSavedFlash(false), 1800);
      qc.invalidateQueries({ queryKey: strategyKeys.detail(id) });
    },
  });

  function onSave() {
    const body: UpdateSlotBody = {};
    if (prompt !== (slot?.prompt ?? "")) body.prompt = prompt;
    if (model !== (slot?.model_requirement ?? ""))
      body.model_requirement = model;
    const parsedTools = tools
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);
    if (parsedTools.join(",") !== (slot?.allowed_tools ?? []).join(","))
      body.allowed_tools = parsedTools;
    if (Object.keys(body).length === 0) return;
    save.mutate(body);
  }

  return (
    <Card>
      <SectionHeader
        label={`${label} slot`}
        hint={
          slot
            ? `${role} · model: ${slot.model_requirement || "(none)"}`
            : required
              ? "Required — not set"
              : "Optional · not set"
        }
      />
      <div className="px-5 pb-5 space-y-3">
        <Field label="Prompt">
          <textarea
            className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono leading-snug min-h-[140px]"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder={`System prompt for the ${role} slot…`}
          />
        </Field>
        <Field
          label="Model requirement"
          hint="Pick a configured model below, or type a constraint pattern (e.g. anthropic.claude-sonnet-4.6+)."
        >
          <div className="space-y-2">
            <ModelPicker
              rows={providers.data?.providers ?? []}
              loading={providers.isPending}
              provider={
                providers.data?.providers.find((p) =>
                  p.enabled_models.includes(model),
                )?.name ?? null
              }
              model={model}
              onChange={(_p, m) => setModel(m)}
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
              placeholder="— pick a configured model —"
              ariaLabel={`Model requirement for ${role} slot`}
            />
            <input
              className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="or type a constraint…"
            />
          </div>
        </Field>
        <Field
          label="Allowed tools"
          hint="Comma-separated tool names from the registry."
        >
          <input
            className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
            value={tools}
            onChange={(e) => setTools(e.target.value)}
            placeholder="ohlcv, indicator_panel"
          />
        </Field>

        <div className="flex items-center gap-3 pt-1">
          <button
            onClick={onSave}
            disabled={!dirty || save.isPending}
            className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 disabled:hover:bg-gold transition-colors"
          >
            {save.isPending ? "Saving…" : "Save slot"}
          </button>
          {savedFlash ? (
            <span className="text-[12px] text-success">Saved.</span>
          ) : save.isError ? (
            <span className="text-[12px] text-danger">
              {errorMessage(save.error)}
            </span>
          ) : null}
        </div>
      </div>
    </Card>
  );
}

function RiskCard({ bundle }: { bundle: Strategy }) {
  const qc = useQueryClient();
  const [savedFlash, setSavedFlash] = useState(false);
  const apply = useMutation({
    mutationFn: (preset: string) =>
      setRiskConfig(bundle.manifest.id, { preset }),
    onSuccess: () => {
      setSavedFlash(true);
      window.setTimeout(() => setSavedFlash(false), 1800);
      qc.invalidateQueries({
        queryKey: strategyKeys.detail(bundle.manifest.id),
      });
    },
  });

  const r = bundle.risk;
  const currentBasis = bundle.manifest.risk_preset_or_config;

  return (
    <Card>
      <SectionHeader label="Risk" hint={`Currently: ${currentBasis}`} />
      <div className="px-5 pb-5 space-y-4">
        <div className="flex items-center gap-2">
          {RISK_PRESETS.map((p) => (
            <button
              key={p.key}
              onClick={() => apply.mutate(p.key)}
              disabled={apply.isPending || currentBasis === p.key}
              className={`px-3 py-2 rounded text-[13px] font-medium border transition-colors ${
                currentBasis === p.key
                  ? "bg-gold text-bg border-gold"
                  : "border-border text-text-2 hover:text-text hover:border-text-3"
              } disabled:opacity-50`}
            >
              {p.label}
            </button>
          ))}
          {savedFlash ? (
            <span className="text-[12px] text-success ml-2">Applied.</span>
          ) : apply.isError ? (
            <span className="text-[12px] text-danger ml-2">
              {errorMessage(apply.error)}
            </span>
          ) : null}
        </div>

        <dl className="grid grid-cols-2 gap-y-2 text-[13px] text-text-2">
          <RiskRow
            label="Risk per trade"
            value={`${(r.risk_pct_per_trade * 100).toFixed(2)}%`}
          />
          <RiskRow
            label="Max concurrent positions"
            value={String(r.max_concurrent_positions)}
          />
          <RiskRow label="Max leverage" value={`${r.max_leverage.toFixed(1)}x`} />
          <RiskRow
            label="Stop-loss ATR ×"
            value={r.stop_loss_atr_multiple.toFixed(1)}
          />
          <RiskRow
            label="Daily loss kill"
            value={`${(r.daily_loss_kill_pct * 100).toFixed(2)}%`}
          />
        </dl>
      </div>
    </Card>
  );
}

function MechanicalParamsCard({ bundle }: { bundle: Strategy }) {
  const json = JSON.stringify(bundle.mechanical_params, null, 2);
  const empty =
    bundle.mechanical_params == null ||
    (typeof bundle.mechanical_params === "object" &&
      Object.keys(bundle.mechanical_params as object).length === 0);

  return (
    <Card>
      <SectionHeader
        label="Mechanical params"
        hint="Read-only in v1; per-field editor lands with the LLM split editor."
      />
      <div className="px-5 pb-5">
        {empty ? (
          <p className="m-0 text-[13px] text-text-3">
            No mechanical params on this template.
          </p>
        ) : (
          <pre className="m-0 p-3 bg-surface-elev border border-border-soft rounded text-[12px] text-text-2 overflow-x-auto font-mono">
            {json}
          </pre>
        )}
      </div>
    </Card>
  );
}

function ValidationCard({
  query,
}: {
  query: ReturnType<typeof useQuery<ValidateDraftOut>>;
}) {
  return (
    <Card>
      <SectionHeader label="Validation" />
      <div className="px-5 pb-5 text-[13px]">
        {query.isPending ? (
          <p className="m-0 text-text-3">Validating…</p>
        ) : query.isError ? (
          <p className="m-0 text-danger">{errorMessage(query.error)}</p>
        ) : query.data ? (
          query.data.ok ? (
            <Pill tone="gold">
              <span className="w-1.5 h-1.5 rounded-full bg-gold" /> valid
            </Pill>
          ) : (
            <div className="space-y-2">
              <Pill tone="danger">
                <span className="w-1.5 h-1.5 rounded-full bg-danger" />
                {" "}
                invalid
              </Pill>
              <ul className="m-0 pl-4 list-disc text-text-2 space-y-1">
                {query.data.errors.map((err, i) => (
                  <li key={i}>{err}</li>
                ))}
              </ul>
            </div>
          )
        ) : null}
        <div className="mt-4">
          <button
            onClick={() => query.refetch()}
            disabled={query.isFetching}
            className="inline-flex items-center gap-2 px-3 py-1.5 rounded text-[12px] font-medium border border-border text-text-2 hover:text-text hover:border-text-3 disabled:opacity-50 transition-colors"
          >
            {query.isFetching ? "Re-validating…" : "Re-validate"}
          </button>
        </div>
      </div>
    </Card>
  );
}

function BackLinkCard() {
  return (
    <Card>
      <div className="px-5 py-4 text-[13px] text-text-2">
        <Link to="/strategies" className="text-text hover:underline">
          ← Back to strategies
        </Link>
      </div>
    </Card>
  );
}

function RunEvalCard({ agentId }: { agentId: string }) {
  // v1 launches eval runs via CLI; the dashboard surfaces results. This
  // card gives the operator a copy-pasteable command + a direct link to
  // the runs list so the loop "edit → eval → inspect" is reachable from
  // inside the Inspector instead of requiring a route hop.
  const cliCommand = `xvn eval run --strategy ${agentId} --scenario crypto-bull-q1-2025 --mode backtest`;
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(cliCommand);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch {
      // Clipboard API can fail in non-secure contexts; silently no-op.
      // The user can still triple-click to select the command text.
    }
  }

  return (
    <Card>
      <SectionHeader
        label="Run eval"
        hint="Launch via CLI; results render in /eval-runs."
      />
      <div className="px-5 py-4 space-y-3">
        <div className="relative">
          <pre className="m-0 px-3 py-2 bg-surface-elev border border-border-soft rounded text-[11.5px] font-mono text-text overflow-x-auto whitespace-pre">
{cliCommand}
          </pre>
          <button
            type="button"
            onClick={copy}
            className="absolute top-1.5 right-1.5 px-2 py-0.5 text-[11px] text-text-3 hover:text-text bg-surface-card border border-border rounded"
            title="Copy command"
          >
            {copied ? "copied" : "copy"}
          </button>
        </div>
        <p className="m-0 text-[12px] text-text-3 leading-snug">
          Swap <code className="font-mono text-text-2">crypto-bull-q1-2025</code> for any{" "}
          <code className="font-mono text-text-2">xvn eval scenarios</code> id. Use{" "}
          <code className="font-mono text-text-2">--mode paper</code> for Alpaca paper trading.
        </p>
        <Link
          to="/eval-runs"
          className="inline-flex items-center gap-1 text-[13px] text-text hover:text-gold"
        >
          Browse eval runs →
        </Link>
      </div>
    </Card>
  );
}

function SectionHeader({ label, hint }: { label: string; hint?: string }) {
  return (
    <header className="px-5 pt-4 pb-3 border-b border-border-soft">
      <div className="text-[12px] uppercase tracking-wide text-text-3">
        {label}
      </div>
      {hint ? (
        <div className="text-[12px] text-text-2 mt-0.5">{hint}</div>
      ) : null}
    </header>
  );
}

function InspectorActions({ strategyId }: { strategyId: string }) {
  // Discoverable CTA for "now that you've edited this bundle, run it."
  // Deep-links the strategy id via `?strategy=<id>` so a future
  // run-form on `/eval-runs` can pre-select it without an extra
  // round-trip. Until that form exists the param is benign — the
  // route just ignores it.
  return (
    <div className="flex items-center justify-end gap-3 mb-5">
      <Link
        to={`/eval-runs?strategy=${encodeURIComponent(strategyId)}`}
        className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft transition-colors"
      >
        Run eval →
      </Link>
    </div>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className="text-[12px] text-text-2 mb-1 block">{label}</span>
      {children}
      {hint ? (
        <span className="text-[11px] text-text-3 mt-1 block">{hint}</span>
      ) : null}
    </label>
  );
}

function RiskRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="text-text-3">{label}</dt>
      <dd className="m-0 text-text font-mono">{value}</dd>
    </>
  );
}

function DT({ children }: { children: React.ReactNode }) {
  return <dt className="text-text-3">{children}</dt>;
}

function DD({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return <dd className={`m-0 text-text ${className ?? ""}`}>{children}</dd>;
}

function LoadingSkeleton() {
  return (
    <div className="px-5 py-4 space-y-3" aria-busy>
      {Array.from({ length: 6 }).map((_, i) => (
        <div key={i} className="flex items-center gap-4 py-2">
          <div className="h-4 w-48 rounded bg-surface-elev animate-pulse" />
          <div className="h-4 w-32 rounded bg-surface-elev animate-pulse" />
        </div>
      ))}
    </div>
  );
}

function ErrorState({ err, onRetry }: { err: unknown; onRetry: () => void }) {
  if (err instanceof ApiError && err.code === "not_found") {
    return (
      <div className="px-6 py-12 text-center">
        <div className="font-serif italic text-[24px] text-text-3 mb-3">
          draft not found
        </div>
        <p className="m-0 mb-5 max-w-md mx-auto text-text-2 leading-snug">
          This draft id doesn't exist on the engine.
        </p>
        <Link
          to="/strategies"
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
        >
          Back to strategies
        </Link>
      </div>
    );
  }
  return (
    <div className="px-6 py-12 text-center">
      <div className="font-serif italic text-[24px] text-danger mb-3">
        couldn't load draft
      </div>
      <p className="m-0 mb-5 max-w-md mx-auto text-text-2 leading-snug">
        <code className="text-danger font-mono text-[12px]">
          {errorMessage(err)}
        </code>
      </p>
      <button
        onClick={onRetry}
        className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3"
      >
        Retry
      </button>
    </div>
  );
}

function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
