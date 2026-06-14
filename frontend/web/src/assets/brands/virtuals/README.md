# Virtuals brand asset

## TODO: swap in the official asset

`virtuals-mark.svg` is a **placeholder** — a neutral geometric glyph that
carries none of Virtuals' actual visual identity. It must be replaced with
the official mark before any public release.

### How to replace

1. Obtain the official SVG from the Virtuals brand kit
   (https://brand.virtuals.io or directly from the Virtuals team).
2. Overwrite `virtuals-mark.svg` with the downloaded file — keep the same
   filename so the import in `VirtualsMark.tsx` still resolves.
3. Do **not** restyle, recolour, or transform the official mark. Render it
   at the size passed via the `size` prop only.
4. Delete or update this README when the swap is complete.

### Constraints

- The component that consumes this SVG (`VirtualsMark.tsx`) passes
  `aria-label="Virtuals"` — preserve that accessibility label.
- The SVG should render cleanly at 12 px–24 px square (the sizes used in
  venue-selector labels and footers). Confirm the official asset works at
  those sizes before removing the placeholder.
