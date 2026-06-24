# Providers & Brokers

**Providers** are LLM backend configs. Each provider entry tells xvn how to
reach an API endpoint, which env var holds the key, and which model ids are
enabled for agent slots and the chat rail. Brokers are a separate concept
covered at the bottom of this page.

## What providers are

An agent slot binds a `provider` id and a `model` id. At run time the engine
resolves the named provider entry, loads the API key, and dispatches to its
endpoint. Multiple providers can coexist — an `anthropic` entry and an
`openai-compat` entry pointing at OpenRouter, for example. Each agent slot
picks one via its `provider` field.

xvn starts with zero providers configured. The dashboard and the chat rail
will indicate that no agent can run until at least one provider is added and
has a reachable key.

## Provider config

Providers are stored in `$XVN_HOME/config/default.toml` (default:
`~/.xvn/config/default.toml`). Each entry is a `[[providers]]` table:

```toml
[[providers]]
name        = "anthropic"
kind        = "anthropic"           # anthropic | openai-compat | local-candle
base_url    = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"   # env var that holds the secret; empty for no-auth endpoints

# Optional — model ids the operator has explicitly enabled for the chat
# rail and wizard dropdown. Empty means "nothing selected yet".
enabled_models = ["claude-haiku-4-5", "claude-sonnet-4-6"]
```

A minimal OpenAI-compatible entry (works for OpenRouter, Ollama, vLLM, etc.):

```toml
[[providers]]
name        = "openrouter"
kind        = "openai-compat"
base_url    = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY"
```

For local endpoints that need no auth, set `api_key_env = ""`.

`local-candle` is an in-process provider; `base_url` is ignored for that kind.

## Auth ladder

When xvn needs the API key for a provider, it resolves in this order:

1. The env var named in `api_key_env` (set in the shell or passed by the
   process supervisor).
2. Inline key stored in `$XVN_HOME/secrets/providers.toml` (mode 0600) — this
   is written when you paste a key into Settings → Providers in the dashboard.
3. If neither is present, the provider is marked as unconfigured and any agent
   referencing it will fail at run time.

Never commit API keys to `config/default.toml`. Use the env var or the
dashboard's key-paste flow.

## CLI verbs

| Verb | Effect |
|---|---|
| `xvn provider list` | Print all registered providers with kind, base URL, env var, and key-present status. |
| `xvn provider show --name <name>` | Show full JSON for one provider including `enabled_models`. |
| `xvn provider check --name <name>` | TCP-connect smoke test; add `--probe` to also send a live `/v1/models` request. |
| `xvn provider add --name <n> --kind <k> --base-url <u> [--api-key-env <e>] [--api-key <k>]` | Register a new provider entry in `config/default.toml`. |
| `xvn provider remove --name <name>` | Remove a provider. Refused if any agent slot references it. |
| `xvn provider refresh-models [--name <name>]` | Hit `/v1/models` and write the catalog to disk. Omit `--name` to refresh all providers. |
| `xvn provider models --name <name>` | Print the cached model catalog (id, context window, max output tokens, reasoning flag). Does not hit the network — run `refresh-models` first. |

## Where things live

| Path | Contents |
|---|---|
| `$XVN_HOME/config/default.toml` | `[[providers]]` table and `[default_llm]` workspace default. |
| `$XVN_HOME/secrets/providers.toml` | Inline keys written by the dashboard (mode 0600). Treat like an SSH private key. |
| `$XVN_HOME/providers/<name>/catalog.json` | Cached model catalog written by `refresh-models`. |

`XVN_HOME` defaults to `~/.xvn`. Override with the `XVN_HOME` env var or
`XVN_CONFIG_PATH` to point at a different `default.toml` directly.

## Brokers (separate concept)

Brokers handle order execution, not LLM calls. They are configured under a
separate **Settings → Brokers** tab in the dashboard and have their own auth
surface. This split is intentional — providers are stateless API endpoints;
brokers carry account-level credentials and settlement risk.

Currently supported:

| Broker | Kind | Notes |
|---|---|---|
| Alpaca | `alpaca` | Paper trading (v1 default). Credentials stored in `$XVN_HOME/secrets/brokers.toml`. |
| Orderly Network | `orderly` | Live only — disabled in v1 paper mode. |

Alpaca credentials are resolved in this order: stored creds in
`$XVN_HOME/secrets/brokers.toml` (written via Settings → Brokers) win over
env vars (`APCA_API_KEY_ID` / `APCA_API_SECRET_KEY`). The env-var path stays
active so CI scripts that already export those vars keep working.

Broker auth failures surface as `broker_auth` and `broker_rejected` failure
classes on eval runs. See [Eval Runs](/docs?slug=eval-runs) for the full
failure class reference.

More brokers will be added in future releases.
