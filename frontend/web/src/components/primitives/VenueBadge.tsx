// Venue label badge — coarse classification shown on eval-run rows and the
// /safety audit log. Three values: paper (default), testnet, live.

import { Pill } from "./Pill";
import type { VenueLabel } from "@/api/safety";

const TONE_FOR: Record<VenueLabel, "default" | "warn" | "danger"> = {
  paper: "default",
  testnet: "warn",
  live: "danger",
};

const LABEL_FOR: Record<VenueLabel, string> = {
  paper: "paper",
  testnet: "testnet",
  live: "live",
};

export function VenueBadge({
  label,
  className = "",
}: {
  label: VenueLabel;
  className?: string;
}) {
  return (
    <Pill
      tone={TONE_FOR[label]}
      data-testid={`venue-badge-${label}`}
      className={className}
    >
      {LABEL_FOR[label]}
    </Pill>
  );
}
