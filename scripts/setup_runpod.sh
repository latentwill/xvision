#!/usr/bin/env bash
# scripts/setup_runpod.sh
#
# One-time setup for a CUDA Linux GPU server (RunPod / Vast.ai). Scoped to v1
# testing: control-vector extraction + xianvec inference + Alpaca paper trading.
# Out of scope: identity (ERC-8004), Orderly, Mantle, web3, 1Password.
#
# Prerequisites — export BEFORE running:
#   HF_TOKEN              required. HuggingFace token (or HUGGING_FACE_HUB_TOKEN).
#   GH_TOKEN              optional. Plumbed into .env.local if set.
#
# Optional non-interactive overrides:
#   MODEL=fp16|q4|q5|q6|q8                Skip the model menu.
#   INTERN=anthropic|openai|openrouter|together|groq|deepseek|local|acpx|custom|skip
#   ACPX_AGENT=codex|claude|openclaw|pi   (used when INTERN=acpx)
#                                         Skip the intern backend menu.
#   ALPACA=skip                           Skip Alpaca paper credential prompt.
#   ASSUME_YES=1                          Take all defaults; never prompt.
#   SKIP_APT=1 SKIP_MODEL=1 SKIP_BUILD=1  Skip individual stages.
#   ONLY=preflight|apt|rust|python|hf|model|intern|alpaca|build|verify
#
# Idempotent — re-running is safe.

set -euo pipefail

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------
COLOR_RESET=$'\033[0m'
COLOR_BLUE=$'\033[1;34m'
COLOR_GREEN=$'\033[1;32m'
COLOR_YELLOW=$'\033[1;33m'
COLOR_RED=$'\033[1;31m'

log()   { printf '%s[%s]%s %s\n' "$COLOR_BLUE"  "$(date +%H:%M:%S)" "$COLOR_RESET" "$*"; }
ok()    { printf '%s  ok%s    %s\n'             "$COLOR_GREEN"                     "$COLOR_RESET" "$*"; }
warn()  { printf '%s  warn%s  %s\n'             "$COLOR_YELLOW"                    "$COLOR_RESET" "$*"; }
fail()  { printf '%s  FAIL%s  %s\n'             "$COLOR_RED"                       "$COLOR_RESET" "$*" >&2; exit 1; }
stage() { printf '\n%s== [%s] %s ==%s\n'        "$COLOR_BLUE"    "$1" "$2"         "$COLOR_RESET"; }
stage_active() { [[ -z "${ONLY:-}" || "$ONLY" == "$1" ]]; }

if [[ "$(id -u)" -eq 0 ]]; then SUDO=""; else SUDO="sudo"; fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

VENV_DIR="$REPO_ROOT/.venv"
MODELS_DIR="$REPO_ROOT/models"
ENV_FILE="$REPO_ROOT/.env.local"

HF_TOKEN="${HF_TOKEN:-${HUGGING_FACE_HUB_TOKEN:-}}"
export HF_TOKEN
export HUGGING_FACE_HUB_TOKEN="${HF_TOKEN}"

ASSUME_YES="${ASSUME_YES:-0}"

# Persisted env choices live in .env.local (gitignored — see end of script).
# Helper: set or replace KEY=VALUE in $ENV_FILE.
env_set() {
  local key="$1" val="$2"
  touch "$ENV_FILE"
  if grep -q "^export $key=" "$ENV_FILE" 2>/dev/null; then
    # shellcheck disable=SC2016
    sed -i.bak "s|^export $key=.*|export $key=$(printf '%s' "$val" | sed 's/[\\&|]/\\&/g')|" "$ENV_FILE"
    rm -f "$ENV_FILE.bak"
  else
    printf 'export %s=%s\n' "$key" "$val" >> "$ENV_FILE"
  fi
}

prompt() {
  # prompt "message" "default" -> echoes user input or default
  local msg="$1" def="${2:-}" reply
  if [[ "$ASSUME_YES" == "1" || ! -t 0 ]]; then echo "$def"; return; fi
  read -r -p "$msg " reply
  echo "${reply:-$def}"
}

