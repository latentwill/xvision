import { useEffect, useState } from 'react';
import type { AssetClass } from '../../api/types.gen/AssetClass';
import type { CalendarRef } from '../../api/types.gen/CalendarRef';
import type { CreateScenarioRequest } from '../../api/types.gen/CreateScenarioRequest';
import type { DataSource } from '../../api/types.gen/DataSource';
import type { FillModel } from '../../api/types.gen/FillModel';
import type { LimitOrderFill } from '../../api/types.gen/LimitOrderFill';
import type { MarketOrderFill } from '../../api/types.gen/MarketOrderFill';
import type { QuoteCurrency } from '../../api/types.gen/QuoteCurrency';
import type { ReplayMode } from '../../api/types.gen/ReplayMode';
import type { ScenarioSource } from '../../api/types.gen/ScenarioSource';
import type { SlippageModel } from '../../api/types.gen/SlippageModel';
import type { Venue } from '../../api/types.gen/Venue';
import { RegimeRangePresets } from './RegimeRangePresets';

export type ScenarioFormDraft = {
  asset: string;
  from: string;
  to: string;
  granularity: ScenarioGranularity;
};

type ScenarioGranularity = string;

export type ScenarioFormProps = {
  initial?: Partial<CreateScenarioRequest>;
  submitting?: boolean;
  error?: string;
  onSubmit: (req: CreateScenarioRequest) => void;
  onCancel?: () => void;
  /** Fires whenever the four preview-relevant fields change. */
  onDraftChange?: (draft: ScenarioFormDraft) => void;
  layout?: 'wizard' | 'inline';
};

const ALPACA_ASSETS = [
  'BTC', 'ETH', 'LTC', 'SOL', 'AVAX', 'LINK', 'AAVE', 'UNI',
  'DOT', 'DOGE', 'SHIB', 'MATIC', 'BCH', 'USDT', 'USDC',
];

const ASSET_CLASS: AssetClass = 'Crypto';
const QUOTE_CURRENCY: QuoteCurrency = 'Usd';
const VENUE: Venue = 'Alpaca';
const SCENARIO_SOURCE: ScenarioSource = 'User';
const CALENDAR: CalendarRef = 'Continuous24x7';
const REPLAY_MODE: ReplayMode = { mode: 'Continuous' };
const MARKET_ORDER_FILL: MarketOrderFill = 'FullAtClose';
const LIMIT_ORDER_FILL: LimitOrderFill = 'NeverFills';
const SCENARIO_CAPITAL = {
  initial: 100000,
  currency: 'USD',
};
const GRANULARITY_OPTIONS = [
  '1m',
  '5m',
  '15m',
  '30m',
  '1h',
  '4h',
  '6h',
  '12h',
  '1d',
  '1w',
  '1mo',
  '3mo',
  '6mo',
  '12mo',
];

