import type { Config, DriveStep, PopoverDOM } from "driver.js";

const STEP_EYEBROWS = [
  "First run",
  "Step 01 - Connect",
  "Step 02 - Build",
  "Step 03 - Define",
  "Step 04 - Evaluate",
  "Next - Deploy",
  "Next - Improve",
  "Next - Discover",
  "Ready",
];

const BRAND_SVG = `
<svg width="58" height="17" viewBox="0 0 48 14" xmlns="http://www.w3.org/2000/svg" aria-label="XVN">
  <g stroke="var(--tour-accent)" stroke-width="1.4" fill="none" stroke-linecap="square">
    <path d="M4 1 H1 V13 H4"/><path d="M44 1 H47 V13 H44"/>
  </g>
  <text x="24" y="7" fill="#ffffff" font-family="'Geist Mono', ui-monospace, monospace"
        font-size="13" font-weight="700" dominant-baseline="central" text-anchor="middle">
    <tspan>X</tspan><tspan dx="0.14em">V</tspan><tspan dx="0.14em">N</tspan>
  </text>
</svg>`;

export interface TourThemeOptions {
  overlay?: number;
  stageRadius?: number;
  stagePadding?: number;
  glow?: boolean;
  accent?: string;
}

type ResolvedTourThemeOptions = Required<Omit<TourThemeOptions, "accent">> &
  Pick<TourThemeOptions, "accent">;

type TourThemeConfig = Config & {
  __teardown: () => void;
};

const defaults: Required<Omit<TourThemeOptions, "accent">> = {
  overlay: 0.74,
  stageRadius: 8,
  stagePadding: 6,
  glow: true,
};

let spotlightEl: HTMLDivElement | null = null;
let currentTarget: Element | null | undefined;

function ensureSpotlight(): HTMLDivElement {
  if (!spotlightEl) {
    spotlightEl = document.createElement("div");
    spotlightEl.id = "xvn-spotlight";
    document.body.appendChild(spotlightEl);
  }
  return spotlightEl;
}

function resolveOptions(options: TourThemeOptions): ResolvedTourThemeOptions {
  return { ...defaults, ...options };
}

function positionSpotlight(
  element: Element | null | undefined,
  options: ResolvedTourThemeOptions,
) {
  const spotlight = ensureSpotlight();
  spotlight.style.setProperty(
    "--xvn-scrim",
    `color-mix(in srgb, #05070b ${Math.round(options.overlay * 100)}%, transparent)`,
  );
  spotlight.style.borderRadius = `${options.stageRadius}px`;
  spotlight.classList.toggle("glow", options.glow);

  if (!element || !("getBoundingClientRect" in element)) {
    spotlight.classList.remove("has-target");
    spotlight.style.left = "50%";
    spotlight.style.top = "44%";
    spotlight.style.width = "0";
    spotlight.style.height = "0";
    spotlight.style.opacity = "1";
    return;
  }

  const rect = element.getBoundingClientRect();
  const padding = options.stagePadding;
  spotlight.classList.add("has-target");
  spotlight.style.left = `${rect.left - padding}px`;
  spotlight.style.top = `${rect.top - padding}px`;
  spotlight.style.width = `${rect.width + padding * 2}px`;
  spotlight.style.height = `${rect.height + padding * 2}px`;
  spotlight.style.opacity = "1";
}

function bindReposition(options: ResolvedTourThemeOptions): () => void {
  const handler = () => {
    if (currentTarget !== undefined) {
      positionSpotlight(currentTarget, options);
    }
  };

  window.addEventListener("resize", handler, { passive: true });
  window.addEventListener("scroll", handler, { passive: true, capture: true });

  return () => {
    window.removeEventListener("resize", handler);
    window.removeEventListener("scroll", handler, true);
  };
}

function removeSpotlight() {
  spotlightEl?.remove();
  spotlightEl = null;
  currentTarget = null;
}

function buildProgress(current: number, total: number): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "xvn-tour-progress";

  const track = document.createElement("div");
  track.className = "xvn-tour-segs";

  for (let index = 0; index < total; index += 1) {
    const segment = document.createElement("span");
    segment.className = "xvn-tour-seg";
    if (index <= current) segment.classList.add("is-done");
    if (index === current) segment.classList.add("is-current");
    track.appendChild(segment);
  }

  const count = document.createElement("span");
  count.className = "xvn-tour-count";
  count.innerHTML = `<b>${String(current + 1).padStart(2, "0")}</b> / ${String(
    total,
  ).padStart(2, "0")}`;

  wrap.appendChild(track);
  wrap.appendChild(count);
  return wrap;
}

function makeOnPopoverRender(total: number): NonNullable<Config["onPopoverRender"]> {
  return (popover: PopoverDOM, opts) => {
    const index = opts.state.activeIndex ?? 0;
    const isWelcome = index === 0;
    popover.wrapper.classList.toggle("is-welcome", isWelcome);

    if (isWelcome) {
      const brand = document.createElement("div");
      brand.className = "xvn-tour-brand";
      brand.innerHTML = BRAND_SVG;
      popover.wrapper.insertBefore(brand, popover.title);
    }

    const eyebrowText = STEP_EYEBROWS[index];
    if (eyebrowText) {
      const eyebrow = document.createElement("div");
      eyebrow.className = "xvn-tour-eyebrow";
      eyebrow.textContent = eyebrowText;
      popover.wrapper.insertBefore(eyebrow, popover.title);
    }

    popover.footer.insertBefore(buildProgress(index, total), popover.footer.firstChild);
  };
}

export function tourThemeConfig(
  steps: DriveStep[],
  themeOptions: TourThemeOptions = {},
): TourThemeConfig {
  const options = resolveOptions(themeOptions);
  if (options.accent) {
    document.documentElement.style.setProperty("--tour-accent", options.accent);
  }
  document.body.classList.toggle("tour-glow", options.glow);

  let unbind: (() => void) | null = null;

  return {
    showProgress: false,
    animate: true,
    overlayColor: "#05070b",
    overlayOpacity: 0,
    stagePadding: options.stagePadding,
    stageRadius: options.stageRadius,
    popoverClass: "xvn-tour",
    smoothScroll: true,
    nextBtnText: "Next",
    prevBtnText: "Back",
    doneBtnText: "Finish tour",
    onPopoverRender: makeOnPopoverRender(steps.length),
    onHighlightStarted: (element) => {
      currentTarget = element ?? null;
      positionSpotlight(element, options);
      if (!unbind) unbind = bindReposition(options);
    },
    __teardown: () => {
      unbind?.();
      unbind = null;
      document.body.classList.remove("tour-glow");
      removeSpotlight();
    },
  };
}
