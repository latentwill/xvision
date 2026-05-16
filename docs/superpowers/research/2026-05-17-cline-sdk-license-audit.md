# Cline SDK License Audit

**Date:** 2026-05-17
**Auditor:** Claude (xvision automation)
**Outcome:** DONE_WITH_CONCERNS — conditional PASS with one blocking transitive dependency

---

## Cline package licenses

| Package | Version | npm `license` field | LICENSE file in tarball? | Source repo license |
| --- | --- | --- | --- | --- |
| @cline/sdk | 0.0.41 | `Apache-2.0` | No | Apache-2.0 (sdk/LICENSE) |
| @cline/llms | 0.0.41 | `null` (unset) | No | Apache-2.0 (sdk/LICENSE) |
| @cline/shared | 0.0.41 | `null` (unset) | No | Apache-2.0 (sdk/LICENSE) |
| @cline/core | 0.0.41 | `null` (unset) | No | Apache-2.0 (sdk/LICENSE) |

**Note on null license fields:** `@cline/llms`, `@cline/shared`, and `@cline/core` do not set
the `license` field in their `package.json` and do not bundle a `LICENSE` file in the npm
tarball. The `license-checker` tool therefore falls back to the `README.md` as the license
file, yielding `Custom: <readme-url>` or `UNKNOWN`. However, the source monorepo carries a
dedicated `sdk/LICENSE` file (Apache-2.0, verified below), and all four packages point to
`https://github.com/cline/cline` as their repository. This is a packaging hygiene gap, not
a license conflict — but it means redistributors cannot rely solely on the npm tarball to
establish provenance; they must reference the upstream git repo.

---

## Parent repo

- Repo: https://github.com/cline/cline
- License: **Apache-2.0** (`Apache License 2.0`)
- Verified at: https://api.github.com/repos/cline/cline/license (spdx_id: `Apache-2.0`)
- SDK sub-directory license file: https://raw.githubusercontent.com/cline/cline/main/sdk/LICENSE
  (confirmed: Apache License, Version 2.0)

---

## Transitive dependency license set

241 packages audited (production only, `@cline/sdk@0.0.41`).

Sorted unique license identifiers reported by `license-checker --production`:

```
(AFL-2.1 OR BSD-3-Clause)
0BSD
Apache-2.0
BSD-2-Clause
BSD-3-Clause
Custom: https://github.com/cline/cline/blob/main/README.md
Custom: https://github.com/cline/sdk/tree/main/examples
Custom: https://img.shields.io/badge/Node.js-18
Custom: https://www.npmjs.com/package/
ISC
MIT
UNKNOWN
```

All `Custom:` and `UNKNOWN` entries are explained below.

---

## Notable items

### 1. @anthropic-ai/claude-agent-sdk — PROPRIETARY LICENSE (blocker)

**Package:** `@anthropic-ai/claude-agent-sdk@0.2.141`
**Package:** `@anthropic-ai/claude-agent-sdk-darwin-arm64@0.2.141`

Both packages ship a `LICENSE.md` containing:

> © Anthropic PBC. All rights reserved. Use is subject to the Legal Agreements outlined
> here: https://code.claude.com/docs/en/legal-and-compliance

The npm registry metadata for the latest version (`0.3.143`) carries:
`"license": "SEE LICENSE IN README.md"` — proprietary.

**Dependency chain:**
```
@cline/sdk → @cline/core → @cline/llms → ai-sdk-provider-claude-code@3.4.4 → @anthropic-ai/claude-agent-sdk
```

`ai-sdk-provider-claude-code` is an AI SDK provider that integrates Claude Code into Vercel
AI SDK. Its dependency on `@anthropic-ai/claude-agent-sdk` pulls in the proprietary Anthropic
package at install time.

**Impact:** This package cannot be redistributed as part of xvision under Apache-2.0. Including
`@cline/sdk` as-is in a Node sidecar would pull this proprietary package into the xvision
distribution unless it is explicitly excluded or the provider is replaced.

**Mitigation paths (not evaluated in this audit):**
- Check whether `ai-sdk-provider-claude-code` (and therefore `@anthropic-ai/claude-agent-sdk`)
  is an optional dependency or only exercised when the "Claude Code" LLM provider is selected.
  If it is not in the critical path for xvision's intended use, it may be possible to avoid
  bundling it by using a custom install (e.g. `--ignore-scripts`, overriding peer dependencies,
  or subclassing the SDK without that provider loaded).
- Contact the Cline team to clarify whether `ai-sdk-provider-claude-code` can be made optional
  or removed from the default dependency tree.