export function ScenarioForm({
  initial,
  submitting,
  error,
  onSubmit,
  onCancel,
  onDraftChange,
  layout = 'wizard',
}: ScenarioFormProps) {
  const [name, setName] = useState(initial?.display_name ?? '');
  const [asset, setAsset] = useState(
    initial?.asset?.[0]?.symbol ?? 'ETH',
  );
  const [from, setFrom] = useState(
    initial?.time_window?.start?.slice(0, 10) ?? '',
  );
  const [to, setTo] = useState(
    initial?.time_window?.end?.slice(0, 10) ?? '',
  );
  const [granularity, setGranularity] = useState<ScenarioGranularity>(() => {
    const normalized = normalizeGranularity(initial?.granularity);
    return normalized && isSupportedGranularity(normalized) ? normalized : '1h';
  });
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [notes, setNotes] = useState(initial?.notes ?? '');
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [feesMaker, setFeesMaker] = useState(
    initial?.venue?.fees?.maker_bps ?? 10,
  );
  const [feesTaker, setFeesTaker] = useState(
    initial?.venue?.fees?.taker_bps ?? 25,
  );
  const [slippageBps, setSlippageBps] = useState(5);
  const [latencyMs, setLatencyMs] = useState(
    initial?.venue?.latency?.decision_to_fill_ms ?? 500,
  );
  const [nameError, setNameError] = useState<string | null>(null);
  const [granularityError, setGranularityError] = useState<string | null>(null);
  const [timeError, setTimeError] = useState<string | null>(null);

  const estimatedBars = estimateBars(from, to, granularity);

  useEffect(() => {
    if (onDraftChange) {
      onDraftChange({ asset, from, to, granularity });
    }
    // Intentionally include onDraftChange — parent should memoize if needed.
  }, [asset, from, to, granularity, onDraftChange]);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const displayName = name.trim();
    if (!displayName) {
      setNameError('Scenario display name is required.');
      return;
    }
    setNameError(null);

    const granularityValue = granularity.trim().toLowerCase();
    if (!isSupportedGranularity(granularityValue)) {
      setGranularityError('Choose a supported Alpaca granularity.');
      return;
    }
    setGranularityError(null);

    if (!isValidWindow(from, to)) {
      setTimeError('End date must be after start date.');
      return;
    }
    setTimeError(null);

    const slippage: SlippageModel = { model: 'linear', bps: slippageBps };
    const fillModel: FillModel = {
      market_order_fill: MARKET_ORDER_FILL,
      limit_order_fill: LIMIT_ORDER_FILL,
      partial_fills: false,
      volume_constraints: null,
    };
    const dataSource: DataSource = {
      type: 'AlpacaHistorical',
      feed: null,
      adjustment: 'Raw',
    };

    const req: CreateScenarioRequest = {
      display_name: displayName,
      description: '',
      asset_class: ASSET_CLASS,
      asset: [{ class: ASSET_CLASS, symbol: asset, venue_symbol: `${asset}/USD` }],
      quote_currency: QUOTE_CURRENCY,
      time_window: { start: `${from}T00:00:00Z`, end: `${to}T00:00:00Z` },
      capital: SCENARIO_CAPITAL,
      granularity: granularityValue,
      timezone: 'UTC',
      calendar: CALENDAR,
      venue: {
        venue: VENUE,
        fees: { maker_bps: feesMaker, taker_bps: feesTaker },
        slippage,
        latency: { decision_to_fill_ms: latencyMs },
        fill_model: fillModel,
      },
      data_source: dataSource,
      replay_mode: REPLAY_MODE,
      tags,
      notes: notes.trim() || null,
      parent_scenario_id: null,
      source: SCENARIO_SOURCE,
    };

    onSubmit(req);
  }

  return (
    <form onSubmit={handleSubmit} className={`space-y-4 ${layout === 'wizard' ? 'max-w-2xl' : ''}`}>
      <Field label="Name">
        <input
          className="input"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            if (nameError) setNameError(null);
          }}
          required
        />
        {nameError ? (
          <div className="mt-1 text-[12px] text-rose-300">{nameError}</div>
        ) : null}
      </Field>
      <Field label="Notes">
        <input
          className="input"
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          placeholder="optional"
        />
      </Field>
      <Field label="Tags">
        <TagInput value={tags} onChange={setTags} />
      </Field>

      <Section title="Market">
        <Row>
          <Field label="Asset">
            <select
              className="input"
              value={asset}
              onChange={(e) => setAsset(e.target.value)}
            >
              {ALPACA_ASSETS.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Quote">
            <span className="input block">USD</span>
          </Field>
        </Row>
        <Row>
          <Field label="From">
            <input
              type="date"
              className="input"
              value={from}
              onChange={(e) => {
                setFrom(e.target.value);
                if (timeError) setTimeError(null);
              }}
              required
            />
          </Field>
          <Field label="To">
            <input
              type="date"
              className="input"
              value={to}
              onChange={(e) => {
                setTo(e.target.value);
                if (timeError) setTimeError(null);
              }}
              required
            />
          </Field>
        </Row>
        {timeError ? (
          <div className="mt-1 text-[12px] text-rose-300">{timeError}</div>
        ) : null}
        <RegimeRangePresets onPick={(start, end) => { setFrom(start); setTo(end); }} />
        <Field label="Granularity">
          <select
            className="input"
            value={granularity}
            onChange={(e) => {
              setGranularity(e.target.value);
              if (granularityError) setGranularityError(null);
            }}
            required
          >
            {GRANULARITY_OPTIONS.map((g) => (
              <option key={g} value={g}>
                {g}
              </option>
            ))}
          </select>
          {granularityError ? (
            <div className="mt-1 text-[12px] text-rose-300">{granularityError}</div>
          ) : null}
        </Field>
      </Section>

      <Section title="Venue (Alpaca)">
        <button
          type="button"
          className="text-text-3 text-[13px]"
          onClick={() => setAdvancedOpen((v) => !v)}
        >
          {advancedOpen ? '▾ Advanced' : '▸ Advanced'}
        </button>
        {advancedOpen && (
          <div className="space-y-3 mt-2">
            <Row>
              <Field label="Fees maker (bps)">
                <input
                  type="number"
                  className="input"
                  value={feesMaker}
                  onChange={(e) => setFeesMaker(+e.target.value)}
                />
              </Field>
              <Field label="Fees taker (bps)">
                <input
                  type="number"
                  className="input"
                  value={feesTaker}
                  onChange={(e) => setFeesTaker(+e.target.value)}
                />
              </Field>
            </Row>
            <Row>
              <Field label="Slippage (linear bps)">
                <input
                  type="number"
                  className="input"
                  value={slippageBps}
                  onChange={(e) => setSlippageBps(+e.target.value)}
                />
              </Field>
              <Field label="Latency (ms)">
                <input
                  type="number"
                  className="input"
                  value={latencyMs}
                  onChange={(e) => setLatencyMs(+e.target.value)}
                />
              </Field>
            </Row>
            <div className="text-[12px] text-text-3">
              Fill model: market-only, full-fills (v1 locked)
            </div>
          </div>
        )}
      </Section>

      <div className="text-[12px] text-text-3">
        Estimated bars to fetch:{' '}
        <span className="font-mono text-text">{estimatedBars.toLocaleString()}</span>
      </div>

      {error && <div className="text-danger text-[12px]">{error}</div>}

      <div className="flex gap-2">
        {onCancel && (
          <button
            type="button"
            onClick={onCancel}
            className="border border-border px-3 py-1.5 rounded text-[13px]"
          >
            Cancel
          </button>
        )}
        <button
          type="submit"
          disabled={submitting}
          className="border border-border bg-surface-elev px-3 py-1.5 rounded text-[13px] hover:border-text-3"
        >
          {submitting ? 'Creating…' : 'Create →'}
        </button>
      </div>
    </form>
  );
}

