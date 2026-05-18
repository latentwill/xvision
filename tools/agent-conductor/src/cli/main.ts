// Executable entrypoint. `index.ts` exports `runCli` (importable from
// tests, the bin shim, future tooling) and this module is the
// script-mode launcher the tsx fallback in `bin/agent-conductor`
// spawns. Kept tiny so the shim doesn't need to inject argv.
//
// Compiled output lives at `dist/cli/main.js` alongside `dist/cli/index.js`,
// so a production install can also point its bin entry at this module
// if/when the shim is simplified.

import { runCli } from "./index.js";

runCli(process.argv).catch((err) => {
  process.stderr.write(`fatal: ${err?.stack ?? err}\n`);
  process.exit(1);
});
