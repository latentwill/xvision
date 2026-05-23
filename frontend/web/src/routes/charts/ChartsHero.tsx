// /charts/hero — Chart 05 Gradient Warm Hero Dashboard.
// B0: placeholder shell. B4 replaces with the real GradientHeroDashboard
// surface: AuraBackground + HeroGradientEquity + PerformanceRadar +
// MarketContextCard. Per spec §11.3, mounts only at /charts/hero in this
// wave; the /-replacement decision is the B5 review milestone.
//
// See docs/superpowers/plans/2026-05-23-charts-section-b4-gradient-hero.md.

import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";

export function ChartsHero() {
  return (
    <EmptyState
      title="B4: Hero — coming soon"
      message="The Gradient Warm Hero dashboard (Chart 05) lands in milestone B4: aura background washes, gradient-fill equity hero, performance radar, and a market-context card."
    />
  );
}
