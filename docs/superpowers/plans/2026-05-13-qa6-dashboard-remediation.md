# QA6 Dashboard Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close QA6 dashboard regressions around chat persistence, strategy authoring, settings IA, eval preflight diagnostics, chart parity, and performance.

**Architecture:** Fix state contracts at their owning layer: chat sessions stay server-owned and append-only unless explicitly deleted; strategy authoring uses an open name-first form backed by the existing `custom` template; settings routes reflect product IA without exposing internal env plumbing; eval launch preflights scenario fixture/cache availability; chart surfaces share layer/range modules instead of each building partial controls.

**Tech Stack:** Rust workspace (`xvision-engine`, `xvision-dashboard`), Axum, SQLx/SQLite, React 18 + TypeScript + TanStack Query, Vite/Vitest, `lightweight-charts`, `cargo`, `pnpm`.

---

## Tasks

- [x] Add this QA6 item to `FOLLOWUPS.md` as `F38`.
- [x] Fix chat rail `New chat`: preserve history, create a fresh session, clear composer, and keep Strategies route scope stable.
- [x] Make `/strategies/new` a name-first open form using the existing `custom` bundle path by default; expose templates as optional starters only.
- [x] Move Skills from Settings to Agents navigation and hide provider env-var details from provider settings UI.
- [x] Add eval preflight diagnostics for missing scenario fixtures/cache before the run starts.
- [x] Extract Scenario chart layers/range behavior to match the shared run chart controls and add expanded SMA/EMA periods to chart payloads.
- [x] Run focused frontend verification and record verification gaps/performance notes.

## Verification Notes

- `corepack pnpm --dir frontend/web test` passed: 15 files, 33 tests.
- `corepack pnpm --dir frontend/web build` passed after the performance follow-up. Dashboard route modules and the chat rail now lazy-load through dynamic imports, chart-heavy route chunks split out of the initial shell, and Vite no longer emits the >500 kB chunk warning. The largest JS chunk is now `~260 kB` minified / `~84 kB` gzip.
- Rust verification is blocked in this workspace because `cargo` is not installed (`cargo: command not found`). Added Rust coverage for chat session creation and missing-bars eval preflight, but those tests still need to run in CI or a Rust-equipped shell.
