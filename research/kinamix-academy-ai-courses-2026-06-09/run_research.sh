#!/usr/bin/env bash
set -euo pipefail
BASE=${1:?base dir}
run() {
  local name="$1"; shift
  local prompt="$1"; shift
  local out="$BASE/$name"
  mkdir -p "$out"
  echo "=== running $name ==="
  100x deepseek \
    --prompt "$prompt" \
    --fan-out 3 \
    --agents 6 \
    --budget 4 \
    --output "$out"
}
run openclaw-hermes "Research online courses, workshops, cohorts, tutorials, paid communities, or training products that teach people to set up or use OpenClaw, Hermes Agent, Claude Code, Codex CLI, Cline, OpenHands, or similar coding-agent CLI/workbench systems. Prioritize official landing pages and public course pages. For each relevant course/product, extract: URL, positioning/headline, landing-page copy patterns that work, promise/outcome, target audience, pricing, refund/guarantee if shown, size/length of course, curriculum/content modules, format (self-paced/cohort/live/community), social proof, bonuses, urgency/scarcity, and anything relevant to Kinamix Academy. Separate direct OpenClaw/Hermes matches from adjacent Claude Code/coding-agent courses. Return source-grounded bullets and a synthesis of winning patterns."
run vibe-coding-apps "Research online courses/cohorts/products that teach people to vibe code their own apps using AI coding tools (Cursor, Lovable, Bolt.new, Replit Agent, Claude Code, v0, Windsurf, GitHub Copilot, ChatGPT, no-code/low-code AI builders). For each, extract URL, headline/promise, landing-page copy patterns, pricing, course size/length, curriculum/content, target audience, included templates/projects, community/live support, proof, guarantees, and notable funnels. Focus on courses that help nontechnical or semi-technical people build and ship real apps. End with patterns and gaps Kinamix Academy could exploit."
run ai-use-courses "Research online courses that teach people how to use AI broadly: ChatGPT/Claude productivity, AI for business, prompt engineering, AI workflows, AI automation, AI for creators/operators, AI literacy. Include major visible courses from creators, platforms, and cohort businesses. For each, extract URL, headline/promise, landing-page copy patterns, pricing, size/length, curriculum/content, target audience, format, proof, guarantees, and funnel mechanics. Synthesize what copy/pricing/content patterns work and which are overused."
run adjacent-ai-agent-automation-product "Research adjacent courses relevant to Kinamix Academy: AI agent building, AI automation agencies, n8n/Zapier/Make with AI, LangChain/LangGraph/crewAI agent courses, AI SaaS/product builder cohorts, solopreneur AI app businesses, and AI consulting/implementation courses. For each, extract URL, positioning, landing-page copy patterns, pricing, size/length, curriculum, target audience, format, proof, community/live support, implementation assets, and notable upsells. Synthesize implications for a Kinamix Academy offer around Hermes/OpenClaw/AI agent app building."
