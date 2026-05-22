# xvision Agent Training Pipeline — Research Notes

## Overview

xvision is an AI agent trading strategy platform. A proposed future enhancement involves a self-improving training loop: record agent traces, replace human guidance turns with agent CoT, flatten multi-turn conversations into single long-horizon trajectories with LLM-inferred high-level goals, then SFT on the resulting data to encode decision-making taste into model weights. The loop repeats until agent autonomy benchmarks (METR-style) saturate.

---

## Self-Improving Training Loop

### Core Idea
1. **Record agent traces** — capture full decision trajectories from live or simulated trading sessions.
2. **Replace human guidance** — substitute human intervention turns with agent-generated Chain-of-Thought (CoT) reasoning.
3. **Flatten trajectories** — compress multi-turn conversations into single long-horizon trajectories.
4. **Infer high-level goals** — use an LLM to extract overarching objectives from the flattened trajectories.
5. **SFT on processed data** — supervised fine-tuning to bake decision-making taste into model weights.
6. **Benchmark and repeat** — run METR-style autonomy benchmarks; iterate until performance saturates.

---

## Post-Hackathon Implementation Path

### Practical Training Stack
- **QLoRA SFT** on a 7B–13B base model — fits within 24GB VRAM.
- **Artifact**: a LoRA adapter, not a full model weights dump.
- Keeps the barrier low for incremental experimentation without massive compute.

### API-Native Alternative
- **Together.ai** or **Fireworks.ai** accept JSONL trace data and return hosted fine-tuned endpoints.
- Good for teams that want to avoid managing GPU infrastructure.

### Self-Hosting Alternative
- **vLLM** or **Ollama** on a Hetzner GPU instance.
- Full control over inference and adapter serving; cost-effective for sustained usage.

---

## Immediate Pre-Training Substitute: RAG Over Trace Corpus

Before committing to full SFT, the fastest path to better agent decisions is:
- **Retrieve relevant past trajectories** from the trace corpus.
- **Inject as few-shot context** at inference time.

### Why RAG May Outperform Baked Weights for Trading
- **Recency**: market regimes shift; recent trajectories are more relevant than old training data.
- **Specificity**: retrieval can target exact market conditions (asset, timeframe, volatility regime) rather than averaging across all historical decisions.
- **No retraining latency**: new traces are immediately available for retrieval without a fine-tuning cycle.

> Full LLM training is not required at any stage for the MVP.

---

## Longer-Horizon Additions (Post-MVP)

1. **Local training module** — integrate QLoRA SFT into the xvision runtime so teams can train adapters on their own trace data without leaving the platform.
2. **Kapathi auto-researcher integration** — wire the training loop into an autonomous research agent that proposes strategy variants, runs evals, and promotes high-performing adapters automatically.

Both are flagged as well-after-hackathon work.

---

## Open Questions

- What is the exact schema for agent traces? (observation, action, reward, human guidance, CoT?)
- How is decision-making taste operationalized in the SFT loss? (preference pairs, imitation, or goal-conditioned?)
- Which METR-style autonomy benchmarks are most relevant for trading agents? (task completion, profit attribution, risk-adjusted returns?)
- What is the minimum viable trace corpus size before SFT shows gains over base model + RAG?

---

## Source / Inspiration

This research note is directly inspired by:

- **@zero_goliath** — [X post, May 20 2026](https://x.com/zero_goliath/status/2056957915060204007?s=46)

  > whenever your frontier LLM's users think their taste in managing agents gives their labour a comparative advantage, follow these steps:
  > 1. record their agent traces
  > 2. replace the tasteful user messages with agent CoT
  > 3. make the trace a single long horizon trajectory with a single high-level goal (inferred by an LLM critic)
  > 4. sft on the traces to teach the LLM taste
  > repeat until the METR chart breaks

- **@irl_danB** (reply):

  > by the time they commoditized my taste in authoring prompts
  > my taste in managing context had put more distance between us
  > and by the time they commoditized my taste in managing context
  > my taste in managing agents put me still further ahead
  > so by the time you commoditize my taste in managing agents
  > my taste in managing the SFT loop will have grown my lead beyond levels of comprehension

---

## Related

- xvision repo:  (Rust workspace,  CLI, dashboard, remote CLI)
- xvision skill references: 
- Eval warmup boundaries, provider schema compatibility, and paper-mode debugging notes are in the xvision skill references.
