import type { DriveStep } from "driver.js";

export const firstRunTourSteps: DriveStep[] = [
  {
    popover: {
      title: "Welcome to XVN",
      description:
        "A quick tour of the surfaces you'll use most: build, test, deploy, discover, and improve agent strategies.",
    },
  },
  {
    element: 'a[href="/strategies"]',
    popover: {
      title: "Strategies",
      description:
        "Author and inspect the strategies your agents will trade. Each strategy composes one or more agents.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/scenarios"]',
    popover: {
      title: "Scenarios",
      description:
        "Define market windows — asset, date range, granularity, fees, slippage — to evaluate strategies against.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/eval-runs"]',
    popover: {
      title: "Eval Runs",
      description:
        "Launch backtests or paper runs, watch decisions stream in, and compare arms when a run completes.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/live"]',
    popover: {
      title: "Live Trading",
      description:
        "Monitor deployed strategies, venue account state, positions, and transport controls when real money is moving.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/marketplace"]',
    popover: {
      title: "Marketplace",
      description:
        "Browse, buy, sell, and inspect strategy lineage across the XVN marketplace.",
      side: "right",
      align: "start",
    },
  },
  {
    element: 'a[href="/optimizer"]',
    popover: {
      title: "Optimizer",
      description:
        "Run improvement cycles, review experiments, and promote stronger strategy variants.",
      side: "right",
      align: "start",
    },
  },
];
