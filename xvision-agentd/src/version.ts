// Bumped manually. Wave-1 baseline.
//
// PROTOCOL_VERSION semver:
//   MAJOR — backwards-incompatible RPC shape change
//   MINOR — additive method or field
//   PATCH — bug-fix only, no protocol surface change
//
// SIDECAR_VERSION is the xvision-agentd build version. Bump when the
// @cline/sdk-binding package version changes (Wave 2+; unbound in Wave 1)
// or when a non-protocol behavior change ships in this daemon.
export const PROTOCOL_VERSION = "0.1.0"
export const SIDECAR_VERSION = "0.1.0"
