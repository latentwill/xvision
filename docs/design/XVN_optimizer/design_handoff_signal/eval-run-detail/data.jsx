// Mock data for the eval run detail prototype.
window.MOCK = (() => {
  const spans = [
    { id: "s1",  name: "agent.run",                  kind: "agent",      depth: 0, start: 0,    dur: 3400, tokens_in: 0,    tokens_out: 0,    cost: 0.18 },
    { id: "s2",  name: "agent.plan",                 kind: "agent",      depth: 1, start: 20,   dur: 410,  tokens_in: 1240, tokens_out: 312,  cost: 0.014 },
    { id: "s3",  name: "model.call gpt-5-mini",      kind: "model",      depth: 2, start: 40,   dur: 380,  tokens_in: 1240, tokens_out: 312,  cost: 0.014, provider: "openai", model: "gpt-5-mini", hash: "ph_8af21c" },
    { id: "s4",  name: "tool.call load_market_data", kind: "tool",       depth: 1, start: 450,  dur: 220,  cost: 0.000 },
    { id: "s5",  name: "agent.step #14",             kind: "agent",      depth: 1, start: 680,  dur: 1180, decision_idx: 14 },
    { id: "s6",  name: "model.call gpt-5",           kind: "model",      depth: 2, start: 700,  dur: 720,  tokens_in: 3820, tokens_out: 540,  cost: 0.062, provider: "openai", model: "gpt-5", hash: "ph_1d77ab", streaming: false,
      prompt: "You are an intraday mean-reversion trader. Order book is thinning; VIX +4.2σ. Decide for SPY @ 14:14:14.330.",
      response: "Take a half-Kelly long at 4218 — mean-reversion signal confirmed by bid-side liquidity reclaim. Risk: stop at 4204; target +0.6%. Conviction 0.82." },
    { id: "s7",  name: "tool.call run_backtest",     kind: "tool",       depth: 2, start: 1430, dur: 410,  cost: 0.000,
      args: { symbol: "SPY", window: "10:00-14:14", strategy: "mean-reversion-v3" },
      result: { trades: 7, win_rate: 0.71, sharpe_24h: 2.31 } },
    { id: "s8",  name: "model.call gpt-5",           kind: "model",      depth: 2, start: 1850, dur: 690,  tokens_in: 2110, tokens_out: 488,  cost: 0.048, provider: "openai", model: "gpt-5", hash: "ph_44ec02", streaming: true,
      prompt: "Confirm placement size for SPY long? Current book depth 14k @ best bid; latency to venue 18ms.",
      response_partial: "Place 220 shares (≈$92.8k notional). Use IOC + post-only fallback. Cancel if not filled in 800ms.…" },
    { id: "s9",  name: "tool.call place_order",      kind: "tool",       depth: 2, start: 2550, dur: 90,   cost: 0.000 },
    { id: "s10", name: "artifact.write trade.json",  kind: "artifact",   depth: 1, start: 2660, dur: 60,   cost: 0.000 },
    { id: "s11", name: "supervisor.review",          kind: "supervisor", depth: 1, start: 2730, dur: 540,  cost: 0.011 },
    { id: "s12", name: "model.call claude-haiku",    kind: "model",      depth: 2, start: 2750, dur: 510,  tokens_in: 4400, tokens_out: 180,  cost: 0.011, provider: "anthropic", model: "claude-haiku-4-5", hash: "ph_92b41e" },
  ];
  const decisions = [
    { i: 11, t: "10:14:02.118", phase: "engaged",  action: "HOLD", conv: 0.41, just: "VIX climbing but liquidity intact; defer.",          pnl: 0       },
    { i: 12, t: "10:14:05.220", phase: "filtered" },
    { i: 13, t: "10:14:08.402", phase: "engaged",  action: "SELL", conv: 0.68, just: "Order book thinning on SPY; trim 30%.",               pnl: +1240   },
    { i: 14, t: "10:14:10.117", phase: "filtered" },
    { i: 15, t: "10:14:11.917", phase: "engaged",  action: "SELL", conv: 0.74, just: "Bid/ask spread widening — protective trim.",          pnl: +860    },
    { i: 16, t: "10:14:14.330", phase: "engaged",  action: "BUY",  conv: 0.82, just: "Mean-reversion signal at 4218; sized to half-Kelly.", pnl: +2150   },
    { i: 17, t: "10:14:15.880", phase: "filtered" },
    { i: 18, t: "10:14:17.604", phase: "engaged",  action: "HOLD", conv: 0.52, just: "Awaiting confirmation candle.",                       pnl: 0       },
    { i: 19, t: "10:14:19.402", phase: "filtered" },
    { i: 20, t: "10:14:21.288", phase: "engaged",  action: "BUY",  conv: 0.71, just: "Volume reclaim; add to long.",                        pnl: +540    },
    { i: 21, t: "10:14:25.011", phase: "engaged",  action: "SELL", conv: 0.63, just: "Hit +0.6% target band; partial close.",               pnl: +1830   },
    { i: 22, t: "10:14:27.144", phase: "filtered" },
    { i: 23, t: "10:14:28.760", phase: "engaged",  action: "HOLD", conv: 0.49, just: "Macro headline pending; pause new entries.",          pnl: 0       },
  ];
  return { spans, decisions };
})();