# ---------------------------------------------------------------------------
# 1. preflight
# ---------------------------------------------------------------------------
preflight() {
  stage 1/9 "preflight"
  log "repo: $REPO_ROOT"
  log "user: $(id -un)  kernel: $(uname -sr)"

  command -v nvidia-smi >/dev/null 2>&1 || fail "nvidia-smi missing — this script targets CUDA Linux."
  log "GPU + driver:"
  nvidia-smi --query-gpu=name,memory.total,driver_version --format=csv,noheader | sed 's/^/    /'
  local drv_cuda; drv_cuda=$(nvidia-smi | sed -n 's/.*CUDA Version: \([0-9.]*\).*/\1/p' | head -1 || true)
  log "driver-reported CUDA: ${drv_cuda:-unknown}"

  local vram; vram=$(nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits | head -1)
  if [[ -n "$vram" && "$vram" -lt 24000 ]]; then
    warn "GPU VRAM <24 GB — even Q4 GGUF inference will be tight. Continuing anyway."
  fi

  local avail; avail=$(df -BG --output=avail "$REPO_ROOT" | tail -1 | tr -dc '0-9')
  log "free disk at repo root: ${avail:-?} GB"
  if [[ -n "$avail" && "$avail" -lt 50 ]]; then
    warn "Free disk <50 GB. Even Q4_K_M (~17 GB) + venv + cargo target may not fit. Resize the volume or pick a smaller model."
  fi

  [[ -n "$HF_TOKEN" ]] || fail "HF_TOKEN (or HUGGING_FACE_HUB_TOKEN) not set."
  ok "HF_TOKEN present (${#HF_TOKEN} chars)"
  if [[ -n "${GH_TOKEN:-}" ]]; then ok "GH_TOKEN present — will be persisted to .env.local"; fi
}

# ---------------------------------------------------------------------------
# 2. apt
# ---------------------------------------------------------------------------
install_apt() {
  stage 2/9 "system packages"
  if [[ "${SKIP_APT:-0}" == "1" ]]; then warn "SKIP_APT=1, skipping"; return; fi
  if ! command -v apt-get >/dev/null 2>&1; then
    warn "apt-get missing — install build-essential / cmake / pkg-config / libssl-dev manually."
    return
  fi
  export DEBIAN_FRONTEND=noninteractive
  $SUDO apt-get update -y
  $SUDO apt-get install -y --no-install-recommends \
    build-essential cmake pkg-config git curl ca-certificates \
    libssl-dev libsqlite3-dev \
    python3 python3-venv python3-pip python3-dev \
    rsync jq tmux unzip
  ok "apt packages installed"
}

# ---------------------------------------------------------------------------
# 3. rust
# ---------------------------------------------------------------------------
install_rust() {
  stage 3/9 "rust toolchain"
  if ! command -v rustup >/dev/null 2>&1; then
    log "installing rustup (rust-toolchain.toml will pin to 1.95.0)"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain none --profile minimal
  fi
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
  rustup show >/dev/null
  ok "rustc: $(rustc --version)"
}

# ---------------------------------------------------------------------------
# 4. python
# ---------------------------------------------------------------------------
install_python() {
  stage 4/9 "python venv + deps"
  [[ -d "$VENV_DIR" ]] || python3 -m venv "$VENV_DIR"
  # shellcheck disable=SC1091
  source "$VENV_DIR/bin/activate"
  python -m pip install --upgrade pip wheel
  python -m pip install -r tools/extract_vectors/requirements.txt
  python -m pip install "huggingface_hub[cli]>=0.24"

  log "verifying torch + CUDA"
  python - <<'PY'
import torch, sys
print(f"  torch:          {torch.__version__}")
print(f"  cuda available: {torch.cuda.is_available()}")
print(f"  cuda built:     {torch.version.cuda}")
print(f"  cudnn:          {torch.backends.cudnn.version()}")
if torch.cuda.is_available():
    print(f"  device:         {torch.cuda.get_device_name(0)}")
    print(f"  capability:     {torch.cuda.get_device_capability(0)}")
else:
    print("  ERROR: torch.cuda.is_available() is False. The wheel may be CPU-only "
          "or its bundled CUDA does not match the host driver.", file=sys.stderr)
    sys.exit(1)
PY
  ok "torch+CUDA verified"

  python - <<'PY'
import transformers, repeng
print(f"  transformers:   {transformers.__version__}")
print(f"  repeng:         {getattr(repeng, '__version__', 'unknown')}")
PY
  ok "extraction deps verified"
}

