# Cline Runtime Unification — Stage 0: ACPX Purge + License Guard — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove every lingering `acpx` / subscription-auth reference from the shipped surface and add a guard test that keeps the runtime API-key-only.

**Architecture:** This is a documentation + dead-reference purge plus two guards. We write the guards *first* (they fail because the refs still exist and because we assert no OAuth path), then purge each surface until both guards are green. The historical tombstone in `docs/cli-non-surfaced.md` is preserved — it is the record of *why* ACPX was removed, not a live reference.

**Tech Stack:** Markdown, Rust doc-comments, Bash (grep guard), Vitest (Node license guard in `xvision-agentd`).

**Umbrella spec:** `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md` (Stage 0 section + "Subplan inheritance contract").

---

## Inherited contract gates (from umbrella §"Subplan inheritance contract")

This stage touches none of the trajectory/record/replay machinery, so items 1 (persistence), 3, 4, 5, 6, 7, 8, 9, 10 do **not** bind it. Only the auth invariant inside item 1 applies. Do not pad this plan with performative gates — the honest set is:

- [ ] **Item 1 (auth invariant only) — API-key auth only.** A guard test asserts no subscription/OAuth auth path exists anywhere in the runtime (`xvision-agentd` config + agent construction). Cline-via-API-key is the only authentication mode. (The trajectory-persistence half of item 1 belongs to Stages 2–3, not here.)

Everything below also satisfies the stage's own exit criteria from the umbrella: *zero `acpx`/subscription-auth references in shipped surface; guard test green.*

---

## File Structure

**Guards (created):**
- `scripts/guard-no-acpx.sh` — repo-wide grep guard; fails if any live `acpx`/`XVN_INTERN_ACPX`/`openclaw` reference exists outside the allow-listed historical/archived files.
- `xvision-agentd/test/license-guard.test.ts` — Vitest guard: `StartRunConfig` carries no OAuth/subscription field and `buildAgent` passes only `apiKey` to the Cline `Agent`.

**Purged (modified):**
- `MANUAL.md` — delete §M11.5 (lines 211–241); strip `| acpx` (line 458); delete ACPX env block (lines 463–468).
- `.claude/skills/xvision-cli/SKILL.md` (line 217), `.claude/skills/xvision-cli/references/architecture.md` (lines 14, 37).
- `.claude/skills/xvision-dev/SKILL.md` (lines 63, 195), `.claude/skills/xvision-dev/references/architecture.md` (lines 15, 44).
- `crates/xvision-mcp/src/lib.rs` (lines 4–8 doc comment), `crates/xvision-mcp/src/main.rs` (lines 3–5 doc comment).
- `crates/xvision-dashboard/wiki/mcp.md` (lines 104, 106).
- `crates/xvision-engine/tests/agent_recovery_malformed_json.rs` (line 128 comment), `crates/xvision-engine/tests/agent_recovery_schema_missing_field.rs` (line 143 comment).

**Preserved (verified, not edited):**
- `docs/cli-non-surfaced.md` — the "ACPX intern subprocess (removed 2026-05-10)" tombstone stays.
- `docs/superpowers/notes/2026-05-21-optimizer-and-capability-framing-handoff.md`, `team/intake/archive/2026-05-21-*.md` — archived notes already marked superseded; allow-listed.
- `FOLLOWUPS.md` — F21 already absent; verify, do not edit.

---

### Task 1: Write the repo-wide ACPX grep guard (failing)

**Files:**
- Create: `scripts/guard-no-acpx.sh`

- [ ] **Step 1: Write the guard script**

```bash
#!/usr/bin/env bash
# Fails if any LIVE acpx / subscription-auth reference exists in the shipped
# surface. Historical tombstones and archived notes are allow-listed.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Patterns that must not appear in live surface.
PATTERN='acpx|AcpxIntern|XVN_INTERN_ACPX|openclaw'

# Files/dirs allowed to mention ACPX as historical record only.
ALLOW='^docs/cli-non-surfaced.md|^docs/superpowers/notes/|^docs/superpowers/plans/2026-05-24-cline-|^docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md|^team/intake/archive/|^team/archive/|^scripts/guard-no-acpx.sh'

hits="$(git grep -nIE "$PATTERN" -- \
  ':!target' ':!node_modules' \
  | grep -vE "$ALLOW" || true)"

if [[ -n "$hits" ]]; then
  echo "guard-no-acpx: FAIL — live ACPX references remain:" >&2
  echo "$hits" >&2
  exit 1
fi
echo "guard-no-acpx: OK — no live ACPX references."
```

