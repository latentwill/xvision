// Host-repo configuration for agent-conductor. The runtime currently
// reads the JSON sibling (`agent-conductor.config.json`) until a TS
// loader lands; this file exists so the schema is self-documenting and
// future ts-config loading drops in without renaming.

import type { AgentConductorConfig } from "./tools/agent-conductor/src/types.js";

const config: AgentConductorConfig = {
  version: "v1",
  name: "xvision",
  repo: { owner: "latentwill", name: "xvision" },
  project: { owner: "latentwill", number: 7 },
  paths: {
    worktreeRoot: ".worktrees",
    queueDir: "team/queue",
    // cacheDir defaults to ~/.cache/agent-conductor
  },
  branch: { prefix: "agent/" },
  pollIntervalS: 30,
  contractsDir: "team/contracts",
  schemaPath: "team/schema/board.schema.json",
};

export default config;
