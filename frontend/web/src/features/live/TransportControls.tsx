// Strategy-pill transport controls (⏸ Pause / ⏹ Stop / ▶ Resume) + the
// inline confirmation expanders (Task B-III / spec §2.7).
//
// Visible button set (spec §2.4):
//   - ⏸ Pause  shown only when ACTIVE
//   - ▶ Resume shown only when PAUSED
//   - ⏹ Stop   shown unless already STOPPED
//
// NO popups. Two inline expanders render UNDER the buttons (within the pill's
// strip area, not as floating overlays):
//   - Pause → "Positions held." + [Flatten positions] / [Keep open].
//   - Stop  → type-to-confirm ("STOP") then [Stop] / [Cancel] (mirrors
//     `agent-runs/HaltStrategyButton`, converted to theme tokens).
//
// All buttons disable while a mutation is in flight (`busy`) so a double-click
// can't double-fire, and stay disabled (with a "Connect wallet to act" tip)
// when `walletDisabled`. Transport errors surface inline (no toast infra).

import { useState, type MouseEvent } from "react";
import type { StripStatus } from "./strip-status";

export interface TransportControlsProps {
  status: StripStatus;
  /** B-III: pause the run. Absent ⇒ button is a disabled placeholder. */
  onPause?: () => void;
  /** B-III: resume the run. Absent ⇒ button is a disabled placeholder. */
  onResume?: () => void;
  /** B-III: open the stop type-to-confirm expander. Absent ⇒ disabled. */
  onStop?: () => void;
  /** Confirm stop (after typing the keyword). */
  onStopConfirm?: () => void;
  /** Dismiss the stop expander without stopping. */
  onStopCancel?: () => void;
  /** From the paused expander: close open positions (run stays paused). */
  onFlatten?: () => void;
  /** From the paused expander: dismiss it; positions remain open. */
  onKeepOpen?: () => void;

  /** Paused → show the "Positions held" [Flatten]/[Keep open] expander. */
  pausedExpanderOpen?: boolean;
  /** A flatten request is in flight/accepted → show "flattening…". */
  flattenPending?: boolean;
  /** Stop type-to-confirm expander is open. */
  stopConfirmOpen?: boolean;
  /** Inline transport error (no toast). */
  error?: string | null;
  /** A mutation is in flight → disable buttons. */
  busy?: boolean;

  /** Label the user must type to confirm a stop (defaults to "STOP"). */
  confirmWord?: string;
  /** Wallet gate: when true, all buttons disabled with "Connect wallet to act". */
  walletDisabled?: boolean;
}

function PauseGlyph() {
  return (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
      <rect x="3.5" y="2.5" width="3.2" height="11" rx="0.8" />
      <rect x="9.3" y="2.5" width="3.2" height="11" rx="0.8" />
    </svg>
  );
}

function PlayGlyph() {
  return (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
      <path d="M4 2.6v10.8a.7.7 0 0 0 1.07.6l8.4-5.4a.7.7 0 0 0 0-1.2L5.07 2a.7.7 0 0 0-1.07.6Z" />
    </svg>
  );
}

function StopGlyph() {
  return (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
      <rect x="3" y="3" width="10" height="10" rx="1.2" />
    </svg>
  );
}

function ctrlClass(tone: "neutral" | "danger"): string {
  const base =
    "inline-flex h-6 w-6 items-center justify-center rounded-sm border transition-colors disabled:cursor-not-allowed disabled:opacity-40";
  const enabled =
    tone === "danger"
      ? "border-border text-text-3 hover:border-danger/50 hover:text-danger"
      : "border-border text-text-3 hover:border-text-3 hover:text-text-2";
  return `${base} ${enabled}`;
}

