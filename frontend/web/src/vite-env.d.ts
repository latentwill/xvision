/// <reference types="vite/client" />

// App-specific build-time env vars. Read them ONLY as the literal
// `import.meta.env.VITE_…` expression — Vite's define replacement rewrites
// that exact form; reading through an alias (`const meta = import.meta`)
// survives to the production bundle as a runtime lookup, where browsers have
// no `import.meta.env` (this silently disabled the marketplace subgraph
// client in every deploy until 2026-06-12).
interface ImportMetaEnv {
  /** Marketplace subgraph endpoint baked at build time; blank/unset → fixture client. */
  readonly VITE_MARKETPLACE_SUBGRAPH_URL?: string;
  /** Force the agent-runs API shim to serve mock fixtures ("1"/"true"/"0"/"false"). */
  readonly VITE_USE_MOCK_AGENT_RUNS?: string;
  /** Scoped Lit client key for sealed-tier decrypt; unset → sealed install unavailable. */
  readonly VITE_LIT_CLIENT_KEY?: string;
}
