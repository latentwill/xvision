import { Pill } from "@/components/primitives/Pill";
import type { ReviewVerdict } from "@/api/eval-review";

const VERDICT_TONE: Record<ReviewVerdict, "gold" | "info" | "danger" | "warn"> = {
  promising: "gold",
  weak: "warn",
  failed: "danger",
  inconclusive: "info",
};

const VERDICT_LABEL: Record<ReviewVerdict, string> = {
  promising: "Promising",
  weak: "Weak",
  failed: "Failed",
  inconclusive: "Inconclusive",
};

export function VerdictBadge({ verdict }: { verdict: ReviewVerdict }) {
  return <Pill tone={VERDICT_TONE[verdict]}>{VERDICT_LABEL[verdict]}</Pill>;
}
