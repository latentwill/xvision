// Degen Arena deploy strip — inline, full-width (NO right-side box; no popups).
//
// Lets an operator pick the Degen Arena (Hyperliquid via Virtuals) venue and
// paste their trade-only HL API key before going live. Prop-driven; the
// network call is injected via `onDeploy` so the component is fully testable
// without app context.
//
// Venue options: Orderly (existing) | Degen Arena — Hyperliquid via Virtuals.
// Key validation: 0x-prefixed 64-hex private key (128 chars total).

import { useState } from "react";

export type DeployVenue = "orderly" | "degen-arena";

export type DegenNetwork = "testnet" | "mainnet";

export interface DegenDeployPayload {
  apiKey: string;
  accountAddress: string;
  network: DegenNetwork;
}

export interface DegenDeployStripProps {
  /** Currently selected venue (default: "degen-arena"). */
  venue?: DeployVenue;
  /** Called when the operator changes venue selector. */
  onVenueChange?: (v: DeployVenue) => void;
  /** Called when operator clicks "Go live" with a valid key. */
  onDeploy: (payload: DegenDeployPayload) => void;
}

// A valid trade-only HL API key is a 0x-prefixed 64-hex string (66 chars).
const HL_KEY_RE = /^0x[0-9a-fA-F]{64}$/;

// A valid EVM account address is a 0x-prefixed 40-hex string (42 chars).
const ADDR_RE = /^0x[0-9a-fA-F]{40}$/;

function isValidKey(k: string): boolean {
  return HL_KEY_RE.test(k);
}

function isValidAddress(a: string): boolean {
  return ADDR_RE.test(a);
}