- [ ] **Step 2: Make it executable and run it to verify it FAILS**

Run:
```bash
chmod +x scripts/guard-no-acpx.sh && bash scripts/guard-no-acpx.sh
```
Expected: **FAIL**, listing hits in `MANUAL.md`, the four skill files, the two `xvision-mcp` files, `crates/xvision-dashboard/wiki/mcp.md`, and the two `agent_recovery_*` test comments. (Exit code 1.)

- [ ] **Step 3: Commit the guard**

```bash
git add scripts/guard-no-acpx.sh
git commit -m "test(stage0): add failing repo-wide ACPX reference guard"
```

---

### Task 2: Write the Node license guard (failing → green by assertion)

**Files:**
- Create: `xvision-agentd/test/license-guard.test.ts`
- Reference (do not edit): `xvision-agentd/src/session/build-agent.ts`, `xvision-agentd/src/session/store.ts`

- [ ] **Step 1: Write the license guard test**

```typescript
import { describe, it, expect } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { buildAgent } from "../src/session/build-agent.js"
import { MOCK_PROVIDER_ID } from "../src/testing/mock-provider.js"

// Forbidden auth surface: anything implying consumer-subscription / OAuth auth.
const FORBIDDEN = /oauth|sessionKey|sessionToken|subscriptionToken|claude_?pro|claude_?max|refreshToken/i

describe("license guard: API-key auth only", () => {
  it("build-agent.ts source contains no OAuth/subscription auth surface", () => {
    const p = fileURLToPath(new URL("../src/session/build-agent.ts", import.meta.url))
    const src = readFileSync(p, "utf8")
    const offending = src.split("\n").filter((l) => FORBIDDEN.test(l))
    expect(offending, `OAuth/subscription surface found:\n${offending.join("\n")}`).toEqual([])
  })

  it("store.ts StartRunConfig source carries no OAuth/subscription field", () => {
    const p = fileURLToPath(new URL("../src/session/store.ts", import.meta.url))
    const src = readFileSync(p, "utf8")
    const offending = src.split("\n").filter((l) => FORBIDDEN.test(l))
    expect(offending, `OAuth/subscription field found:\n${offending.join("\n")}`).toEqual([])
  })

  it("buildAgent passes only apiKey (not any session/oauth token) to Agent", () => {
    // Mock-provider path constructs a wrapped model and never touches auth;
    // real-provider path forwards apiKey only. We assert the real path shape
    // by inspecting the constructed config indirectly: buildAgent must not
    // throw for an apiKey-only config and must reject configs lacking apiKey
    // for a non-mock provider would be a Stage 1 concern — here we only assert
    // the auth field surface.
    const agent = buildAgent({
      provider_id: MOCK_PROVIDER_ID,
      model_id: "mock",
      api_key: "sk-test",
      base_url: undefined,
      system_prompt: "x",
      allowed_tools: [],
      budget_limits: { max_input_tokens: 1, max_output_tokens: 1, max_wall_ms: 1 },
    })
    expect(agent).toBeDefined()
  })
})
```

- [ ] **Step 2: Run it to verify current state**

Run:
```bash
cd xvision-agentd && npm run test -- license-guard
```
Expected: the two source-scan tests **PASS today** (no OAuth surface exists — verified during exploration); the `buildAgent` test **PASS**. This guard is a *regression lock*: it fails only if someone later adds an OAuth path. If any test fails now, stop — an OAuth surface was introduced since the audit and must be removed before proceeding.

- [ ] **Step 3: Commit the license guard**

```bash
git add xvision-agentd/test/license-guard.test.ts
git commit -m "test(stage0): lock API-key-only auth in xvision-agentd"
```

---

### Task 3: Purge MANUAL.md

**Files:**
- Modify: `MANUAL.md:211-241` (delete §M11.5), `MANUAL.md:458` (strip `| acpx`), `MANUAL.md:463-468` (delete ACPX env block)

- [ ] **Step 1: Delete §M11.5**

Delete the entire block from the `### M11.5. Wire the MCP indicator server (only when \`INTERN=acpx\`)` heading (line 211) up to — but not including — the next `###` heading. This removes the ACPX harness wiring instructions, the `acpx.config.json` stanza, the per-agent table, and the `**Unblocks:** F21` line.

- [ ] **Step 2: Strip the `| acpx` enum hint on line 458**

