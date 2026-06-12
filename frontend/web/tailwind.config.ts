import type { Config } from "tailwindcss";

// Wrap a CSS variable so Tailwind opacity modifiers (border-gold/40, bg-gold/10,
// etc.) actually generate CSS. Without this, Tailwind v3 silently drops the
// opacity modifier for CSS-variable-based colors, leaving border-color at
// Tailwind's preflight default (#e5e7eb — near-white), which looks like a
// bright white outline on dark backgrounds.
function cv(variable: string) {
  const fn = ({ opacityValue }: { opacityValue?: string }) => {
    if (opacityValue !== undefined) {
      const pct = Math.round(parseFloat(opacityValue) * 100);
      if (!isNaN(pct)) {
        return `color-mix(in srgb, var(${variable}) ${pct}%, transparent)`;
      }
    }
    return `var(${variable})`;
  };
  return fn as unknown as string;
}

// Signal theme — tokens live in src/styles/tokens.css; this config maps
// Tailwind utilities onto those CSS variables so components can use either.
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: cv("--bg"),
        "surface-sidebar": cv("--surface-sidebar"),
        "surface-card": cv("--surface-card"),
        "surface-elev": cv("--surface-elev"),
        "surface-panel": cv("--surface-panel"),
        "surface-hover": cv("--surface-hover"),

        border: cv("--border"),
        "border-strong": cv("--border-strong"),
        "border-soft": cv("--border-soft"),

        text: cv("--text"),
        "text-2": cv("--text-2"),
        "text-3": cv("--text-3"),
        "text-4": cv("--text-4"),

        accent: cv("--accent"),
        "on-accent": cv("--on-accent"),

        gold: cv("--gold"),
        "gold-soft": cv("--gold-soft"),
        "gold-bg": cv("--gold-bg"),
        "gold-bg-strong": cv("--gold-bg-strong"),
        warn: cv("--warn"),
        danger: cv("--danger"),
        info: cv("--info"),
        violet: cv("--violet"),
      },
      fontFamily: {
        // `serif` retained as a Tailwind utility name for compatibility with
        // existing `font-serif` classes — Signal has no serif, it maps to Geist.
        serif: ["'Geist'", "ui-sans-serif", "system-ui", "sans-serif"],
        sans: ["'Geist'", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ["'Geist Mono'", "ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
      },
      borderRadius: {
        card: "6px",
      },
      fontSize: {
        // The prototype baselines body text at 13px; reflect that here.
        base: ["13px", "1.45"],
      },
      // Motion tokens (src/styles/tokens.css). Enable `duration-fast/base/slow`
      // and `ease-out` utilities; the global reduced-motion block collapses them.
      transitionDuration: {
        fast: "var(--duration-fast)",
        base: "var(--duration-base)",
        slow: "var(--duration-slow)",
      },
      transitionTimingFunction: {
        out: "var(--ease-out)",
      },
    },
  },
  plugins: [],
} satisfies Config;
