// VirtualsMark — Virtuals co-branding mark (placeholder).
//
// Renders an inline SVG at the requested size. The glyph is a neutral
// geometric placeholder; the official Virtuals brand-kit SVG must be
// swapped in before public release. See:
//   src/assets/brands/virtuals/README.md

export interface VirtualsMarkProps {
  /** Square dimension in pixels (default: 14). */
  size?: number;
  /** Extra CSS classes forwarded to the <svg> element. */
  className?: string;
}

/**
 * Virtuals co-branding mark.
 *
 * TODO: replace the inline path data with the official Virtuals SVG from
 * their brand kit (https://brand.virtuals.io). See
 * src/assets/brands/virtuals/README.md for swap instructions.
 *
 * PLACEHOLDER — the current shape is a simple diamond-ring glyph that
 * carries no Virtuals visual identity. It MUST NOT appear in any
 * public-facing release as-is.
 */
export function VirtualsMark({ size = 14, className }: VirtualsMarkProps) {
  return (
    <svg
      data-testid="virtuals-mark"
      aria-label="Virtuals"
      role="img"
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      {/* PLACEHOLDER glyph — diamond-ring; NOT the Virtuals logo */}
      <polygon points="12,3 21,12 12,21 3,12" />
      <line x1="3" y1="12" x2="21" y2="12" />
    </svg>
  );
}
