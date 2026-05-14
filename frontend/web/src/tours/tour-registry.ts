export type TourStep = {
  element: string;
  popover: {
    title: string;
    description: string;
  };
};

export type RouteTour = {
  id: string;
  route: string;
  steps: TourStep[];
};

export const ROUTE_TOURS: RouteTour[] = [
  {
    id: "home",
    route: "/",
    steps: [
      {
        element: '[data-tour="home-attention"]',
        popover: {
          title: "Workspace attention",
          description:
            "This panel collects failed runs, missing provider keys, and broker setup gaps before you start new work.",
        },
      },
      {
        element: '[data-tour="home-runs"]',
        popover: {
          title: "Recent eval runs",
          description:
            "Use this list to jump back into the latest backtests and paper runs.",
        },
      },
      {
        element: '[data-tour="home-counts"]',
        popover: {
          title: "Workspace inventory",
          description:
            "These counters show whether strategies, reusable agents, and providers are ready for evaluation.",
        },
      },
    ],
  },
];

export function tourForPath(pathname: string): RouteTour | null {
  return ROUTE_TOURS.find((tour) => tour.route === pathname) ?? null;
}
