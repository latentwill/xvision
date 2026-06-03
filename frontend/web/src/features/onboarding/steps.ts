import type { DriveStep } from "driver.js";

export const firstRunTourSteps: DriveStep[] = [
  {
    popover: {
      title: "Welcome to xvn",
      description:
        "A quick three-step tour of the surfaces you'll use most. You can dismiss at any step.",
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
];