# ---------------------------------------------------------------------------
# 5. hf login
# ---------------------------------------------------------------------------
hf_login() {
  stage 5/9 "huggingface auth"
  # shellcheck disable=SC1091
  source "$VENV_DIR/bin/activate"
  huggingface-cli login --token "$HF_TOKEN" --add-to-git-credential
  log "whoami:"; huggingface-cli whoami | sed 's/^/    /'
  ok "huggingface authenticated"
}

# ---------------------------------------------------------------------------
# 6. model — pick ONE artifact, download just that
# ---------------------------------------------------------------------------
choose_model() {
  # Prints a key (fp16|q4|q5|q6|q8) on stdout.
  if [[ -n "${MODEL:-}" ]]; then echo "$MODEL"; return; fi
  if [[ "$ASSUME_YES" == "1" || ! -t 0 ]]; then echo "q8"; return; fi
  cat >&2 <<EOF

Pick ONE Qwen3-32B artifact:
  1) fp16 safetensors  (~64 GB)  — for control-vector extraction (needs ≥80 GB VRAM,
                                    or bf16 + device_map='auto' offload on smaller GPUs)
  2) GGUF Q8_0         (~32 GB)  — xvn inference, headline-quality (M4)
  3) GGUF Q6_K         (~26 GB)  — xvn inference, near-lossless
  4) GGUF Q5_K_M       (~22 GB)  — xvn inference, balanced
  5) GGUF Q4_K_M       (~17 GB)  — xvn inference, dev-loop default

EOF
  local sel; sel=$(prompt "Selection [1-5, default 2]:" "2")
  case "$sel" in
    1) echo fp16 ;;
    2|"") echo q8 ;;
    3) echo q6 ;;
    4) echo q5 ;;
    5) echo q4 ;;
    *) fail "unknown model selection: $sel" ;;
  esac
}

