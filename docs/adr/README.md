# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for xvision.

## What is an ADR?

An ADR captures a significant architectural decision: what was decided, why, what alternatives were considered, and what consequences follow. It is a durable record — once accepted, the decision and its rationale stay here even after the code evolves, so future contributors understand why the system is shaped the way it is.

## Naming convention

```
ADR-NNNN-short-title-in-kebab-case.md
```

- `NNNN` is a zero-padded four-digit sequence number, assigned in creation order.
- The title is kebab-case, 3–7 words, describing the decision topic — not the outcome.
- Example: `ADR-0001-context-management-trading-agents.md`

## Status values

- **proposed** — under discussion, not yet adopted
- **accepted** — adopted; code or design reflects this decision
- **superseded by ADR-NNNN** — replaced by a later decision; kept for history
- **deprecated** — no longer relevant; kept for history

## Index

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-0001](ADR-0001-context-management-trading-agents.md) | Context management for AI trading agents | accepted | 2026-06-07 |
