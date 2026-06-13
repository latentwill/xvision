# Byreal credentials in Settings (move off env vars) — design

Goal: let an operator enter byreal credentials in **Settings → Brokers** instead
of only via `BYREAL_*` env vars, mirroring the existing Alpaca stored-creds
model. Driven by the operator request to "move off env var and into settings."

## Non-custodial constraint (hard requirement)

The landing-page claim is *"xvision signs orders with a scoped key it can't
withdraw with."* Therefore the byreal key we store MUST be a **Hyperliquid
agent / API wallet key (trade-only, cannot withdraw)** — never the master
account key. We:
- Label the settings field **"Trading-only agent key"** and say in help text it
  must be a Hyperliquid API/agent wallet that cannot withdraw.
- Store it encrypted-at-rest exactly like the Orderly/Alpaca scoped keys
  (`$XVN_HOME/secrets/brokers.toml`, mode 0600), never returned by the read API
  (only a `last4` suffix surfaces).
- (Confirmed by operator: byreal key IS an agent/trade-only key.)

## Backend (mirror Alpaca in `crates/xvision-engine/src/api/settings/brokers.rs`)

- `ByrealCredentials { private_key, network: Option, account: Option }` →
  `[byreal]` table in `brokers.toml`.
- `SetByrealReq` / `ByrealStored` (redacted summary).
- `byreal_entry(stored)` reports `configured = env OR stored`, `stored`, and a
  `last4(private_key)` suffix (mirror `alpaca_entry`).
- `set_byreal` / `clear_byreal` (audit-logged with the key redacted to last4).
- `resolve_byreal_credentials(xvn_home)` → stored-wins-over-env, returns
  `{ private_key, network, account, source }`.

## Runtime resolution (so stored creds actually drive orders)

`SubprocessByrealApi` shells to the perps CLI, which reads `BYREAL_PRIVATE_KEY`
from ITS env. To use stored creds without mutating the global process env:
- `SubprocessByrealApi` gains optional `private_key`/`network`/`account` and
  injects them per-subprocess via `Command::env(...)` (falls back to inherited
  env when unset — current behavior).
- The live-eval builder (`build_live_executor` in `api/eval.rs`) calls
  `resolve_byreal_credentials` and constructs the surface with the resolved
  creds. `ByrealLiveSurface`/`from_env` keep working (env path unchanged).

## Dashboard routes (`crates/xvision-dashboard/src/routes/settings/brokers.rs`)

- `POST /api/settings/brokers/byreal`  → `set_byreal`
- `DELETE /api/settings/brokers/byreal` → `clear_byreal`

## Frontend (`frontend/web/src/routes/settings/index.tsx` + `api/settings.ts`)

- New `ByrealBrokerCard` (mirror `AlpacaBrokerCard`): a single secret field
  "Trading-only agent key" + optional network (mainnet/testnet) + optional
  account; `setByrealCredentials` / `clearByrealCredentials`.
- Swap `<BrokerCard entry={data.byreal} />` → `<ByrealBrokerCard … />`.
- Regenerate `types.gen` for `SetByrealReq` / `ByrealStored`.

## Out of scope

- Test-connection for byreal (needs the CLI + a funded account) — follow-up.
- The live-order testnet smoke (bead `xvision-ym9v.9`) — separate, operator-creds.
