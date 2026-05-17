import { createRequire } from "node:module"
import { readFileSync } from "node:fs"
import { join } from "node:path"

// JSON-RPC protocol version. Bumped manually. Wave 2 baseline.
export const PROTOCOL_VERSION = "0.1.0"

// Sidecar build version. Bumped manually.
export const SIDECAR_VERSION = "0.2.0"

// Resolved @cline/sdk version, read once at module load. Uses createRequire
// so node's module resolution finds the installed package no matter where
// dist/ lives relative to node_modules/ (works in dev and in the deploy image).
// @cline/sdk does not export ./package.json in its exports map, so we use
// require.resolve.paths to locate the node_modules search paths and then
// read package.json directly via fs.
function resolveClineSdkVersion(): string {
  try {
    const require = createRequire(import.meta.url)
    const searchPaths: readonly string[] = require.resolve.paths("@cline/sdk") ?? []
    for (const searchPath of searchPaths) {
      try {
        const pkgPath = join(searchPath, "@cline/sdk", "package.json")
        const pkg = JSON.parse(readFileSync(pkgPath, "utf8")) as { version?: unknown; name?: unknown }
        if (pkg.name === "@cline/sdk" && typeof pkg.version === "string" && /^\d+\.\d+\.\d+/.test(pkg.version)) {
          return pkg.version
        }
      } catch {
        // try next search path
      }
    }
  } catch {
    // fall through
  }
  return "unknown"
}

export const CLINE_SDK_VERSION = resolveClineSdkVersion()
