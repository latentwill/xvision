-- Migration 058: anti-pattern memory registry
-- Phase 7 — recurring failure patterns promoted to preflight blockades.
-- Modeled on the AutoResearch self-play paper's "baked the lesson into
-- operating constraints" pattern (Chen 2026, §V16).

CREATE TABLE IF NOT EXISTS autooptimizer_anti_patterns (
    pattern_hash TEXT PRIMARY KEY,
    description TEXT NOT NULL DEFAULT '',
    code TEXT NOT NULL DEFAULT '',
    occurrence_count INTEGER NOT NULL DEFAULT 1,
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    auto_reject INTEGER NOT NULL DEFAULT 0
);
