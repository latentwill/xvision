// Core domain types. Keep this file host-repo agnostic.

export type TaskStatus =
  | "BACKLOG"
  | "READY"
  | "CLAIMED"
  | "CODING"
  | "PR_OPEN"
  | "REVIEWING"
  | "CHANGES_REQUESTED"
  | "FIXING"
  | "APPROVED"
  | "MERGE_READY"
  | "MERGED"
  | "DEPLOYED"
  | "ARCHIVED";

export type TaskLane = "foundation" | "leaf" | "integration";

export interface BoardTask {
  status: TaskStatus;
  lane: TaskLane;
  track: string;
  owner_agent?: string | null;
  branch?: string | null;
  worktree?: string | null;
  pr?: number | null;
  review_status?: "none" | "requested" | "blocking" | "approved";
  deploy_status?:
    | "none"
    | "queued"
    | "building"
    | "deployed"
    | "failed"
    | "rolled_back";
  intake_doc?: string | null;
  created_at?: string;
  updated_at?: string;
}

// Daemon configuration. Loaded from agent-conductor.config.{ts,json}.
// Validated at startup; the daemon refuses to start on a missing/invalid
// field and points at the failing key.
export interface AgentConductorConfig {
  version?: string; // default "v1"
  name: string; // instance.name; required, non-empty
  repo: {
    owner: string;
    name: string;
  };
  project: {
    owner: string;
    number: number;
  };
  paths: {
    worktreeRoot: string; // relative to repo root; e.g. ".worktrees"
    queueDir: string; // relative to repo root; per-host convention
    cacheDir?: string; // default `${os.homedir}/.cache/agent-conductor`
  };
  branch: {
    prefix: string; // any prefix the host chooses; daemon never hardcodes
  };
  pollIntervalS?: number; // default 30; AGENT_CONDUCTOR_POLL_S overrides
  contractsDir: string; // required; host-repo decides location
  schemaPath: string; // required; path to board task JSON Schema
}

// Status envelope v1 — emitted by `status --json`, `watch --json`,
// and atomically written to `<cacheDir>/state.json` each poll.
export interface StatusEnvelope {
  envelope: {
    schema: "agent-conductor.status/v1";
    ts: string; // ISO 8601 UTC
  };
  instance: {
    name: string;
    repo: string; // "<owner>/<name>"
    project: string; // "<owner>:<number>"
    host: string;
    daemon_version: string;
    config_path: string;
    config_hash: string; // sha256 hex
    config_version: string;
  };
  daemon: {
    pid: number | null;
    started_at: string | null;
    state: "stopped" | "starting" | "running" | "paused" | "stopping";
    shadow: boolean;
    enabled: boolean;
    poll_interval_s: number;
    last_poll_at: string | null;
    next_poll_at: string | null;
  };
  tasks: BoardTask[];
  stuck: Array<{
    track: string;
    status: TaskStatus;
    reason: string;
    since: string; // ISO 8601 UTC
  }>;
  digest_tail: string[]; // last N lines of today's digest
}