// ── Helpers ────────────────────────────────────────────────────────────────

function estimateBars(from: string, to: string, g: ScenarioGranularity): number {
  if (!from || !to) return 0;
  const ms = +new Date(to) - +new Date(from);
  if (ms <= 0) return 0;
  const barSeconds = granularitySeconds(g);
  if (!barSeconds) return 0;
  return Math.round(ms / 1000 / barSeconds);
}

function isSupportedGranularity(granularity: string) {
  return GRANULARITY_OPTIONS.includes(granularity);
}

function isValidWindow(from: string, to: string) {
  if (!from || !to) return false;
  const start = new Date(`${from}T00:00:00Z`).getTime();
  const end = new Date(`${to}T00:00:00Z`).getTime();
  return Number.isFinite(start) && Number.isFinite(end) && end > start;
}

function normalizeGranularity(granularity: string | undefined) {
  switch (granularity) {
    case 'Minute1':
      return '1m';
    case 'Minute5':
      return '5m';
    case 'Minute15':
      return '15m';
    case 'Hour1':
      return '1h';
    case 'Hour4':
      return '4h';
    case 'Hour6':
      return '6h';
    case 'Day1':
      return '1d';
    default:
      return granularity;
  }
}

function granularitySeconds(granularity: string) {
  const match = granularity.trim().match(/^(\d+)(m|h|d|w|mo)$/i);
  if (!match) return null;
  const amount = Number(match[1]);
  const unit = match[2].toLowerCase();
  if (!Number.isFinite(amount) || amount <= 0) return null;
  if (unit === 'm') return amount * 60;
  if (unit === 'h') return amount * 3_600;
  if (unit === 'd') return amount * 86_400;
  if (unit === 'w') return amount * 604_800;
  if (unit === 'mo') return amount * 30 * 86_400;
  return null;
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <fieldset className="border border-border rounded p-4">
      <legend className="px-2 text-text-3 text-[12px]">{title}</legend>
      {children}
    </fieldset>
  );
}

function Row({ children }: { children: React.ReactNode }) {
  return <div className="flex gap-3">{children}</div>;
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block text-[12px] text-text-3 flex-1">
      <div className="mb-1">{label}</div>
      {children}
    </label>
  );
}

function TagInput({
  value,
  onChange,
}: {
  value: string[];
  onChange: (v: string[]) => void;
}) {
  const [draft, setDraft] = useState('');
  return (
    <div className="flex flex-wrap gap-1.5 items-center">
      {value.map((t, i) => (
        <span key={i} className="px-2 py-0.5 rounded border border-border text-[11px]">
          {t}{' '}
          <button
            type="button"
            onClick={() => onChange(value.filter((_, j) => j !== i))}
          >
            ×
          </button>
        </span>
      ))}
      <input
        className="input flex-1 min-w-[120px]"
        value={draft}
        placeholder="+ add tag"
        onKeyDown={(e) => {
          if (e.key === 'Enter' && draft.trim()) {
            e.preventDefault();
            onChange([...value, draft.trim()]);
            setDraft('');
          }
        }}
        onChange={(e) => setDraft(e.target.value)}
      />
    </div>
  );
}
