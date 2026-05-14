# qa8-agent-ux-cli-templates

Status: complete

Claimed: 2026-05-13T15:27:43Z

Worktree: `.worktrees/qa8-agent-ux-cli-templates`

Branch: `qa8-agent-ux-cli-templates`

Base: `qa8-cli-doctor-help-examples` commit `0e9a4e4`

Current focus:

- Added copy-pastable `xvn strategy create --template ... --name ... --json`
  rendering to the Strategy create form.
- Added `xvn strategy templates --json`, returning a versioned registry object
  with template names, display names, and plain summaries.
- Updated README/MANUAL CLI examples for the template JSON surface.

Verification:

- `corepack pnpm --dir frontend/web test -- strategies-new`
- `corepack pnpm --dir frontend/web typecheck`
- `corepack pnpm --dir frontend/web test`
- `git diff --check`
- `rg -n -e "Templates \\{|templates\\(json|registry_version|strategy templates \\[--json\\]|templates_json" crates/xvision-cli/src/commands/strategy.rs crates/xvision-cli/tests/strategy_cli.rs crates/xvision-engine/src/templates/registry.rs README.md MANUAL.md frontend/web/src/routes/strategies-new.tsx frontend/web/src/routes/strategies-new.test.tsx`

Not run:

- Cargo commands are forbidden on this deploy host by `CLAUDE.md`; Rust CLI
  tests were added but must run in CI/non-deploy.
