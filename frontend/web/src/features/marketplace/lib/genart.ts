function fnvBytes(input: string): Uint8Array {
  const SEEDS = [
    0xcbf29ce484222325n,
    0x14650fb0739d0383n,
    0x9368da02daeb19d6n,
    0x5ada82de37be73d1n,
  ];
  const PRIME = 0x100000001b3n;
  const MASK = 0xffffffffffffffffn;
  const result = new Uint8Array(32);
  for (let i = 0; i < 4; i++) {
    let h = SEEDS[i];
    for (let j = 0; j < input.length; j++) {
      h = ((h ^ BigInt(input.charCodeAt(j))) * PRIME) & MASK;
    }
    h = (h ^ (BigInt(i) * 0x517cc1b727220a95n)) & MASK;
    for (let b = 0; b < 8; b++) {
      result[i * 8 + b] = Number((h >> BigInt(b * 8)) & 0xffn);
    }
  }
  return result;
}

function hexDecode32(s: string): Uint8Array | null {
  if (s.length !== 64) return null;
  const result = new Uint8Array(32);
  for (let i = 0; i < 32; i++) {
    const hi = parseInt(s[i * 2], 16);
    const lo = parseInt(s[i * 2 + 1], 16);
    if (isNaN(hi) || isNaN(lo)) return null;
    result[i] = (hi << 4) | lo;
  }
  return result;
}

function hslToHex(hue: number, sat: number, light: number): string {
  const s = sat / 100;
  const l = light / 100;
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = l - c / 2;
  let r1 = 0, g1 = 0, b1 = 0;
  if (hue < 60)       { r1 = c; g1 = x; b1 = 0; }
  else if (hue < 120) { r1 = x; g1 = c; b1 = 0; }
  else if (hue < 180) { r1 = 0; g1 = c; b1 = x; }
  else if (hue < 240) { r1 = 0; g1 = x; b1 = c; }
  else if (hue < 300) { r1 = x; g1 = 0; b1 = c; }
  else                { r1 = c; g1 = 0; b1 = x; }
  const byte = (v: number) => Math.floor(Math.min(1, Math.max(0, v + m)) * 255);
  return `#${byte(r1).toString(16).padStart(2, '0')}${byte(g1).toString(16).padStart(2, '0')}${byte(b1).toString(16).padStart(2, '0')}`;
}

function base64Encode(data: Uint8Array): string {
  const CHARS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  const n = data.length;
  const full = Math.floor(n / 3);
  const rem = n % 3;
  let out = '';
  for (let i = 0; i < full; i++) {
    const b = (data[i*3] << 16) | (data[i*3+1] << 8) | data[i*3+2];
    out += CHARS[(b >> 18) & 0x3f] + CHARS[(b >> 12) & 0x3f] + CHARS[(b >> 6) & 0x3f] + CHARS[b & 0x3f];
  }
  if (rem === 1) {
    const b = data[full*3] << 16;
    out += CHARS[(b >> 18) & 0x3f] + CHARS[(b >> 12) & 0x3f] + '==';
  } else if (rem === 2) {
    const b = (data[full*3] << 16) | (data[full*3+1] << 8);
    out += CHARS[(b >> 18) & 0x3f] + CHARS[(b >> 12) & 0x3f] + CHARS[(b >> 6) & 0x3f] + '=';
  }
  return out;
}

export function generateSvg(agentId: string, manifestHash: string): string {
  if (!agentId) throw new Error('agentId must be non-empty');
  if (manifestHash.length !== 64) throw new Error('manifestHash must be 64-char hex');
  const e = hexDecode32(manifestHash) ?? fnvBytes(manifestHash);
  const p = fnvBytes(agentId);
  const hue = (p[0] / 255) * 360;
  const sat = 60 + (p[1] % 30);
  const lit = 50 + (p[2] % 20);
  const c1 = hslToHex(hue, sat, lit);
  const c2 = hslToHex((hue + 120) % 360, sat, lit);
  const c3 = hslToHex((hue + 240) % 360, sat, lit);
  const label = agentId.substring(0, 8);
  const u = (b: number, max: number) => Math.floor(b * max / 255);
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 400" width="400" height="400">`,
    `<rect width="400" height="400" fill="#0a0a0f"/>`,
    `<circle cx="${50+u(e[3],300)}" cy="${50+u(e[4],300)}" r="${40+u(e[5],80)}" fill="${c1}" opacity="0.7"/>`,
    `<circle cx="${50+u(e[6],300)}" cy="${50+u(e[7],300)}" r="${30+u(e[8],60)}" fill="${c2}" opacity="0.6"/>`,
    `<rect x="${u(e[9],350)}" y="${u(e[10],350)}" width="${20+u(e[11],100)}" height="${20+u(e[12],100)}" fill="${c3}" opacity="0.5"/>`,
    `<rect x="${u(e[13],350)}" y="${u(e[14],350)}" width="${15+u(e[15],80)}" height="${15+u(e[16],80)}" fill="${c1}" opacity="0.4"/>`,
    `<line x1="${u(e[17],400)}" y1="${u(e[18],400)}" x2="${u(e[19],400)}" y2="${u(e[20],400)}" stroke="${c2}" stroke-width="2" opacity="0.8"/>`,
    `<line x1="${u(e[21],400)}" y1="${u(e[22],400)}" x2="${u(e[23],400)}" y2="${u(e[24],400)}" stroke="${c3}" stroke-width="1.5" opacity="0.7"/>`,
    `<polygon points="${u(e[25],400)},${u(e[26],400)} ${u(e[27],400)},${u(e[28],400)} ${u(e[29],400)},${u(e[30],400)}" fill="${c2}" opacity="0.45"/>`,
    `<text x="8" y="392" font-family="monospace" font-size="9" fill="${c1}" opacity="0.6">${label}</text>`,
    `</svg>`,
  ].join('');
}

export function generateTokenUri(agentId: string, manifestHash: string): string {
  const svg = generateSvg(agentId, manifestHash);
  const svgB64 = base64Encode(new TextEncoder().encode(svg));
  const short = agentId.substring(0, 8);
  const json = JSON.stringify({ name: `xvn agent ${short}`, image: `data:image/svg+xml;base64,${svgB64}`, agent_id: agentId });
  return `data:application/json;base64,${base64Encode(new TextEncoder().encode(json))}`;
}
