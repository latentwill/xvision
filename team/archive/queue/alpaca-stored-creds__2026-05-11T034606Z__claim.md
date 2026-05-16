---
from: alpaca-stored-creds
to: all
topic: claim
created_at: 2026-05-11T03:46:06Z
ack_required: false
---

# `alpaca-stored-creds` track claimed (post-v1-gaps UX work)

Operator request: stop requiring `APCA_API_KEY_ID` / `APCA_API_SECRET_KEY`
to be re-exported every shell session. Move Alpaca credentials into
`Settings → Brokers` with persistent storage.

Branch `feature/alpaca-stored-creds` based on `origin/main` @ `4f1c470`.

## Scope

- **Engine `api::settings::brokers`**: new `AlpacaCredentials`,
  `SetAlpacaReq`, `AlpacaStored` types + `set_alpaca` / `clear_alpaca` /
  `load_alpaca_credentials` fns. Writes `$XVN_HOME/secrets/brokers.toml`
  with file mode 0600 (same security posture as `identity/signing.key`).
  `BrokerEntry` gains `stored` + `stored_key_id_suffix` (last-4 of key id;
  secret never returned).
- **Engine `api::eval::run`**: `build_alpaca_paper_broker` helper that
  prefers stored creds, falls back to env vars, returns a clear
  `ApiError::Validation` ("configure in Settings → Brokers, or export…")
  if neither.
- **Dashboard**: `POST /api/settings/brokers/alpaca` (201) and
  `DELETE /api/settings/brokers/alpaca` (204).
- **Frontend**: replace read-only env-presence card with an editable
  `AlpacaBrokerCard` — key-id text input, secret password input, optional
  base URL, "Save" / "Replace" / "Clear" actions. Env-var fallback status
  still visible behind a `<details>` collapse.

## Security posture

Plaintext-on-disk, owner-only. README + MANUAL already cover the alpha
warning. Matches the existing `~/.xvn/identity/signing.key` file. Not OS
keychain; that's a v1.1 follow-up if we want it. Audit log records
set/clear ops with only the redacted suffix.

## Tests

- 7 engine unit tests (`api::settings::brokers::tests`) — round-trip,
  validation, overwrite, clear, idempotent-clear, get-reports-stored,
  mode-0600 check
- 4 dashboard integration tests (`tests/brokers_routes.rs`) — POST creates,
  GET returns redacted, secret never in response body, validation rejects
  empty fields, DELETE clears, DELETE idempotent
- `tsc -b` + `vite build` clean
- `cargo build --workspace` clean

## Non-conflicts

Touches `api::settings::brokers` (read-only before this PR) and the
brokers UI route. No overlap with Track F (Danger) — different module.
