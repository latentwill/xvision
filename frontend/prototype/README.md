# xvn v1 design prototype

Runnable HTML/JSX mockup of the v1 xvision dashboard, exported from Claude Design on 2026-05-10. Visual source of truth for the upcoming production frontend.

The original entry file `xvn v1 design.html` was renamed to `index.html` for ergonomics; everything else is unchanged.

## Run locally

```sh
python3 -m http.server 8000   # from this directory
# open http://localhost:8000/
```

No build step — Babel transpiles JSX in the browser. React 18 + Babel are loaded from unpkg.

## Files

| File | Role |
|---|---|
| `index.html` | Design canvas entry — mounts `<DesignCanvas>` with all 6 artboards |
| `index-print.html` | Print-friendly stack of all 6 screens (Cmd+P → Save as PDF, landscape 1440×900) |
| `styles.css` | Folio dark theme tokens + component CSS |
| `design-canvas.jsx` | Figma-ish wrapper providing `<DCSection>` + `<DCArtboard>` |
| `shared.jsx` | `Icon`, `Sidebar`, `Topbar`, `Sparkline` — reused across screens |
| `screen-*.jsx` | One file per screen (home, setup, strategies, inspector, eval-runs, run-detail) |
| `_handoff/` | Original handoff README, chat transcript, upload reference images |

## Screen index

See `frontend/README.md` for the full screen list and v1 scope.

## Don't

- Don't render this in a browser to "verify" what it looks like — read the source. The HTML/CSS spell out every dimension, color, and layout rule.
- Don't copy the prototype's internal structure when building the real frontend — match the visual output, but use whatever component architecture fits the production stack.