export function TransportControls({
  status,
  onPause,
  onResume,
  onStop,
  onStopConfirm,
  onStopCancel,
  onFlatten,
  onKeepOpen,
  pausedExpanderOpen = false,
  flattenPending = false,
  stopConfirmOpen = false,
  error = null,
  busy = false,
  confirmWord = "STOP",
  walletDisabled = false,
}: TransportControlsProps) {
  const [typed, setTyped] = useState("");
  const confirmMatches = typed.trim().toUpperCase() === confirmWord.toUpperCase();

  // Stop a control click from also selecting the pill.
  const swallow = (fn?: () => void) => (e: MouseEvent) => {
    e.stopPropagation();
    fn?.();
  };
  const tip = walletDisabled ? "Connect wallet to act" : undefined;
  // No handler wired (B-I placeholder), wallet gate, or mutation in flight.
  const disabled = (fn?: () => void) =>
    walletDisabled || busy || fn === undefined;

  return (
    <div
      className="flex flex-col gap-1.5"
      data-testid="transport-controls"
      // Expander inputs/buttons must not bubble a click up to pill-select.
      onClick={(e) => e.stopPropagation()}
    >
      <div className="flex items-center gap-1">
        {status === "PAUSED" && (
          <button
            type="button"
            aria-label="Resume strategy"
            title={tip ?? "Resume"}
            disabled={disabled(onResume)}
            onClick={swallow(onResume)}
            className={ctrlClass("neutral")}
          >
            <PlayGlyph />
          </button>
        )}
        {status === "ACTIVE" && (
          <button
            type="button"
            aria-label="Pause strategy"
            title={tip ?? "Pause"}
            disabled={disabled(onPause)}
            onClick={swallow(onPause)}
            className={ctrlClass("neutral")}
          >
            <PauseGlyph />
          </button>
        )}
        {status !== "STOPPED" && (
          <button
            type="button"
            aria-label="Stop strategy"
            title={tip ?? "Stop"}
            disabled={disabled(onStop)}
            onClick={swallow(onStop)}
            className={ctrlClass("danger")}
          >
            <StopGlyph />
          </button>
        )}
      </div>

      {/* Pause → "Positions held" inline expander (no popup). */}
      {pausedExpanderOpen && (
        <div
          data-testid="paused-expander"
          className="flex flex-col gap-1.5 rounded-sm border border-warn/40 bg-warn/10 px-2 py-1.5"
        >
          {flattenPending ? (
            <span className="text-[11px] text-warn" data-testid="flatten-pending">
              Flattening positions…
            </span>
          ) : (
            <>
              <span className="text-[11px] text-text-2">Positions held.</span>
              <div className="flex items-center gap-1.5">
                <button
                  type="button"
                  disabled={disabled(onFlatten)}
                  onClick={swallow(onFlatten)}
                  className="rounded-sm border border-danger/40 bg-danger/15 px-2 py-0.5 text-[11px] text-danger disabled:opacity-40"
                >
                  Flatten positions
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={swallow(onKeepOpen)}
                  className="rounded-sm px-2 py-0.5 text-[11px] text-text-3 hover:text-text disabled:opacity-40"
                >
                  Keep open
                </button>
              </div>
            </>
          )}
        </div>
      )}

      {/* Stop → type-to-confirm inline expander (mirrors HaltStrategyButton). */}
      {stopConfirmOpen && (
        <div
          data-testid="stop-confirm-expander"
          className="flex flex-col gap-1.5 rounded-sm border border-danger/40 bg-danger/15 px-2 py-1.5"
        >
          <span className="text-[11px] text-text-2">
            Type <span className="font-mono font-semibold">{confirmWord}</span> to
            stop &amp; close positions.
          </span>
          <div className="flex items-center gap-1.5">
            <input
              type="text"
              value={typed}
              autoFocus
              onChange={(e) => setTyped(e.target.value)}
              placeholder={confirmWord}
              aria-label="Type to confirm stop"
              className="w-24 rounded-sm border border-border bg-surface-card px-2 py-0.5 font-mono text-[11px]"
            />
            <button
              type="button"
              disabled={!confirmMatches || busy}
              onClick={swallow(() => {
                onStopConfirm?.();
                setTyped("");
              })}
              className="rounded-sm border border-danger/40 bg-danger/15 px-2 py-0.5 text-[11px] text-danger disabled:opacity-40"
            >
              Stop
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={swallow(() => {
                onStopCancel?.();
                setTyped("");
              })}
              className="rounded-sm px-2 py-0.5 text-[11px] text-text-3 hover:text-text disabled:opacity-40"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {error && (
        <span
          data-testid="transport-error"
          className="text-[11px] text-danger"
          role="alert"
        >
          {error}
        </span>
      )}
    </div>
  );
}