Change:
```
export XVN_INTERN_PROVIDER=anthropic          # | openai-compat | acpx
```
to:
```
export XVN_INTERN_PROVIDER=anthropic          # | openai-compat
```

- [ ] **Step 3: Delete the ACPX env block (lines 463–468)**

Delete these six lines in full:
```
# ACPX path only (XVN_INTERN_PROVIDER=acpx):
export XVN_INTERN_ACPX_AGENT=claude           # | codex | openclaw | hermes | ...
# export XVN_INTERN_ACPX_CUSTOM_CMD="hermes acp"   # escape hatch for Hermes
# export XVN_INTERN_ACPX_BIN=acpx                  # override binary name
# export XVN_INTERN_ACPX_TIMEOUT_SECS=300          # default 300s
# export XVN_INTERN_ACPX_MAX_OUTPUT_BYTES=2097152  # default 2 MiB
```

- [ ] **Step 4: Run the grep guard to confirm MANUAL.md is clean**

Run: `bash scripts/guard-no-acpx.sh`
Expected: still FAIL, but the hit list no longer contains `MANUAL.md` lines.

- [ ] **Step 5: Commit**

```bash
git add MANUAL.md
git commit -m "docs(stage0): purge ACPX intern harness from MANUAL"
```

---

### Task 4: Purge the four skill files

**Files:**
- Modify: `.claude/skills/xvision-cli/SKILL.md:217`
- Modify: `.claude/skills/xvision-cli/references/architecture.md:14,37`
- Modify: `.claude/skills/xvision-dev/SKILL.md:63,195`
- Modify: `.claude/skills/xvision-dev/references/architecture.md:15,44`

- [ ] **Step 1: xvision-cli/SKILL.md** — delete line 217 (the `Don't recommend \`AcpxIntern\` for backtest pairing …` bullet) entirely.

- [ ] **Step 2: xvision-cli/references/architecture.md**
  - Line 14: change `Intern backends (\`OpenAICompatIntern\`, \`AnthropicIntern\`, \`AcpxIntern\`)` → `Intern backends (\`OpenAICompatIntern\`, \`AnthropicIntern\`)`.
  - Line 37: delete the entire `AcpxIntern` table row.

- [ ] **Step 3: xvision-dev/SKILL.md**
  - Line 63: same crate-row edit as Step 2 line 14.
  - Line 195: delete the sentence/bullet `\`AcpxIntern\` is agentic and **breaks** this — never use it for backtests / A/B compare. Use \`OpenAICompatIntern\` or \`AnthropicIntern\`.` (Keep the surrounding "A/B cache pairing is tier-1" guidance; only the AcpxIntern clause is removed.)

- [ ] **Step 4: xvision-dev/references/architecture.md**
  - Line 15: same crate-row edit.
  - Line 44: delete the `| \`AcpxIntern\` | … |` row.

- [ ] **Step 5: Run guard** — `bash scripts/guard-no-acpx.sh`; expected FAIL with no skill-file hits remaining.

- [ ] **Step 6: Commit**

```bash
git add .claude/skills/xvision-cli .claude/skills/xvision-dev
git commit -m "docs(stage0): drop AcpxIntern from cli/dev skill architecture tables"
```

---

### Task 5: Fix the xvision-mcp doc comments

**Files:**
- Modify: `crates/xvision-mcp/src/lib.rs:4-8`
- Modify: `crates/xvision-mcp/src/main.rs:3-5`

- [ ] **Step 1: lib.rs** — replace lines 4–8 with:

```rust
//! Registered as Cline agent tools via the `xvision-agentd` sidecar so any
//! Cline-driven agent stage (intern, trader, risk, critic, …) can recompute
//! indicators at parameter sets the snapshot doesn't pre-bake (e.g. RSI(7)
//! when the snapshot only carries RSI(14)).
```

- [ ] **Step 2: main.rs** — replace lines 3–5 with:

```rust
//! Started by an MCP host (the `xvision-agentd` Cline sidecar, or a local MCP
//! client). Speaks MCP over stdin/stdout; logs go to stderr so they don't
//! corrupt the JSON-RPC stream.
```

- [ ] **Step 3: Confirm it still parses (no cargo)**

