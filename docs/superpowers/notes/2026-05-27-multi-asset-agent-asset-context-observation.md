# Observation: multi-asset agent may not know which symbol it's trading

Date: 2026-05-27
Status: open observation — needs investigation, NOT yet confirmed a bug
Found during: the decisions-panel asset-column work (the "duplicate decisions"
report turned out to be per-asset fan-out; see that change). This is a
*separate*, deeper concern surfaced while reading real multi-asset run data.

## What was seen

On the **xvn dev** container, eval run `01KSMAGAHQTC30N7WWSNST30D5`
(BTC/USD + ETH/USD):

1. The decision recorded as the **ETH** leg (decision_index 1) has a
   justification that explicitly talks about **"BTC/USD"**:
   > "BTC/USD shows a clear bullish breakout on the hourly timeframe…"
   while the BTC leg (index 0) reasons about a concrete price ("closed at
   43697"). So the two legs are genuinely distinct LLM generations, but the
   ETH one names the wrong asset.

2. Same-timestamp BTC/ETH pairs have **identical realized PnL** to 4dp:
   - idx 0/1: both `long_open`, both `-3.75` (this part is benign — the
     opening fee is a fixed-dollar notional, price-independent)
   - idx 6/7: both `flat`, both `-95.2695` (a *close*; identical realized
     implies near-identical % returns between the two legs)

3. But the legs **do diverge** elsewhere: at 2024-02-05T15:00 BTC `short_open`
   (+89.49) while ETH goes `flat` (-5.54). So data routing is not wholesale
   broken — the agents make different calls at the same timestamp.

## Hypotheses

- **(a) Fixture/scenario data.** This dev run's BTC and ETH bar series may be
  a shared or scaled copy (common in synthetic scenarios), which would make
  same-timestamp realized PnL coincide. If so, nothing is wrong.
- **(b) Per-asset prompt doesn't thread the symbol.** The agent prompt/inputs
  built per asset in the `'asset` fan-out may not clearly state which
  `AssetSymbol` the agent is deciding on, so the model defaults to naming BTC.
  If true, "ETH" decisions are being reasoned under a BTC framing — a real
  multi-asset correctness problem, even though positions still diverge.

## Where to look

- `crates/xvision-engine/src/eval/executor/backtest.rs` — the per-asset loop
  (`'asset: for (&asset_sym, &i) in assets_at_ts.iter()`, ~line 884) and how
  the trader prompt / `build_bar_history` / inputs are assembled per asset.
  Confirm the asset symbol is passed into the prompt the agent sees.
- The scenario's bar data for both assets (are BTC and ETH distinct series?).
- Whether the trader model is told the symbol at all in multi-asset mode.

## Not in scope of the decisions-panel fix

The ASSET column + per-step numbering (the change this note rode in with) is
purely presentational and does not touch decision production. This observation
is orthogonal and should be picked up as its own investigation if multi-asset
agent fidelity matters for a real-data scenario.
