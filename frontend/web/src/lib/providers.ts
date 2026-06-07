import type { ProviderRow } from "@/api/types.gen";

/**
 * A provider is "configured" when it is ready to serve models:
 * - Not synthetic (placeholder rows in the catalog)
 * - Either has its API key set, OR needs no key at all (e.g. Ollama / local endpoints
 *   where api_key_env is an empty string or the provider kind is local-only)
 */
export function isProviderConfigured(row: ProviderRow): boolean {
  if (row.synthetic) return false;
  const needsKey =
    row.api_key_env.trim().length > 0 &&
    !["ollama", "llama-cpp", "vllm", "local-candle"].includes(row.kind);
  return !needsKey || row.api_key_set;
}
