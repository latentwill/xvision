// Icon set ported from frontend/prototype/shared.jsx. Stroke-only line icons
// drawn on a 20x20 viewBox, current-color so they inherit theme tokens.

import type { ReactNode } from "react";

export type IconName =
  | "home"
  | "chart"
  | "play"
  | "stop"
  | "bars"
  | "book"
  | "db"
  | "cog"
  | "pulse"
  | "dollar"
  | "bag"
  | "barchart"
  | "diamond"
  | "code"
  | "arrow"
  | "check"
  | "branch"
  | "plus"
  | "search"
  | "chevR"
  | "chevDown"
  | "settings"
  | "box"
  | "user"
  | "list"
  | "flame"
  | "sliders"
  | "trash"
  | "sun"
  | "moon"
  | "chartPie"
  | "moreH"
  | "copy"
  | "pin"
  | "fileDown"
  | "fileCode"
  | "folderRight"
  | "openExternal"
  | "compare";

const PATHS: Record<IconName, ReactNode> = {
  home: <path d="M3 9.5L10 4l7 5.5V16a1 1 0 01-1 1h-3v-5H9v5H4a1 1 0 01-1-1V9.5z" />,
  chart: <path d="M3 16h14M5 13l3-4 3 2 4-6" />,
  play: (
    <>
      <circle cx="10" cy="10" r="7" />
      <path d="M8 7l5 3-5 3V7z" fill="currentColor" stroke="none" />
    </>
  ),
  stop: (
    <>
      <circle cx="10" cy="10" r="7" />
      <path d="M7.5 7.5h5v5h-5z" fill="currentColor" stroke="none" />
    </>
  ),
  bars: <path d="M4 16V8M8 16V5M12 16v-6M16 16v-9" />,
  book: <path d="M4 4h5a3 3 0 013 3v9a2 2 0 00-2-2H4V4zM16 4h-5a3 3 0 00-3 3v9a2 2 0 012-2h6V4z" />,
  db: (
    <>
      <ellipse cx="10" cy="5" rx="6" ry="2" />
      <path d="M4 5v10c0 1.1 2.7 2 6 2s6-.9 6-2V5M4 10c0 1.1 2.7 2 6 2s6-.9 6-2" />
    </>
  ),
  cog: (
    <>
      <circle cx="10" cy="10" r="2.5" />
      <path d="M10 2v2M10 16v2M16.4 6l-1.4.8M5 13.2L3.6 14M18 10h-2M4 10H2M16.4 14L15 13.2M5 6.8L3.6 6" />
    </>
  ),
  pulse: <path d="M3 10h3l2-5 4 10 2-5h3" />,
  dollar: (
    <>
      <circle cx="10" cy="10" r="7" />
      <path d="M10 6v8M12.5 7.5h-3.5a1.5 1.5 0 000 3h2a1.5 1.5 0 010 3H7.5" />
    </>
  ),
  bag: <path d="M5 7h10l-1 10H6L5 7zM7 7V5a3 3 0 016 0v2" />,
  barchart: <path d="M3 17h14M6 17V9M10 17V5M14 17v-6" />,
  diamond: <path d="M10 3l5 6-5 8-5-8 5-6z" />,
  code: <path d="M7 6l-4 4 4 4M13 6l4 4-4 4" />,
  arrow: <path d="M4 10h12M12 6l4 4-4 4" />,
  check: (
    <>
      <circle cx="10" cy="10" r="7" />
      <path d="M7 10l2 2 4-4" />
    </>
  ),
  branch: (
    <>
      <circle cx="6" cy="5" r="1.5" />
      <circle cx="6" cy="15" r="1.5" />
      <circle cx="14" cy="9" r="1.5" />
      <path d="M6 6.5v7M7.5 9h2A4.5 4.5 0 0014 9v-1.5" />
    </>
  ),
  plus: <path d="M10 4v12M4 10h12" />,
  search: (
    <>
      <circle cx="9" cy="9" r="5" />
      <path d="M13 13l4 4" />
    </>
  ),
  chevR: <path d="M8 5l5 5-5 5" />,
  settings: (
    <>
      <circle cx="10" cy="10" r="2.5" />
      <path d="M3 10h2M15 10h2M10 3v2M10 15v2M5 5l1.5 1.5M13.5 13.5L15 15M5 15l1.5-1.5M13.5 6.5L15 5" />
    </>
  ),
  box: <path d="M3 7l7-3 7 3v6l-7 3-7-3V7z M3 7l7 3 7-3M10 10v7" />,
  user: (
    <>
      <circle cx="10" cy="7" r="3" />
      <path d="M4 17c0-3 2.5-5 6-5s6 2 6 5" />
    </>
  ),
  list: <path d="M3 6h14M3 10h14M3 14h14" />,
  flame: <path d="M10 17c3 0 5-2 5-5 0-3-3-4-3-7-2 1-3 3-3 4-1-1-1.5-2-1.5-3-2 1.5-2.5 4-2.5 6 0 3 2 5 5 5z" />,
  sliders: (
    <>
      <path d="M4 6h6M14 6h2M4 10h2M10 10h6M4 14h10M16 14h0" />
      <circle cx="12" cy="6" r="1.5" />
      <circle cx="8" cy="10" r="1.5" />
      <circle cx="14" cy="14" r="1.5" />
    </>
  ),
  trash: (
    <>
      <path d="M3 5h14" />
      <path d="M7.5 5V3.5a1.5 1.5 0 011.5-1.5h2a1.5 1.5 0 011.5 1.5V5" />
      <path d="M5 5l.8 11a1.5 1.5 0 001.5 1.4h5.4a1.5 1.5 0 001.5-1.4L15 5" />
      <path d="M8.5 8v6M11.5 8v6" />
    </>
  ),
  sun: (
    <>
      <circle cx="10" cy="10" r="3" />
      <path d="M10 2.5v2M10 15.5v2M4.7 4.7l1.4 1.4M13.9 13.9l1.4 1.4M2.5 10h2M15.5 10h2M4.7 15.3l1.4-1.4M13.9 6.1l1.4-1.4" />
    </>
  ),
  moon: <path d="M14.5 13.8A6.5 6.5 0 016.2 5.5 6.5 6.5 0 1014.5 13.8z" />,
  chartPie: (
    <>
      <path d="M10 3a7 7 0 107 7h-7V3z" />
      <path d="M12 3a5 5 0 015 5h-5V3z" />
    </>
  ),
  chevDown: <path d="M5 8l5 5 5-5" />,
  moreH: (
    <>
      <circle cx="5" cy="10" r="1.5" fill="currentColor" stroke="none" />
      <circle cx="10" cy="10" r="1.5" fill="currentColor" stroke="none" />
      <circle cx="15" cy="10" r="1.5" fill="currentColor" stroke="none" />
    </>
  ),
  copy: (
    <>
      <rect x="8" y="8" width="9" height="9" rx="1" />
      <path d="M4 12V4a1 1 0 011-1h8" />
    </>
  ),
  pin: (
    <>
      <path d="M10 2l2 4h4l-3 3 1 5-4-2.5L6 14l1-5L4 6h4z" />
      <path d="M10 14v4" />
    </>
  ),
  fileDown: (
    <>
      <path d="M13 2H6a1 1 0 00-1 1v14a1 1 0 001 1h8a1 1 0 001-1V6z" />
      <polyline points="13 2 13 7 18 7" />
      <line x1="10" y1="9" x2="10" y2="14" />
      <polyline points="7 12 10 15 13 12" />
    </>
  ),
  fileCode: (
    <>
      <path d="M13 2H6a1 1 0 00-1 1v14a1 1 0 001 1h8a1 1 0 001-1V6z" />
      <polyline points="13 2 13 7 18 7" />
      <path d="M8 11l-2 2 2 2M12 11l2 2-2 2" />
    </>
  ),
  folderRight: (
    <>
      <path d="M4 4h5l2 2h5a1 1 0 011 1v8a1 1 0 01-1 1H4a1 1 0 01-1-1V5a1 1 0 011-1z" />
      <path d="M10 10h4M12 8l2 2-2 2" />
    </>
  ),
  openExternal: (
    <>
      <path d="M12 4h4v4M9 11l6-6M7 5H5a1 1 0 00-1 1v9a1 1 0 001 1h9a1 1 0 001-1v-2" />
    </>
  ),
  compare: (
    <>
      <path d="M4 6h12M4 10h12M4 14h12" />
      <path d="M8 4v12M12 4v12" />
    </>
  ),
};

export function Icon({
  name,
  size = 16,
  strokeWidth = 1.5,
  className,
}: {
  name: IconName;
  size?: number;
  strokeWidth?: number;
  className?: string;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      {PATHS[name]}
    </svg>
  );
}