- Use an older version of `@cline/sdk` predating the `ai-sdk-provider-claude-code` dependency
  (if one exists with the needed API surface).

### 2. @cline/llms, @cline/shared, @cline/core — missing LICENSE in npm tarball

These three packages have `null` in their `package.json` `license` field and no `LICENSE` file
in the published npm tarball. License derivable only from the upstream GitHub repo (`sdk/LICENSE`,
Apache-2.0). This is a packaging deficiency. For a production embed, the NOTICE/attribution
requirements of Apache-2.0 must still be satisfied by referencing the upstream repo.

### 3. json-schema@0.4.0 — AFL-2.1 OR BSD-3-Clause

The AFL-2.1 (Academic Free License 2.1) is a permissive OSI-approved license compatible with
Apache-2.0 redistribution under the BSD-3-Clause alternative. No action required.

### 4. @openai/codex — Apache-2.0, platform binary

`@openai/codex@0.130.0` and the platform-specific `@openai/codex@0.130.0-darwin-arm64` are
listed as Apache-2.0 and are transitive dependencies via `@cline/llms`. No license concern,
but worth noting that a Darwin-arm64 binary is pulled in at install time.

---

## Verdict

**BLOCK — conditional**

The transitive dependency `@anthropic-ai/claude-agent-sdk` (pulled via `@cline/llms` →
`ai-sdk-provider-claude-code`) carries a proprietary "all rights reserved" Anthropic license.
This package cannot be redistributed as part of xvision under Apache-2.0 without Anthropic's
explicit permission or a contractual arrangement covering redistribution.

Wave 1 **cannot proceed** with a naive `npm install @cline/sdk` embed until this dependency
is resolved. The Cline packages themselves and the remainder of the transitive graph are
Apache-2.0, MIT, ISC, BSD-2-Clause, BSD-3-Clause, 0BSD, or permissive dual-license — no
GPL/AGPL/LGPL/SSPL/BUSL found.

**Required before wave 1 can unblock:**
1. Confirm whether `ai-sdk-provider-claude-code` / `@anthropic-ai/claude-agent-sdk` is
   exercised in the xvision use case (if Claude Code provider support is not needed, it
   may be excludable).
2. If it is needed, obtain Anthropic's redistribution terms or replace the provider with
   one that does not carry a proprietary license.
3. Address the packaging gap on `@cline/llms`, `@cline/shared`, `@cline/core` (no LICENSE
   in tarball) — add a NOTICE referencing `https://github.com/cline/cline/blob/main/sdk/LICENSE`
   to the xvision distribution.

---

## 2026-05-17 status — deferred, wave 1 continues without `@cline/sdk` import

**Decision:** Per user direction on 2026-05-17, the licensing baseline (Wave 1 Task 2 of
`docs/superpowers/plans/2026-05-17-cline-sdk-agent-replacement-wave1.md`) is **deferred**.
Wave 1 continues with the sidecar adapter scaffold (Tasks 3–10), which does **not** import
`@cline/sdk` or any of its transitive proprietary chain. The deferred work and the open
questions below must be resolved before Wave 2 imports the SDK.

**Open follow-ups to resolve before Wave 2:**

- [ ] **F1 — provider exclusion path.** Investigate whether `@cline/sdk` can be consumed
  without `ai-sdk-provider-claude-code`. Specifically:
  - Does `@cline/llms`'s `registerProvider` API permit excluding the Claude Code provider
    at runtime via tree-shaking or conditional imports?
  - If not, is there an upstream issue/PR to make the Claude Code chain an optional dep?
- [ ] **F2 — Anthropic redistribution.** If F1 is not viable, obtain written redistribution
  terms from Anthropic covering `@anthropic-ai/claude-agent-sdk` in our deploy image,
  or replace the affected provider.
- [ ] **F3 — Cline NOTICE gap.** `@cline/llms`, `@cline/shared`, `@cline/core` ship no
  `LICENSE` file in their npm tarballs. When we eventually add `NOTICE`, reference
  `https://github.com/cline/cline/blob/main/sdk/LICENSE` explicitly.
- [ ] **F4 — Wave 1 Task 2 deferred.** The repo licensing baseline (`LICENSE`, `NOTICE`,
  `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `THIRD_PARTY_LICENSES.md`,
  `deny.toml`, `.github/workflows/license.yml`) is deferred. Land it as a precondition
  to Wave 2 once F1/F2 are resolved.

These follow-ups block Wave 2's `@cline/sdk` import, not Wave 1's adapter scaffold.
