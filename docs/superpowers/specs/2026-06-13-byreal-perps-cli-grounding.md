# Byreal perps-CLI grounding — verified surface (2026-06-13)

Verified against `@byreal-io/byreal-perps-cli@0.3.7` (`order market` etc.) by
installing the package and reading `--help` + `-o json catalog`. This corrects
the **speculative** `SubprocessByrealApi` mapping written in PR #962, which used
invented flags that the real CLI does not accept.

## Real command surface (authoritative — from `catalog`)

| Need | Real CLI command |
|---|---|
| Account equity | `account info` |
| Single-coin mark/quote | `signal detail <coin>` (NOT `signal scan`, which is a market-wide scan with no coin arg) |
| Open positions | `position list` (`[coin]` optional filter) — returns `leverage`, `liq_price`, `funding_paid` |
| Market entry | `order market <side> <size> <coin>` + `[--slippage] [--reduce-only] [--tp <price>] [--sl <price>]` |
| Limit entry | `order limit <side> <size> <coin> <price>` + `[--tif Gtc\|Ioc\|Alo] [--reduce-only] [--tp] [--sl]` |
| Set leverage | `position leverage <coin> <leverage> [--cross] [--isolated]` |
| Brackets on open pos | `position tpsl <coin> [--tp] [--sl] [--cancel-tp] [--cancel-sl]` |
| Close at market | `position close-market <coin> [--size] [--slippage]` |

`order market`/`order limit` carry `--tp`/`--sl` **prices** directly on the entry
(no separate algo-leg call, unlike Orderly). Post-only = `order limit --tif Alo`.

## Mismatches in the pre-existing `SubprocessByrealApi` (PR #962)

1. `order market --symbol X --side Y --qty Z --client-id C` → **wrong**; real CLI
   is positional `order market <side> <size> <coin>` with `--reduce-only/--tp/--sl`.
2. `signal scan --symbol X` for mark price → **wrong**; use `signal detail <coin>`.
3. `close-market --symbol X` → **wrong**; real is `position close-market <coin>`.
4. **No client-order-id** on `order market` → venue-side idempotency cannot be
   enforced via client id. Retries are best-effort; read-before-write dedup is a
   follow-up. `client_id` is retained on the trait for the mock/receipt path and
   tracing, but is **not forwarded** to the CLI.

Output JSON schemas (the `data` structs) still reflect the M0 probe's documented
shape and are **not** validated here — that requires live testnet creds (bead
`xvision-ym9v.9`). This grounding fixes **command construction**, which is unit-
tested via pure arg-builder functions.

## Runtime dependency

`SubprocessByrealApi` shells out to `npx -y @byreal-io/byreal-perps-cli@latest`.
Operators self-hosting byreal execution need network access for `npx` (or a
global install: `npm i -g @byreal-io/byreal-perps-cli`). The CLI reads
`BYREAL_PRIVATE_KEY` itself; `BYREAL_NETWORK`/`BYREAL_ACCOUNT` are forwarded.