download_model() {
  stage 6/9 "model download"
  if [[ "${SKIP_MODEL:-0}" == "1" ]]; then warn "SKIP_MODEL=1, skipping"; return; fi
  # shellcheck disable=SC1091
  source "$VENV_DIR/bin/activate"
  mkdir -p "$MODELS_DIR"
  local choice; choice=$(choose_model)
  log "selected: $choice"

  case "$choice" in
    fp16)
      log "→ Qwen/Qwen3-32B safetensors → models/qwen3-32b/  (~64 GB)"
      huggingface-cli download Qwen/Qwen3-32B \
        --local-dir "$MODELS_DIR/qwen3-32b" \
        --exclude "*.gguf" "*.msgpack" "*.h5" "*.ot" "*flax*" "*.onnx"
      env_set XVN_MODEL_KIND "fp16"
      env_set XVN_MODEL_DIR  "$MODELS_DIR/qwen3-32b"
      ;;
    q4|q5|q6|q8)
      local file dir
      case "$choice" in
        q4) file="Qwen_Qwen3-32B-Q4_K_M.gguf"; dir="qwen3-32b-q4-gguf" ;;
        q5) file="Qwen_Qwen3-32B-Q5_K_M.gguf"; dir="qwen3-32b-q5-gguf" ;;
        q6) file="Qwen_Qwen3-32B-Q6_K.gguf";   dir="qwen3-32b-q6-gguf" ;;
        q8) file="Qwen_Qwen3-32B-Q8_0.gguf";   dir="qwen3-32b-q8-gguf" ;;
      esac
      log "→ Qwen/Qwen3-32B-GGUF $file → models/$dir/"
      huggingface-cli download Qwen/Qwen3-32B-GGUF "$file" \
        --local-dir "$MODELS_DIR/$dir"
      log "→ tokenizer.json (from Qwen/Qwen3-32B base repo)"
      huggingface-cli download Qwen/Qwen3-32B tokenizer.json \
        --local-dir "$MODELS_DIR/$dir"
      env_set XVN_MODEL_KIND "gguf"
      env_set XVN_MODEL_PATH "$MODELS_DIR/$dir/$file"
      env_set XVN_TOKENIZER  "$MODELS_DIR/$dir/tokenizer.json"
      ;;
    *) fail "unknown choice: $choice" ;;
  esac

  ok "model on disk:"
  du -sh "$MODELS_DIR"/* 2>/dev/null | sed 's/^/    /' || true
}

# ---------------------------------------------------------------------------
# 7. intern backend — picks the LLM provider for Stage 1 Intern
# ---------------------------------------------------------------------------
choose_intern() {
  if [[ -n "${INTERN:-}" ]]; then echo "$INTERN"; return; fi
  if [[ "$ASSUME_YES" == "1" || ! -t 0 ]]; then echo "anthropic"; return; fi
  cat >&2 <<EOF

Pick the Stage 1 Intern backend:
  1) anthropic   — Claude (claude-haiku-4-5 by default)             [ANTHROPIC_API_KEY]
  2) openai      — OpenAI (gpt-style)                               [OPENAI_API_KEY]
  3) openrouter  — multi-model gateway (recommended for evaluation) [OPENROUTER_API_KEY]
  4) together    — Together AI                                      [TOGETHER_API_KEY]
  5) groq        — Groq fast-inference                              [GROQ_API_KEY]
  6) deepseek    — DeepSeek API                                     [DEEPSEEK_API_KEY]
  7) local       — local OpenAI-compat server (vLLM / Ollama / llama.cpp)
  8) custom      — user-supplied base URL + key env var
  9) acpx        — agent harness via Agent Client Protocol (F21).
                   Subprocesses 'acpx <agent> exec' — multi-step tool use.
                   Non-deterministic: best for forward paper, not backtest.
 10) skip        — configure later

EOF
  local sel; sel=$(prompt "Selection [1-10, default 1]:" "1")
  case "$sel" in
    1|"") echo anthropic ;;
    2) echo openai ;;
    3) echo openrouter ;;
    4) echo together ;;
    5) echo groq ;;
    6) echo deepseek ;;
    7) echo local ;;
    8) echo custom ;;
    9) echo acpx ;;
    10) echo skip ;;
    *) fail "unknown intern selection: $sel" ;;
  esac
}

install_acpx() {
  # Node + global acpx. Idempotent — `npm i -g` over an existing install
  # just upgrades. Underlying agent CLI (codex/claude/openclaw/pi) is the
  # operator's responsibility — too many auth flows to script safely.
  if ! command -v node >/dev/null 2>&1; then
    log "installing Node.js (apt nodejs npm)"
    if command -v apt-get >/dev/null 2>&1; then
      $SUDO apt-get install -y --no-install-recommends nodejs npm
    else
      warn "no apt-get — install Node.js manually, then re-run with INTERN=acpx ONLY=intern"
      return 1
    fi
  fi
  log "node: $(node --version)  npm: $(npm --version)"
  if ! command -v acpx >/dev/null 2>&1; then
    log "installing acpx globally (npm i -g acpx@latest)"
    $SUDO npm install -g acpx@latest
  fi
  ok "acpx: $(acpx --version 2>/dev/null || echo 'installed')"
}

setup_intern() {
  stage 7/9 "intern backend"
  local choice; choice=$(choose_intern)
  log "selected: $choice"
  local key

  case "$choice" in
    anthropic)
      env_set XVN_INTERN_PROVIDER  "anthropic"
      env_set XVN_INTERN_BASE_URL  "https://api.anthropic.com"
      env_set XVN_INTERN_MODEL     "claude-haiku-4-5"
      env_set XVN_INTERN_KEY_ENV   "ANTHROPIC_API_KEY"
      key=$(prompt "ANTHROPIC_API_KEY (paste, blank to skip):" "")
      [[ -n "$key" ]] && env_set ANTHROPIC_API_KEY "$key"
      ;;
    openai)
      env_set XVN_INTERN_PROVIDER  "openai-compat"
      env_set XVN_INTERN_BASE_URL  "https://api.openai.com/v1"
      env_set XVN_INTERN_MODEL     "$(prompt 'OpenAI model [gpt-5]:' 'gpt-5')"
      env_set XVN_INTERN_KEY_ENV   "OPENAI_API_KEY"
      key=$(prompt "OPENAI_API_KEY (paste, blank to skip):" "")
      [[ -n "$key" ]] && env_set OPENAI_API_KEY "$key"
      ;;
    openrouter)
      env_set XVN_INTERN_PROVIDER  "openai-compat"
      env_set XVN_INTERN_BASE_URL  "https://openrouter.ai/api/v1"
      env_set XVN_INTERN_MODEL     "$(prompt 'OpenRouter model [deepseek/deepseek-r1]:' 'deepseek/deepseek-r1')"
      env_set XVN_INTERN_KEY_ENV   "OPENROUTER_API_KEY"
      key=$(prompt "OPENROUTER_API_KEY (paste, blank to skip):" "")
      [[ -n "$key" ]] && env_set OPENROUTER_API_KEY "$key"
      ;;
    together)
      env_set XVN_INTERN_PROVIDER  "openai-compat"
      env_set XVN_INTERN_BASE_URL  "https://api.together.xyz/v1"
      env_set XVN_INTERN_MODEL     "$(prompt 'Together model [Qwen/Qwen3-32B]:' 'Qwen/Qwen3-32B')"
      env_set XVN_INTERN_KEY_ENV   "TOGETHER_API_KEY"
      key=$(prompt "TOGETHER_API_KEY (paste, blank to skip):" "")
      [[ -n "$key" ]] && env_set TOGETHER_API_KEY "$key"
      ;;
    groq)
      env_set XVN_INTERN_PROVIDER  "openai-compat"
      env_set XVN_INTERN_BASE_URL  "https://api.groq.com/openai/v1"
      env_set XVN_INTERN_MODEL     "$(prompt 'Groq model [qwen/qwen3-32b]:' 'qwen/qwen3-32b')"
      env_set XVN_INTERN_KEY_ENV   "GROQ_API_KEY"
      key=$(prompt "GROQ_API_KEY (paste, blank to skip):" "")
      [[ -n "$key" ]] && env_set GROQ_API_KEY "$key"
      ;;
    deepseek)
      env_set XVN_INTERN_PROVIDER  "openai-compat"
      env_set XVN_INTERN_BASE_URL  "https://api.deepseek.com/v1"
      env_set XVN_INTERN_MODEL     "$(prompt 'DeepSeek model [deepseek-reasoner]:' 'deepseek-reasoner')"
      env_set XVN_INTERN_KEY_ENV   "DEEPSEEK_API_KEY"
      key=$(prompt "DEEPSEEK_API_KEY (paste, blank to skip):" "")
      [[ -n "$key" ]] && env_set DEEPSEEK_API_KEY "$key"
      ;;
    local)
      local url; url=$(prompt "Local OpenAI-compat URL [http://localhost:8000/v1]:" "http://localhost:8000/v1")
      env_set XVN_INTERN_PROVIDER  "openai-compat"
      env_set XVN_INTERN_BASE_URL  "$url"
      env_set XVN_INTERN_MODEL     "$(prompt 'Model name as the local server expects [Qwen/Qwen3-32B]:' 'Qwen/Qwen3-32B')"
      env_set XVN_INTERN_KEY_ENV   ""
      ;;
    custom)
      env_set XVN_INTERN_PROVIDER  "openai-compat"
      env_set XVN_INTERN_BASE_URL  "$(prompt 'Custom base URL:' 'https://api.example.com/v1')"
      env_set XVN_INTERN_MODEL     "$(prompt 'Model id:' '')"
      local key_env; key_env=$(prompt "Env var name holding the API key (blank if no auth):" "")
      env_set XVN_INTERN_KEY_ENV   "$key_env"
      if [[ -n "$key_env" ]]; then
        key=$(prompt "Value for $key_env (paste, blank to skip):" "")
        [[ -n "$key" ]] && env_set "$key_env" "$key"
      fi
      ;;
    acpx)
      install_acpx || warn "acpx install incomplete — re-run after fixing Node/npm"
      local agent; agent="${ACPX_AGENT:-}"
      local custom_cmd=""
      if [[ -z "$agent" ]]; then
        cat >&2 <<EOAGENT

ACPX delegates Stage 1 to an ACP-speaking agent. Built-in registry:

  1) claude     Claude Code (npm i -g @anthropic-ai/claude-code; ANTHROPIC_API_KEY)
  2) codex      OpenAI Codex via @zed-industries/codex-acp                [OPENAI_API_KEY]
  3) gemini     Google Gemini CLI (gemini --acp)                          [GEMINI_API_KEY]
  4) opencode   OpenCode AI — open-source agent (npx -y opencode-ai acp)
  5) cursor     Cursor agent (cursor-agent acp)
  6) copilot    GitHub Copilot CLI (copilot --acp --stdio)
  7) qwen       Alibaba Qwen Coder (qwen --acp)
  8) kimi       Moonshot Kimi (kimi acp)
  9) iflow      iFlow / Aliyun (iflow --experimental-acp)
 10) trae       ByteDance Trae (traecli acp serve)
 11) qoder      Qoder (qodercli --acp)
 12) kilocode   KiloCode (npx -y @kilocode/cli acp)
 13) kiro       Kiro (kiro-cli-chat acp)
 14) droid      Factory Droid (droid exec --output-format acp)
 15) openclaw   OpenClaw ACP bridge — predecessor to Hermes
 16) pi         Pi Coding Agent (npx pi-acp)

Custom ACP servers (escape hatch — runs as 'acpx --agent <cmd>'):
 17) hermes     NousResearch Hermes Agent — itself an ACP server. Direct
                routes to Xiaomi MiMo, Kimi, GLM, MiniMax, Nous Portal.
                Successor to OpenClaw. Run 'pip install hermes-agent' or the
                installer at https://hermes-agent.nousresearch.com/docs/.
 18) custom     Paste your own '--agent <cmd>' invocation.

Underlying agent CLIs are NOT installed by this script — auth flows vary.
Install separately per the agent's docs.

EOAGENT
        local sel; sel=$(prompt "Agent [1-18, default 1]:" "1")
        case "$sel" in
          1|"") agent=claude ;;
          2) agent=codex ;;
          3) agent=gemini ;;
          4) agent=opencode ;;
          5) agent=cursor ;;
          6) agent=copilot ;;
          7) agent=qwen ;;
          8) agent=kimi ;;
          9) agent=iflow ;;
          10) agent=trae ;;
          11) agent=qoder ;;
          12) agent=kilocode ;;
          13) agent=kiro ;;
          14) agent=droid ;;
          15) agent=openclaw ;;
          16) agent=pi ;;
          17) agent=hermes; custom_cmd="hermes acp" ;;
          18) agent=custom; custom_cmd="$(prompt 'Full --agent command (e.g. "node ./my-acp-server.mjs"):' '')" ;;
          *) fail "unknown acpx agent selection: $sel" ;;
        esac
      fi
      env_set XVN_INTERN_PROVIDER          "acpx"
      env_set XVN_INTERN_ACPX_AGENT        "$agent"
      env_set XVN_INTERN_ACPX_TIMEOUT_SECS "${XVN_INTERN_ACPX_TIMEOUT_SECS:-300}"
      if [[ -n "$custom_cmd" ]]; then
        env_set XVN_INTERN_ACPX_CUSTOM_CMD "$custom_cmd"
      fi
      # Sandbox the agent's fs/* operations to a scratch workspace by
      # default; operator can repoint if they want the agent in-tree.
      local ws="${XVN_INTERN_ACPX_WORKSPACE:-$REPO_ROOT/.acpx-workspace}"
      mkdir -p "$ws"
      env_set XVN_INTERN_ACPX_WORKSPACE    "$ws"

      # Write acpx.config.json inside the workspace, registering xvn-mcp
      # as a stdio MCP server. Every ACP-compatible agent ACPX talks to
      # (Hermes, Claude Code, Codex, OpenCode, ...) will see the xvn_*
      # tools at session start. The xvn-mcp binary is built later in the
      # `build` stage; the config just points at where it'll live.
      local cfg="$ws/acpx.config.json"
      cat > "$cfg" <<EOJSON
{
  "mcpServers": [
    {
      "type": "stdio",
      "name": "xianvec",
      "command": "$REPO_ROOT/target/release/xvn-mcp",
      "args": [],
      "env": []
    }
  ]
}
EOJSON
      ok "wrote $cfg"

      # Best-effort key prompts for the most common agents; harmless to skip.
      case "$agent" in
        codex)   key=$(prompt "OPENAI_API_KEY (paste, blank if already set):" "")
                 [[ -n "$key" ]] && env_set OPENAI_API_KEY "$key" ;;
        claude)  key=$(prompt "ANTHROPIC_API_KEY (paste, blank if using 'claude login'):" "")
                 [[ -n "$key" ]] && env_set ANTHROPIC_API_KEY "$key" ;;
        gemini)  key=$(prompt "GEMINI_API_KEY (paste, blank to skip):" "")
                 [[ -n "$key" ]] && env_set GEMINI_API_KEY "$key" ;;
        hermes)  log "Hermes is configured via 'hermes model' / 'hermes setup' — provider keys live in Hermes's own config." ;;
        *) : ;;
      esac
      ok "acpx agent: $agent${custom_cmd:+  (--agent \"$custom_cmd\")}  workspace: $ws"
      ;;
    skip)
      log "intern backend not configured — edit $ENV_FILE and config/default.toml later."
      ;;
    *) fail "unknown intern choice: $choice" ;;
  esac
  ok "intern backend: $choice"
}

# ---------------------------------------------------------------------------
# 8. alpaca paper trading credentials (optional)
# ---------------------------------------------------------------------------
setup_alpaca() {
  stage 8/9 "alpaca paper credentials"
  if [[ "${ALPACA:-}" == "skip" ]]; then warn "ALPACA=skip, skipping"; return; fi

  if [[ "$ASSUME_YES" == "1" || ! -t 0 ]]; then
    log "non-interactive — skipping (set APCA_* in $ENV_FILE manually)"
    return
  fi

  local want; want=$(prompt "Configure Alpaca paper-trading credentials now? [y/N]:" "n")
  case "$want" in
    y|Y|yes)
      local k s
      k=$(prompt "APCA_API_KEY_ID:" "")
      s=$(prompt "APCA_API_SECRET_KEY:" "")
      [[ -n "$k" ]] && env_set APCA_API_KEY_ID     "$k"
      [[ -n "$s" ]] && env_set APCA_API_SECRET_KEY "$s"
      env_set APCA_API_BASE_URL "https://paper-api.alpaca.markets"
      ok "Alpaca paper creds saved to $ENV_FILE"
      ;;
    *) log "skipped — add later by editing $ENV_FILE" ;;
  esac
}

# ---------------------------------------------------------------------------
# 9. patch + build xvn (--features cuda)
# ---------------------------------------------------------------------------
patch_build_for_cuda() {
  # GAP fix: xianvec-inference defaults to [metal] (Apple), and xianvec-cli
  # has no cuda passthrough. On Linux we need the cli to (a) disable
  # default-features on the inference dep and (b) expose `cuda`. Minimal
  # scoped patch — only crates/xianvec-cli/Cargo.toml is touched.
  local cli_toml="$REPO_ROOT/crates/xianvec-cli/Cargo.toml"
  if grep -q '^cuda = \["xianvec-inference/cuda"\]' "$cli_toml"; then
    return
  fi
  log "patching $cli_toml — cuda feature passthrough"
  python3 - <<PY
import pathlib, re
p = pathlib.Path("$cli_toml")
src = p.read_text()
src = re.sub(
    r'^xianvec-inference = \{ path = "\.\./xianvec-inference" \}',
    'xianvec-inference = { path = "../xianvec-inference", default-features = false }',
    src, count=1, flags=re.MULTILINE,
)
if "\n[features]\n" not in src:
    src += '\n[features]\ndefault = []\ncuda = ["xianvec-inference/cuda"]\n'
elif 'cuda = ["xianvec-inference/cuda"]' not in src:
    src = src.replace("[features]\n", '[features]\ncuda = ["xianvec-inference/cuda"]\n', 1)
p.write_text(src)
PY
  ok "patched"
}

build_xvn() {
  stage 9/9 "build xvn (release, --features cuda)"
  if [[ "${SKIP_BUILD:-0}" == "1" ]]; then warn "SKIP_BUILD=1, skipping"; return; fi
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
  patch_build_for_cuda
  cargo build --release -p xianvec-cli --features cuda
  ok "built target/release/xvn"

  # xvn-mcp: stdio Model Context Protocol server exposing xianvec-data
  # indicators as agent-callable tools. Pure CPU; no cuda feature needed.
  # Advertised to ACPX via acpx.config.json so any ACP-compatible agent
  # (Hermes, Claude Code, Codex, OpenCode, etc.) gets the xvn_* tools at
  # session start.
  log "building xvn-mcp"
  cargo build --release -p xianvec-mcp
  ok "built target/release/xvn-mcp"
}

# ---------------------------------------------------------------------------
# verify + summary
# ---------------------------------------------------------------------------
verify() {
  stage verify "smoke checks"
  # shellcheck disable=SC1091
  source "$VENV_DIR/bin/activate"
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"

  python tools/extract_vectors/extract_vectors.py --help >/dev/null \
    && ok "extract_vectors imports cleanly" \
    || fail "extract_vectors --help failed"

  if [[ -x "$REPO_ROOT/target/release/xvn" ]]; then
    "$REPO_ROOT/target/release/xvn" --help >/dev/null \
      && ok "xvn --help works" \
      || warn "xvn --help failed"
  fi

  # MCP smoke: send `initialize` + `tools/list`, confirm `xvn_rsi` is in
  # the registered tools. Bail-out timeout in case the server hangs.
  if [[ -x "$REPO_ROOT/target/release/xvn-mcp" ]]; then
    local got
    got=$(
      {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"setup","version":"0"}}}'
        printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
        printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
        sleep 0.3
      } | timeout 5 "$REPO_ROOT/target/release/xvn-mcp" 2>/dev/null || true
    )
    if grep -q 'xvn_rsi' <<<"$got"; then
      ok "xvn-mcp tools/list lists xvn_rsi"
    else
      warn "xvn-mcp handshake did not list expected tools — check stderr manually"
    fi
  fi
}

ensure_gitignore() {
  local gi="$REPO_ROOT/.gitignore"
  if [[ -f "$gi" ]] && ! grep -qxF ".env.local" "$gi"; then
    echo ".env.local" >> "$gi"
  fi
}

print_summary() {
  ensure_gitignore
  if [[ -n "${GH_TOKEN:-}" ]]; then env_set GH_TOKEN "$GH_TOKEN"; fi
  cat <<EOF

${COLOR_GREEN}============================================================
 setup_runpod.sh complete
============================================================${COLOR_RESET}

Activate in new shells:
    source $VENV_DIR/bin/activate
    source \$HOME/.cargo/env
    source $ENV_FILE        # XVN_* + provider keys persisted here

Persisted env (.env.local):
$(sed 's/^/    /' "$ENV_FILE" 2>/dev/null || echo "    (none)")

Vector extraction (uses fp16 safetensors):
    python tools/extract_vectors/extract_vectors.py \\
      --model "\$XVN_MODEL_DIR" \\
      --spec tools/extract_vectors/specs/conviction.yaml \\
      --layers 20,32,42,50 --device cuda --dtype fp16 \\
      --out data/vectors/conviction_v1

xvn inference (uses GGUF + tokenizer):
    target/release/xvn run-setup --model "\$XVN_MODEL_PATH" --tokenizer "\$XVN_TOKENIZER" ...

xvn-mcp (Model Context Protocol server — exposes xianvec-data indicators
as agent-callable tools):
    target/release/xvn-mcp                  # speaks JSON-RPC over stdio
    # Registered in \$XVN_INTERN_ACPX_WORKSPACE/acpx.config.json. Any ACP-
    # compatible agent driven through ACPX (Hermes, Claude, Codex, …) will
    # see xvn_rsi / xvn_macd / xvn_bollinger / etc. at session start.

Out of v1 scope, do separately if/when you need them: identity / Mantle /
Orderly / op-vault integration.
EOF
}

# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------
stage_active preflight && preflight
stage_active apt       && install_apt
stage_active rust      && install_rust
stage_active python    && install_python
stage_active hf        && hf_login
stage_active model     && download_model
stage_active intern    && setup_intern
stage_active alpaca    && setup_alpaca
stage_active build     && build_xvn
stage_active verify    && verify

[[ -z "${ONLY:-}" ]] && print_summary
exit 0
