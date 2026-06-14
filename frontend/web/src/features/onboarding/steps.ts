import type { DriveStep } from "driver.js";

// Action-first first-run tour. The opening steps walk the getting-started path
// in order — connect a model → build a strategy → pick a window → run a
// backtest — then the remaining steps surface the rooms you grow into. Every
// step highlights a real sidebar link (clickable through the overlay) so the
// copy can say "do this now", not just "here's a thing". The final step routes
// the user to Settings → Providers (see useFirstRunTour.ts) so the tour ends on
// their actual first action.
export const firstRunTourSteps: DriveStep[] = [
  {
    popover: {
      title: "Welcome to XVN",
      description:
        "Three steps to your first trading agent: connect a model, build a strategy, run a backtest. This tour points you at each one — plus a few surfaces you'll grow into.",
    },
  },
  {
    element: 'a[href="/settings"]',
    popover: {
      title: "Connect a model",
      description:
        "Start here. Open Settings → Providers and add an API key — OpenAI, Anthropic, or a local model. Nothing runs until a model is connected; the brain is yours.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/strategies"]',
    popover: {
      title: "Build a strategy",
      description:
        "Use the chat rail on the right to describe the strategy you want — XVN builds it for you. Or start from a template or a loaded example. Each strategy composes one or more agents.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/scenarios"]',
    popover: {
      title: "Pick a market window",
      description:
        "Choose what to test against — asset, date range, fees, slippage. A few market scenarios ship ready to use.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/eval-runs"]',
    popover: {
      title: "Run your first backtest",
      description:
        "Backtest on real historical data — or paper-trade against the live market, risk-free. This is where you find the winners. Start here.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/live"]',
    popover: {
      title: "Deploy your winners",
      description:
        "Fund your agent and let it trade for real. Live trading is real money — full stop.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/optimizer"]',
    popover: {
      title: "Improve your strategy",
      description:
        "Optimize it automatically — XVN tests different prompts and settings to find what performs best.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/marketplace"]',
    popover: {
      title: "Discover & monetize",
      description:
        "Browse proven strategies, or list your own for others to run. Every live trade is attested on-chain.",
      side: "right",
      align: "start",
    },
  },
  {
    popover: {
      title: "You're set",
      description:
        "That's the loop — build, prove, deploy, improve. Start by connecting a model; we'll take you to Settings → Providers now.",
    },
  },
];
