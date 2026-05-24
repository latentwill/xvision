import type { Config } from "tailwindcss";

// Signal theme — tokens live in src/styles/tokens.css; this config maps
// Tailwind utilities onto those CSS variables so components can use either.
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "var(--bg)",
        "surface-sidebar": "var(--surface-sidebar)",
        "surface-card": "var(--surface-card)",
        "surface-elev": "var(--surface-elev)",
        "surface-panel": "var(--surface-panel)",
        "surface-hover": "var(--surface-hover)",

        border: "var(--border)",
        "border-strong": "var(--border-strong)",
        "border-soft": "var(--border-soft)",

        text: "var(--text)",
        "text-2": "var(--text-2)",
        "text-3": "var(--text-3)",
        "text-4": "var(--text-4)",

        gold: "var(--gold)",
        "gold-soft": "var(--gold-soft)",
        warn: "var(--warn)",
        danger: "var(--danger)",
        info: "var(--info)",
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
    },
  },
  plugins: [],
} satisfies Config;