export function DegenDeployStrip({
  venue = "degen-arena",
  onVenueChange,
  onDeploy,
}: DegenDeployStripProps) {
  const [apiKey, setApiKey] = useState("");
  const [touched, setTouched] = useState(false);
  const [accountAddress, setAccountAddress] = useState("");
  const [addressTouched, setAddressTouched] = useState(false);
  const [network, setNetwork] = useState<DegenNetwork>("testnet");

  const keyValid = isValidKey(apiKey);
  const addressValid = isValidAddress(accountAddress);
  // Show the inline hint only after the user has typed something but is invalid.
  const showHint = touched && apiKey.length > 0 && !keyValid;
  const showAddressHint =
    addressTouched && accountAddress.length > 0 && !addressValid;

  // Both key AND address must be valid before go-live is enabled.
  const canDeploy = keyValid && addressValid;

  function handleDeploy() {
    if (!canDeploy) return;
    onDeploy({ apiKey, accountAddress, network });
  }

  return (
    <section
      data-testid="degen-deploy-strip"
      className="rounded-card border border-border bg-surface-card"
    >
      {/* ── Header ──────────────────────────────────────────────────── */}
      <header className="border-b border-border px-4 py-2.5">
        <span className="text-[10px] font-mono uppercase tracking-[0.16em] text-text-3">
          Deploy venue
        </span>
      </header>

      <div className="space-y-4 px-4 py-4">
        {/* ── Venue selector ──────────────────────────────────────── */}
        <fieldset>
          <legend className="mb-2 text-[11px] font-medium text-text-2">
            Select venue
          </legend>
          <div
            className="inline-flex overflow-hidden rounded border border-border text-[12px]"
            role="group"
            aria-label="Venue selector"
          >
            <label
              className={`flex cursor-pointer items-center px-3 py-1.5 transition-colors ${
                venue === "orderly"
                  ? "bg-surface-inset font-medium text-text"
                  : "text-text-3 hover:text-text-2"
              }`}
            >
              <input
                type="radio"
                name="deploy-venue"
                value="orderly"
                checked={venue === "orderly"}
                onChange={() => onVenueChange?.("orderly")}
                className="sr-only"
                data-testid="venue-orderly"
              />
              Orderly
            </label>
            <span className="w-px bg-border" aria-hidden="true" />
            <label
              className={`flex cursor-pointer items-center gap-1.5 px-3 py-1.5 transition-colors ${
                venue === "degen-arena"
                  ? "bg-surface-inset font-medium text-text"
                  : "text-text-3 hover:text-text-2"
              }`}
            >
              <input
                type="radio"
                name="deploy-venue"
                value="degen-arena"
                checked={venue === "degen-arena"}
                onChange={() => onVenueChange?.("degen-arena")}
                className="sr-only"
                data-testid="venue-degen-arena"
              />
              {/* ◈ Virtuals mark placeholder — branding SVG is a separate task */}
              <span
                aria-hidden="true"
                className="font-mono text-[11px] text-text-3"
              >
                ◈
              </span>
              Degen Arena — Hyperliquid via Virtuals
            </label>
          </div>
        </fieldset>

        {/* ── Degen Arena detail — only when that venue is selected ── */}
        {venue === "degen-arena" && (
          <div
            data-testid="degen-arena-detail"
            className="space-y-4"
          >
            {/* Docs link */}
            <p className="text-[12px] text-text-3">
              New here?{" "}
              <a
                href="https://degen.virtuals.io/docs"
                target="_blank"
                rel="noopener noreferrer"
                data-testid="virtuals-docs-link"
                className="text-info underline-offset-2 hover:underline"
              >
                Set up your agent on Virtuals
              </a>
            </p>

            {/* API key field */}
            <div className="space-y-1">
              <label
                htmlFor="hl-api-key"
                className="block text-[12px] font-medium text-text-2"
              >
                Trade-only HL API key
              </label>
              <input
                id="hl-api-key"
                type="text"
                data-testid="hl-api-key-input"
                value={apiKey}
                onChange={(e) => {
                  setApiKey(e.target.value);
                  setTouched(true);
                }}
                onBlur={() => setTouched(true)}
                placeholder="0x…"
                autoComplete="off"
                spellCheck={false}
                className={`w-full rounded border bg-surface px-3 py-2 font-mono text-[13px] text-text outline-none transition-colors placeholder:text-text-3 focus:ring-1 focus:ring-info/50 ${
                  showHint
                    ? "border-danger/60 focus:ring-danger/30"
                    : "border-border focus:border-info/40"
                }`}
              />
              {/* Helper text — always visible */}
              <p className="text-[11px] text-text-3" data-testid="api-key-helper">
                Add your key if you haven&apos;t already.
              </p>
              {/* Inline validation hint — non-popup, shown when invalid + touched */}
              {showHint && (
                <p
                  role="alert"
                  data-testid="api-key-hint"
                  className="text-[11px] text-danger"
                >
                  Must be a 0x-prefixed 64-hex private key (66 characters total).
                </p>
              )}
            </div>

            {/* Account address field */}
            <div className="space-y-1">
              <label
                htmlFor="account-address"
                className="block text-[12px] font-medium text-text-2"
              >
                Account address
              </label>
              <input
                id="account-address"
                type="text"
                data-testid="account-address-input"
                value={accountAddress}
                onChange={(e) => {
                  setAccountAddress(e.target.value);
                  setAddressTouched(true);
                }}
                onBlur={() => setAddressTouched(true)}
                placeholder="0x…"
                autoComplete="off"
                spellCheck={false}
                className={`w-full rounded border bg-surface px-3 py-2 font-mono text-[13px] text-text outline-none transition-colors placeholder:text-text-3 focus:ring-1 focus:ring-info/50 ${
                  showAddressHint
                    ? "border-danger/60 focus:ring-danger/30"
                    : "border-border focus:border-info/40"
                }`}
              />
              {/* Inline validation hint — non-popup */}
              {showAddressHint && (
                <p
                  role="alert"
                  data-testid="account-address-hint"
                  className="text-[11px] text-danger"
                >
                  Must be a 0x-prefixed 40-hex EVM address (42 characters total).
                </p>
              )}
            </div>

            {/* Network toggle */}
            <fieldset>
              <legend className="mb-2 text-[11px] font-medium text-text-2">
                Network
              </legend>
              <div
                className="inline-flex overflow-hidden rounded border border-border text-[12px]"
                role="group"
                aria-label="Network selector"
              >
                <label
                  className={`flex cursor-pointer items-center px-3 py-1.5 transition-colors ${
                    network === "testnet"
                      ? "bg-surface-inset font-medium text-text"
                      : "text-text-3 hover:text-text-2"
                  }`}
                >
                  <input
                    type="radio"
                    name="degen-network"
                    value="testnet"
                    checked={network === "testnet"}
                    onChange={() => setNetwork("testnet")}
                    className="sr-only"
                    data-testid="network-testnet"
                  />
                  Testnet
                </label>
                <span className="w-px bg-border" aria-hidden="true" />
                <label
                  className={`flex cursor-pointer items-center px-3 py-1.5 transition-colors ${
                    network === "mainnet"
                      ? "bg-surface-inset font-medium text-text"
                      : "text-text-3 hover:text-text-2"
                  }`}
                >
                  <input
                    type="radio"
                    name="degen-network"
                    value="mainnet"
                    checked={network === "mainnet"}
                    onChange={() => setNetwork("mainnet")}
                    className="sr-only"
                    data-testid="network-mainnet"
                  />
                  Mainnet
                </label>
              </div>
            </fieldset>

            {/* Go live button */}
            <button
              type="button"
              data-testid="go-live-btn"
              disabled={!canDeploy}
              onClick={handleDeploy}
              className={`rounded px-4 py-2 text-[13px] font-medium transition-colors ${
                canDeploy
                  ? "bg-info text-white hover:bg-info/90"
                  : "cursor-not-allowed bg-surface-inset text-text-3"
              }`}
            >
              Map strategy → go live
            </button>
          </div>
        )}
      </div>

      {/* ── Footer ──────────────────────────────────────────────────── */}
      <footer
        className="border-t border-border px-4 py-2"
        data-testid="virtuals-footer"
      >
        <span className="text-[10px] font-mono text-text-3">
          Powered by Virtuals Protocol
        </span>
      </footer>
    </section>
  );
}
