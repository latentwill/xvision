// genart.ts — Bitfields v3 SVG + tokenURI. Byte-identical twin of
// crates/xvision-identity/src/genart.rs; parity enforced by tests/fixtures/genart_v3.json.
// SVG body uses stroke-path encoding: one <path> per palette index with ≥1 run,
// in ascending palette-index order; each run: M{x} {y}.5h{w}.
import { N, buildGrid, type Traits } from "./genartGrid";

function base64Encode(data: Uint8Array): string {
  const CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const n = data.length;
  const full = Math.floor(n / 3);
  const rem = n % 3;
  let out = "";
  for (let i = 0; i < full; i++) {
    const b = (data[i * 3] << 16) | (data[i * 3 + 1] << 8) | data[i * 3 + 2];
    out += CHARS[(b >> 18) & 0x3f] + CHARS[(b >> 12) & 0x3f] + CHARS[(b >> 6) & 0x3f] + CHARS[b & 0x3f];
  }
  if (rem === 1) {
    const b = data[full * 3] << 16;
    out += CHARS[(b >> 18) & 0x3f] + CHARS[(b >> 12) & 0x3f] + "==";
  } else if (rem === 2) {
    const b = (data[full * 3] << 16) | (data[full * 3 + 1] << 8);
    out += CHARS[(b >> 18) & 0x3f] + CHARS[(b >> 12) & 0x3f] + CHARS[(b >> 6) & 0x3f] + "=";
  }
  return out;
}

export function deriveTraits(agentId: string, manifestHash: string): Traits {
  return buildGrid(agentId, manifestHash).traits;
}

export function generateSvg(agentId: string, manifestHash: string): string {
  const { grid, palette } = buildGrid(agentId, manifestHash);
  const parts: string[] = [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${N} ${N}" width="560" height="560" shape-rendering="crispEdges">`,
    `<rect width="${N}" height="${N}" fill="${palette[0]}"/>`,
  ];
  // Build per-palette-index run lists in row-major order
  const dStrings: string[] = new Array(palette.length).fill("");
  for (let y = 0; y < N; y++) {
    let x = 0;
    while (x < N) {
      const v = grid[y * N + x];
      if (v < 0) { x++; continue; }
      let x2 = x + 1;
      while (x2 < N && grid[y * N + x2] === v) x2++;
      dStrings[v] += `M${x} ${y}.5h${x2 - x}`;
      x = x2;
    }
  }
  // Emit one <path> per palette index with ≥1 run, ascending index order
  for (let i = 0; i < palette.length; i++) {
    if (dStrings[i].length > 0) {
      parts.push(`<path stroke="${palette[i]}" stroke-width="1" d="${dStrings[i]}"/>`);
    }
  }
  parts.push("</svg>");
  return parts.join("");
}

export function generateTokenUri(agentId: string, manifestHash: string): string {
  const { traits } = buildGrid(agentId, manifestHash);
  const svg = generateSvg(agentId, manifestHash);
  const svgB64 = base64Encode(new TextEncoder().encode(svg));
  const short = agentId.substring(0, 8);
  // Field order is normative — the Rust twin emits the identical byte string.
  const json =
    `{"name":"xvn strategy ${short}",` +
    `"image":"data:image/svg+xml;base64,${svgB64}",` +
    `"agent_id":"${agentId}",` +
    `"attributes":[` +
    `{"trait_type":"Symmetry","value":"${traits.symmetry}"},` +
    `{"trait_type":"Palette","value":"${traits.palette}"},` +
    `{"trait_type":"Density","value":${traits.density}},` +
    `{"trait_type":"Layers","value":${traits.layers}}]}`;
  return `data:application/json;base64,${base64Encode(new TextEncoder().encode(json))}`;
}
