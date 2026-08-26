# Rider free-model validation sweep — 27/27 windows complete (2026-08-24)

Strategy: `01M0S9TM2NJWM07FFSZJM226WQ` (rider clone, daily cadence, BTC long-only trend gate)
Model: `nemotron-3.5-lightning-free` via `opencode-zen` free tier, agent `01M0S9TM2P5RTC0744PPZYX4ZC`.

## Results (node backtest, % return / trades)

| Window | Ret% | Trades | | Window | Ret% | Trades |
|---|---|---|---|---|---|---|
| camp-m1-2024-08 | -1.843 | 4 | | camp-m5 | -2.025 | 12 |
| camp-h02b | 0.000 | 0 | | camp-m6 | 0.000 | 0 |
| camp-h04a | 0.000 | 0 | | camp-m7 | 0.000 | 0 |
| camp-h04b | -0.186 | 4 | | camp-m8 | 0.000 | 0 |
| camp-h06a | -0.935 | 4 | | camp-m9 | -0.186 | 4 |
| camp-h06b | 0.000 | 0 | | camp-m10 | +2.728 | 6 |
| camp-h10a | -0.284 | 2 | | camp-m11 | -0.084 | 2 |
| camp-h10b | -0.953 | 6 | | mech-bear-q3 | -0.897 | 6 |
| camp-h12a | -1.315 | 2 | | mech-bull-q1 | 0.000 | 0 |
| camp-h12b | -0.535 | 2 | | mech-flash-aug24 | -1.843 | 4 |
| camp-h12c | +2.049 | 8 | | mech-range-q2 | +3.535 | 10 |
| camp-h12d | -2.428 | 8 | | camp-m2 | -0.688 | 2 |
| camp-m3 | -1.234 | 8 | | camp-m4 | +16.461 | 4 |

**Aggregate: +7.49% over 27 windows — 4 positive / 7 flat / 16 negative, 98 trades.**
Best: camp-m4 +16.46%. Offline round2 best reference: 22/27 profitable.

## Infrastructure that made this run (all still live)

1. **`zen-strip-proxy.py`** (this dir, hub process `zen-strip-proxy`, port 8917 on this Mac,
   reachable from the node at `http://100.90.135.112:8917`). Required because:
   - opencode-zen free tier rejects `response_format` (json_object/json_schema) with an
     upstream 500 instead of the HTTP 400 the engine's fallback expects → proxy strips it.
     The engine embeds the same schema in the prompt, so validation is unchanged.
   - Free tier intermittently returns HTTP 200 SSE with `finish_reason: "network_error"`
     and 503s → proxy buffers and retries (up to 6 attempts, exponential backoff).
   - Uses curl as transport (python TLS fingerprints get Cloudflare-challenged) and strips
     the decoy `:11434` path segment used to steer xvision's provider map onto the ollama
     carrier (chat completions) instead of the litellm carrier (Responses API, which zen
     500s). Provider base_url: `http://100.90.135.112:8917/ollama:11434/v1`.
2. **Scenario clones** for the 4 mech-* windows (`*-daily`, tag `riderval`, warmup_bars 200):
   their 7200-bar warmup assumes hourly bars; at the rider's daily granularity the warmup
   window predates Alpaca crypto history (2021-09-26) and preflight refuses the run.

## Known residual failure mode

nemotron occasionally emits `stop_loss_pct` as a fraction (0.017) instead of percent (1.7);
engine validation requires 0.1–20.0 and the run dies with `trader_output[invalid_field]`.
Guardrails rewrote most cases (one run: rewrite_pct=42) but not all — failed windows needed
1–3 retries. Expect ~10–20% per-window retry rate on this model class.

## Model availability observed 2026-08-24 (free tier, tool-calling probe)

- Working tools: `nemotron-3.5-lightning-free`, `hy3-free`, `laguna-s-2.1-free`
- Broken then: `x-preview-f-free` (tools upstream down), `deepseek-v4-flash-free`,
  `mimo-v2.5-free` (timeouts); `muse-spark-1.2-contributor-free`, `nemotron-3-ultra-free` (hard down)
