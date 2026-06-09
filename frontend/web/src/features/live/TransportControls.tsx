// Strategy-pill transport-control buttons (⏸ Pause / ⏹ Stop / ▶ Resume).
//
// B-I scope: render the BUTTONS with correct icons + a clean prop seam.
// They are no-op placeholders here — `onPause`/`onResume`/`onStop` are
// optional and the buttons are DISABLED until B-III wires the transport
// behavior (and whenever the wallet is not connected). The visible-set
// rule follows spec §2.4:
//   - ⏸ Pause  shown only when ACTIVE
//   - ▶ Resume shown only when PAUSED
//   - ⏹ Stop   shown unless already STOPPED
//
// No popups: any confirmation flow B-III adds must inline-expand on the
// pill, not open a modal.

import type { MouseEvent } from "react";
import type { StripStatus } from "./strip-status";

export interface TransportControlsProps {
  status: StripStatus;
  /** B-III seam: pause the run. Absent ⇒ button is a disabled placeholder. */
  onPause?: () => void;
  /** B-III seam: resume the run. Absent ⇒ button is a disabled placeholder. */
  onResume?: () => void;
  /** B-III seam: stop the run. Absent ⇒ button is a disabled placeholder. */
  onStop?: () => void;
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
  walletDisabled = false,
}: TransportControlsProps) {
  // Stop a control click from also selecting the pill.
  const swallow = (fn?: () => void) => (e: MouseEvent) => {
    e.stopPropagation();
    fn?.();
  };
  const tip = walletDisabled ? "Connect wallet to act" : undefined;
  // B-I: no handler wired ⇒ disabled placeholder. B-III supplies handlers.
  const disabled = (fn?: () => void) => walletDisabled || fn === undefined;

  return (
    <div className="flex items-center gap-1" data-testid="transport-controls">
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
  );
}
