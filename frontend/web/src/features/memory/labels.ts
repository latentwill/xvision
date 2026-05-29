export function formatVerdict(v: string | null | undefined): string {
  if (v === "passed") return "Kept";
  if (v === "failed") return "Dropped";
  return v ?? "";
}

// Handles both pattern states ("active","staged","forgotten") and autoresearch
// run states ("promoted","demoted") — the value sets are non-overlapping.
export function formatPromotionState(s: string | null | undefined): string {
  switch (s) {
    case "staged":    return "Staged";
    case "active":    return "Active";
    case "forgotten": return "Forgotten";
    case "promoted":  return "Active";
    case "demoted":   return "Retired";
    default:          return s ?? "";
  }
}