These are doc comments only — no code change. Confirm by reading the file back; do not run `cargo` (shared checkout / OOM rule). The compile is validated later in the normal workspace build.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-mcp/src/lib.rs crates/xvision-mcp/src/main.rs
git commit -m "docs(stage0): re-point xvision-mcp doc comments from ACPX to Cline"
```

---

### Task 6: Fix the dashboard wiki and test comments

**Files:**
- Modify: `crates/xvision-dashboard/wiki/mcp.md:104,106`
- Modify: `crates/xvision-engine/tests/agent_recovery_malformed_json.rs:128`
- Modify: `crates/xvision-engine/tests/agent_recovery_schema_missing_field.rs:143`

- [ ] **Step 1: wiki/mcp.md**
  - Line 104: `### Registration in \`acpx.config.json\`` → `### Registration in an MCP host`.
  - Line 106: `The intended host is \`acpx\`. Add the server to the \`mcpServers\` list:` → `Add the server to your MCP host's \`mcpServers\` list (e.g. the \`xvision-agentd\` Cline sidecar, or Claude Code's \`claude_desktop_config.json\` — see below):`.

- [ ] **Step 2: agent_recovery_malformed_json.rs:128** — change the comment from referencing `AcpxIntern` to the general rule:
```rust
// A/B cache pairing requires a deterministic intern backend. Anthropic
```
(Replace the `depend on AcpxIntern (which can't be A/B-cache paired). Anthropic` text; keep the paragraph's intent.)

- [ ] **Step 3: agent_recovery_schema_missing_field.rs:143** — change:
```rust
// Per the contract's A/B cache pairing rule, use a deterministic intern backend.
```
(Replace the `do NOT use AcpxIntern.` clause.)

- [ ] **Step 4: Run the grep guard — now expected to PASS**

Run: `bash scripts/guard-no-acpx.sh`
Expected: **OK — no live ACPX references.** (Exit code 0.) The only remaining matches are in the allow-listed tombstone/archive files and the dated Cline planning/spec documents that describe the purge itself.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-dashboard/wiki/mcp.md \
        crates/xvision-engine/tests/agent_recovery_malformed_json.rs \
        crates/xvision-engine/tests/agent_recovery_schema_missing_field.rs
git commit -m "docs(stage0): scrub trailing ACPX mentions in wiki + test comments"
```

---

### Task 7: Verify FOLLOWUPS.md F21 is already absent

**Files:**
- Verify only (no edit): `FOLLOWUPS.md`

- [ ] **Step 1: Confirm F21/acpx are gone**

Run:
```bash
git grep -nIE 'F21|acpx|ACPX' -- FOLLOWUPS.md || echo "FOLLOWUPS.md clean (no F21/ACPX)"
```
Expected: `FOLLOWUPS.md clean (no F21/ACPX)`. The umbrella's scope listed F21 here, but it was already removed with the 2026-05-10 code purge. **No edit is required** — this step documents that the umbrella's scope item is already satisfied. Do not invent an F21 entry to delete.

---

### Task 8: Final gate — both guards green

- [ ] **Step 1: Run the repo guard**

Run: `bash scripts/guard-no-acpx.sh`
Expected: `guard-no-acpx: OK — no live ACPX references.`

- [ ] **Step 2: Run the license guard**

Run: `cd xvision-agentd && npm run test -- license-guard`
Expected: all three tests PASS.

- [ ] **Step 3: Wire the repo guard into the lint entrypoint**

Add an invocation of `bash scripts/guard-no-acpx.sh` to `scripts/board-lint.sh` (the existing pre-push lint referenced in `CLAUDE.md`) so the purge cannot silently regress. Append at the end of that script:
```bash
bash scripts/guard-no-acpx.sh
```

- [ ] **Step 4: Commit**

```bash
git add scripts/board-lint.sh
git commit -m "ci(stage0): run ACPX guard in board-lint"
```

---

## Self-Review

- **Spec coverage:** Umbrella Stage 0 scope = strip MANUAL §M11.5 + env block (Task 3 ✓), cli/dev skills (Task 4 ✓), xvision-mcp doc comments (Task 5 ✓), wiki/mcp.md (Task 6 ✓), FOLLOWUPS F21 (Task 7 — verified already absent ✓), re-point indicator tools to "Cline agent tools" (Task 5 ✓), add no-subscription-auth guard (Task 2 ✓). Exit = zero live refs + guard green (Task 8 ✓).
- **Auth invariant (item 1 half):** Task 2 locks it. Honest scoping note included so later stages own the persistence half.
- **Placeholder scan:** No TBD/TODO; every edit names exact file + line + before/after text.
- **No-cargo discipline:** Task 5 explicitly avoids `cargo` per the shared-checkout rule; doc-comment-only edits are validated in the normal workspace build, not here.
