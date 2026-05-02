# 0003 — Related work and differentiation

Status: living doc. Update as the field moves and as we encounter new comparables in the wild.

## Why this exists

Demo prep. The two questions that will be asked at hackathon presentation, in order of likelihood:

1. *"How is this different from TradingAgents?"*
2. *"How does this compare to FinMem?"*

If the answer to either is muttered or hand-waved, the rest of the demo lands soft regardless of how good the Δ-Sharpe number is. This doc is the rehearsed, citation-backed answer.

---

## TradingAgents (Yu et al., December 2024)

**Citation:** Xiao, Y. et al. *TradingAgents: Multi-Agents LLM Financial Trading Framework.* [arXiv:2412.20138](https://arxiv.org/abs/2412.20138). [GitHub: TauricResearch/TradingAgents](https://github.com/TauricResearch/TradingAgents). v0.2.4 released 2025-2026.

**What they do.** Multi-agent LLM trading framework with seven specialized roles modeled on a real trading firm: Fundamentals Analyst, Sentiment Analyst, News Analyst, Technical Analyst, Researcher, Trader, Risk Manager. Each role is a distinct LLM call (or chain) with role-specific prompting. Risk profiles for the Trader role ("conservative," "aggressive") are expressed via prompt instructions. The framework reports improvements in cumulative returns, Sharpe ratio, and max drawdown over baselines on US equities.

**What we share with them.**
- Multi-stage architecture with role separation.
- A "Trader" role distinct from analysis roles.
- Structured handoffs between stages.
- Recognition that "character" or "risk profile" is a meaningful axis of variation.

**What we do differently.** TradingAgents encodes risk profile *textually* — via prompts like "you are a conservative trader, respond accordingly." We encode disposition *geometrically* — via control vectors injected into the model's hidden states at inference time, learned from contrastive activation pairs.

This is a real, narrow, testable claim. It's not "ours is better." It's "ours is the same kind of question approached at a different layer of the stack — prompt-conditioning vs hidden-state intervention — and that mechanistic difference matters because it's independently ablatable."

**Why the mechanistic difference matters in one sentence:**

> Prompt-conditioning and steering vectors are orthogonal interventions. You can run prompt-only, steering-only, or both, on the same base model — and so the contribution of "character" to a decision can be cleanly attributed for the first time. With prompts alone, you can't separate the model's general competence from the role-instruction's effect.

**Concrete differentiation talking points:**

- **Ablatability:** TradingAgents cannot run "same prompt, different character" with mechanistic isolation — the character *is* the prompt. We can run "same prompt, vectors-on/off" and "same vectors, different prompts." That's a clean experimental setup.
- **Scope:** They cover fundamentals, sentiment, news, technical. We focus on technicals + onchain. Narrower domain, deeper mechanistic claim. Not a fair head-to-head; we don't claim to beat them on equities.
- **Innovation framing:** Theirs is *organizational* (firm-as-multi-agent). Ours is *representational* (disposition-as-geometry).
- **What we'd lose to them on:** breadth of inputs, equities domain, multi-modal signal aggregation. Acknowledge this if asked.

**Why we don't try to integrate or copy their architecture for v1:** the multi-analyst pattern is good but it dilutes the experimental signal. Our claim is about a single dispositional intervention. Adding more analyst roles makes the experiment harder to attribute. Post-hackathon, if the vector hypothesis holds, integrating their multi-analyst frame as additional Intern voices is a natural v2 extension.

---

## FinMem (Yu et al., NeurIPS 2024 / FinLLM)

**Citation:** *FinMem: A Performance-Enhanced LLM Trading Agent with Layered Memory and Character Design.* [GitHub: pipiku915/FinMem-LLM-StockTrading](https://github.com/pipiku915/FinMem-LLM-StockTrading).

**What they do.** LLM trading agent with three modules: Profiling (character), Memory (layered: short / medium / long-term), Decision-making. The character system is intended to inject trader-style variation. Memory accumulates from past trades and is retrieved as text context for new decisions.

**What we share.**
- Explicit "character" or "profile" concept. They call it Character Design; we call it disposition. Same target, different mechanism.
- Recognition that experience should compound.

**What we do differently.**
- **Memory:** they retrieve text. We don't have episodic memory in v1. (We've discussed this earlier — disposition vectors are not episodic memory; they're orthogonal. Memory is a v2 addition.)
- **Character:** they encode via prompt and retrieved exemplars. We encode via control vector geometry.
- **Self-improvement:** their memory layer naturally compounds. Our planned Karpathy loop would compound *vector updates* from trade outcomes — geometric, not textual. Both are deferred from v1 but the FinMem version is text-retrieval-based; ours would be steering-direction-based.

**Talking point:** "FinMem stores experience as text and retrieves it. We aim to store experience as steering direction. The retrieval-vs-steering distinction is the same one Anthropic's interpretability work makes between memorization and generalization at the activation level."

---

## Other comparables worth knowing

**FinGPT** ([HKUDS](https://github.com/AI4Finance-Foundation/FinGPT)). LLM fine-tuned for financial NLP tasks. Different layer of the stack — they fine-tune model weights for finance domain knowledge. We use general-purpose Qwen and shape disposition at inference. Complementary not competitive; FinGPT-style domain fine-tuning could be combined with our control vectors in principle.

**QuantAgent** ([Wang et al., 2024](https://openreview.net/pdf/873b287eb460fbd3ca55b52474ab8b4256296938.pdf)). Price-driven multi-agent LLMs. Similar to TradingAgents in shape, more focused on price action. Same mechanism distinction applies — prompt-driven character vs steered character.

**FinBen** ([Xie et al., 2024](https://huggingface.co/spaces/finosfoundation/Open-Financial-LLM-Leaderboard)). Benchmark, not an agent. 24 financial tasks across 42 datasets. Useful for model selection but doesn't include our specific use case. Not a comparable, a measurement instrument.

**Freqtrade / Jesse / Hummingbot.** Classical algo trading frameworks. Not LLM agents. Fundamentally different stack — strategy classes with parameterized indicators. Our experiment doesn't claim to beat these; if challenged, the response is "we're testing a mechanistic hypothesis about LLM disposition encoding, not chasing absolute alpha against tuned classical strategies. The question is whether vectors-on outperforms vectors-off on the same setups, which is independent of how either compares to tuned RSI."

---

## The one-sentence positioning

> **xianvec is to TradingAgents as control vectors are to prompt instructions: the same question (how does trader disposition affect outcomes?) addressed at the geometric layer rather than the textual layer, in service of an ablatable experiment about whether dispositional knowledge can be encoded into model inference geometry rather than retrieved as text.**

Memorize this. Be able to deliver it without the qualifications when asked. Add the qualifications only if pushed.

---

## What we'd lose at and how to handle it

- **"Have you tested against TradingAgents on the same data?"** No. Different domain (crypto vs equities), different base model, different timeframe. A head-to-head is post-hackathon work and would require running their stack on our data, which is a project unto itself. Acknowledge cleanly: "We focused the hackathon on the mechanistic question; head-to-head is a clean v2."
- **"What about FinMem's memory advantage?"** Memory is an additive innovation, not a competitor. If memory and steering both work, you compose them. Hackathon scope is steering only.
- **"Why not just fine-tune the model on disposition instead of vectors?"** Fine-tuning encodes disposition into weights non-reversibly. Control vectors are runtime-toggleable, recomposable, regime-conditional. The optionality is the point — and it makes the Karpathy-style update loop tractable in a way fine-tuning doesn't.
- **"How is this different from a temperature knob?"** Temperature affects sampling stochasticity uniformly. Control vectors shift the distribution along a learned semantic axis. They're orthogonal.

---

*Last updated: 2026-05-02.*
