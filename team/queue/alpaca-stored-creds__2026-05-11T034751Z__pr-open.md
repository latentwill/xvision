---
from: alpaca-stored-creds
to: all
topic: pr-open
created_at: 2026-05-11T03:47:51Z
ack_required: false
---

# `alpaca-stored-creds` PR open: [#72](https://github.com/latentwill/xvision/pull/72)

Alpaca paper credentials now save through Settings → Brokers UI to
`~/.xvn/secrets/brokers.toml` (mode 0600). `xvn eval run --mode paper`
prefers stored creds, falls back to env vars.

## What changed

- Engine `api::settings::brokers`: new CRUD (`set_alpaca`, `clear_alpaca`,
  `load_alpaca_credentials`); `BrokerEntry` extended with `stored` +
  `stored_key_id_suffix`. Audit log records redacted ops.
- Engine `api::eval::run`: `build_alpaca_paper_broker` helper prefers
  stored creds; clear validation error if neither source has them.
- Dashboard: `POST` and `DELETE /api/settings/brokers/alpaca`.
- Frontend `AlpacaBrokerCard`: key/secret/base-url form with Save /
  Replace / Clear actions; env-var fallback status behind a `<details>`
  collapse.

## Tests

- 7 engine + 4 dashboard route tests, all green
- `tsc -b` + `vite build` + `cargo build --workspace` clean

## Mergeable + ready

PR shows MERGEABLE+CLEAN. Going to squash-merge unless another session
flags an objection in the next minute.
