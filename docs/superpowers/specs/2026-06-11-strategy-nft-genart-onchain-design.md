# Strategy NFT generative art — onchain design (Bitfields v3, "unified family")

Date: 2026-06-11
Status: draft — pending operator review
Supersedes: the v1 hash-shapes SVG generator (`crates/xvision-identity/src/genart.rs` +
`frontend/web/src/features/marketplace/lib/genart.ts`, PR #741), which is retired by this design.
Related: `docs/testnft/code/v2/index.html` (bitfields v2 study),
`GenArtPlaceholder.tsx` (live card renderer), `docs/superpowers/specs/2026-06-08-live-trading-marketplace-spec.md` (§3.1 token stack).

## 1. Goal

Every minted strategy NFT gets deterministic generative identity art, stored **fully
onchain** as a `data:application/json;base64,…` tokenURI on Mantle (Sepolia now, mainnet
later), generated at mint time by the Rust backend and rendered identically by the
frontend for previews. The art must be:

- **Varied but never noise** — entropy is spent on a few discrete high-level traits
  (symmetry, palette, layer params); all visual richness comes from deterministic
  bitwise math downstream of those choices.
- **Cheap** — compact SVG, run-length encoded; target ≤8 KB raw SVG, hard ceiling
  12 KB tokenURI. On Mantle this stores for cents.
- **Reproducible everywhere** — Rust (mint path) and TypeScript (preview path) produce
  byte-identical SVG for the same seed. Golden-fixture tests enforce parity.

Design validated interactively 2026-06-11 (visual companion session
`.superpowers/brainstorm/63688-1781146860/`): operator selected the bitfield language +
symmetry-as-trait family over the lane prototypes (cathedral/faultline/circuit/tree)
and the v1 hash-shapes generator, and curated the 33-palette roster in §5.

## 2. Seed and trait derivation

```
seed_string = "{agent_id}:{manifest_hash}"
```

- `agent_id` — the pre-mint string ULID that becomes the NFT token id post-mint
  (terminology lock). Known before mint, so the URI is computable pre-mint.
- `manifest_hash` — 64-char lowercase hex of the listing content hash
  (keccak256 of the canonical strategy bundle JSON — the same `contentHash` stored in
  `ListingRegistry.createListing`). Ties the art immutably to the exact strategy
  artifact.

All randomness flows from two primitives, specified bit-exactly so Rust and TS agree:

- `fnv1a32(s)` — 32-bit FNV-1a over UTF-8 bytes (`h=2166136261; h^=byte; h*=16777619`,
  wrapping, output `u32`).
- `rng32(seed: u32)` — mulberry32: `t = seed += 0x6D2B79F5; t = imul(t^(t>>>15), t|1);
  t ^= t + imul(t^(t>>>7), t|61); return ((t^(t>>>14)) >>> 0) / 2^32` as f64.
  (Same generator already used by `GenArtPlaceholder.tsx` and all prototypes.)

Trait draws happen in a **fixed order** from `r = rng32(fnv1a32(seed_string))`:

1. `palette` — uniform pick from the locked roster (§5).
2. `symmetry` — pick from the weighted bag (§4).
3. (engine layer parameters are drawn afterwards inside the grid builder, §3)

The draw order is part of the spec; reordering breaks every minted token's
reproducibility and is forbidden after launch.

## 3. The engine: composited bitfield grid

Fixed canvas: **N=28 × 28 cells** of palette indices (`-1` = background).

`raw_grid(seed_str, layers=6, states=6, transparent=7)`:

For each layer `L` in `0..6`, draw from `r = rng32(fnv1a32(seed_str))` (one shared
`r` across layers, sequential):

- `op` ∈ {AND, XOR, OR} (uniform)
- `band = 1 + floor(r()*7)`, `base = 2 + floor(r()*9)`
- `xo = floor(r()*64)`, `yo = floor(r()*64)`
- `radial = r() > 0.7`, `invert = r() > 0.8`

Per cell `(x, y)`:

```
step = radial ? floor(hypot(x+xo-cx, y+yo-cy) / band)   // cx=N/2+xo, cy=N/2+yo
              : floor((y+yo) / band)
t = base + step
v = op(x+y+xo, y-x+yo);  if invert: v = ~v
v = ((v % t) + t) % t
s = v % (states + transparent)
if s >= transparent:  grid[y][x] = (s - transparent) % palette_len
```

Later layers overwrite earlier ones; transparent states skip (leaving prior layers
visible) — this compositing is what produces masses and bands instead of static.

**Density floor** (fixes the sparse-seed failure, e.g. seed `01HQOV8BA`):

```
transparent = 7
for attempt in 0..5:
    g = raw_grid(seed_str + (attempt ? "#"+attempt : ""), transparent)
    if filled(g) / (N*N) >= 0.14: return g
    transparent = max(2, transparent - 2)
return raw_grid(seed_str + "#final", transparent=2)
```

Pure-function retry — fully deterministic, portable.

`hypot` note: implementations MUST compute `sqrt(dx*dx + dy*dy)` as f64 (not a fused
`hypot` intrinsic) so Rust/TS round identically; inputs are small integers so this is
exact in practice, but the formula is normative.

## 4. Symmetry as a trait

The final image maps each display cell to a canonical source cell; the engine grid is
only read through this mapping. Eight modes, weighted bag of 14:

| Mode | Weight | Canonical mapping for (x, y), N=28 |
|---|---|---|
| `free` | 3 | `(x, y)` |
| `mirror-x` | 1 | `(min(x, N-1-x), y)` |
| `mirror-y` | 1 | `(x, min(y, N-1-y))` |
| `quad` | 3 | `(min(x, N-1-x), min(y, N-1-y))` |
| `diagonal` | 1 | `x < y ? (y, x) : (x, y)` |
| `anti-diagonal` | 1 | `x + y > N-1 ? (N-1-y, N-1-x) : (x, y)` |
| `rot180` | 1 | `idx(x,y) <= idx(N-1-x,N-1-y) ? (x,y) : (N-1-x,N-1-y)` |
| `rot90` | 2 | min-index cell among the 4 rotations `(x,y)→(y,N-1-x)→…` |

Weights double as the collection's rarity distribution and may be tuned pre-launch
only. The mode is emitted as an NFT attribute (`Symmetry`).

## 5. Palette roster

33 locked palettes (7 colors each: dark base → ramp → accent pop at slot 5 → cream).
Curated by the operator in the 2026-06-11 session. Roster lives in code as a shared
constant (Rust + TS, golden-tested); growing the roster is allowed anytime pre-launch
and **append-only** post-launch (existing indices/names must never change, since a
token's palette name is derived from roster order — see note below).

> **Roster-stability rule:** palette is picked by `floor(r() * roster_len)`. Because
> `roster_len` changes the pick for future mints only (each token's URI is computed
> and stored at mint), appending palettes post-launch is safe — already-minted tokens
> keep their stored URI. The roster constant must still never reorder or mutate
> existing entries, so historical seeds remain re-derivable for verification.

The launch roster is the exact 33-palette set validated on the unified-family wall:

- **Keepers from the explorer rounds (13):** `risoBlue, risoRedTeal, candyArcade,
  circuit, coldSignal, grapeSoda, punolit, calmSunset, lineage, signalRust,
  magmaCore, tidalDusk, ultraviolet` (4 of these — `circuit, coldSignal, punolit,
  calmSunset, lineage` — carry over from bitfields v2 / the lanes).
- **Pop-formula round (20):** `voltYellow, mintMagenta, tealEmber, indigoCoral,
  limeViolet, roseCyan, amberInk, crimsonMint, cobaltTangerine, orchidLime,
  pinkPitch, acidTeal, goldGrape, rustTurquoise, cherryCola, duskNeon, peachAbyss,
  saffronSea, furnacePink, glacierPunch`.

Full hex values are normative and listed in Appendix A.

**Pop formula** for future palettes: `[near-black base, deep ramp₁, ramp₂, ramp₃,
light ramp₄, contrasting accent pop, cream]` — accent hue roughly complementary to
the ramp hue.

Per-token onchain cost of palettes is ~80 bytes (only the 7 used hexes are embedded);
roster size is unconstrained.

## 6. SVG encoding

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 28 28" width="560" height="560"
     shape-rendering="crispEdges">
  <rect width="28" height="28" fill="{pal[0]}"/>
  <!-- per-row horizontal RLE: consecutive same-color cells merge into one rect -->
  <rect x="3" y="0" width="5" height="1" fill="{pal[i]}"/>
  …
</svg>
```

- Integer coordinates in grid units (1–2 chars each) keep rects ~45 bytes.
- The **post-symmetry** display grid is RLE-encoded directly (no `<use>` mirror
  tricks — symmetric grids RLE well anyway, and one code path means one parity
  surface; the ~1 KB saving is not worth the complexity on Mantle).
- `shape-rendering="crispEdges"` preserves the pixel aesthetic at any scale.
- Expected size: ~1.5–6 KB raw SVG. Hard test ceiling: 12 KB encoded tokenURI.

Metadata JSON (then base64-wrapped into the tokenURI):

```json
{
  "name": "xvn strategy {agent_id[:8]}",
  "image": "data:image/svg+xml;base64,…",
  "agent_id": "{agent_id}",
  "attributes": [
    {"trait_type": "Symmetry", "value": "quad"},
    {"trait_type": "Palette", "value": "risoBlue"},
    {"trait_type": "Density", "value": 41},
    {"trait_type": "Layers", "value": 6}
  ]
}
```

`Density` = percentage of filled cells in the display grid, rounded.

## 7. Components

| Unit | Location | Responsibility |
|---|---|---|
| `genart` module (rewrite) | `crates/xvision-identity/src/genart.rs` | `generate_svg(agent_id, manifest_hash) -> String`, `generate_token_uri(..) -> String`, `derive_traits(..) -> Traits`. Pure functions, no I/O. |
| TS twin (rewrite) | `frontend/web/src/features/marketplace/lib/genart.ts` | Same three functions, byte-identical output. |
| Shared canvas renderer | `frontend/web/src/features/marketplace/lib/genartGrid.ts` | Exposes `buildGrid(seed) -> {grid, traits}` consumed by both `genart.ts` (SVG) and `GenArtPlaceholder.tsx` (canvas), so card previews ARE the NFT art. |
| `GenArtPlaceholder.tsx` (modify) | same path | Switch from v2 `drawBitfield` to `buildGrid`; props unchanged (`seed, size, className`). Seed becomes `"{agent_id}:{manifest_hash}"` where both are available, falling back to current seeds elsewhere. |
| Mint wiring (Rust) | `crates/xvision-marketplace/src/adapter.rs` | `PublishRequest` gains `agent_id: String` + `manifest_hash: B256`; `publish_listing` pre-mints the identity NFT via `IdentityClient::register(generate_token_uri(..))` when `agent_nft_id` is absent, then `createListing`. |
| Backend route | `crates/xvision-dashboard` | `POST /api/marketplace/publish` — computes the canonical bundle hash, generates the URI, mints + lists, returns `{token_id, listing_id, tx_hashes}`. |
| Frontend publish | `features/marketplace/data/MarketplaceData.ts` | Replace fixture `submitListing` with a call to the new route. Sell-flow Step 3 preview renders `buildGrid` with the real seed so the operator sees the exact NFT before minting. |

No contract changes: `IdentityRegistry.register(string agentURI)` already stores
arbitrary URIs (ERC721URIStorage), and the deployed Mantle Sepolia instance is reused
as-is.

## 8. Error handling

- `manifest_hash` not 64-char hex → reject at the API boundary (400); the generator
  itself asserts and never silently falls back (the v1 FNV fallback is dropped —
  a wrong-hash mint must fail loudly, not mint wrong art).
- Empty `agent_id` → reject (400 / `Err`).
- Mint tx failure → no listing is created; the publish call is sequenced
  register → createListing, and a registered-but-unlisted agent NFT is recoverable
  (publish retries skip registration if the NFT exists).
- Density floor guarantees no near-empty art for any seed.

## 9. Testing (TDD, per repo policy)

1. **Golden fixtures** — `fixtures/genart_v3.json`: ~24 seed → {svg, traits, uri}
   triples generated once, hand-reviewed. Rust and TS test suites both assert exact
   matches; this IS the parity contract.
2. **Density property test** — 1,000 derived seeds all ≥14% fill.
3. **RLE round-trip** — decode emitted rects back to a grid, assert equal to source.
4. **Size ceiling** — all fixture + property seeds produce tokenURI ≤ 12 KB.
5. **Symmetry law tests** — for each mode, assert the display grid satisfies its
   invariant (e.g. `quad`: `g[x][y] == g[N-1-x][y] == g[x][N-1-y]`).
6. **Wiring tests** — publish route happy path + invalid-hash rejection; adapter
   pre-mint sequencing against the existing mock/anvil harness.

## 10. Out of scope

- Migrating/re-uriing any previously minted test tokens.
- The lane renderers (cathedral/faultline/circuit/tree) — parked in `docs/testnft/`.
- An onchain renderer contract (registry stores full URIs; revisit only if per-mint
  cost on mainnet proves material).
- Animation, HTML tokenURIs, offchain image servers.

## Appendix A — palette roster (normative hex values)

| Name | Colors (base → ramp → pop → cream) |
|---|---|
| risoBlue | `#0d1026 #1c2a6b #2f4bb8 #3f6df2 #7fa3ff #ffd23f #fff6e0` |
| risoRedTeal | `#140a0d #5c1a2e #c1224f #ff5470 #1ca3a3 #9fe3d4 #fff3e8` |
| candyArcade | `#0d0714 #2e1245 #5a1f7d #9032a8 #e84393 #ffd24f #fff5dc` |
| circuit | `#041013 #08242b #0f5260 #18a98f #a5f3dc #ff3b73 #ffe95e` |
| coldSignal | `#071019 #102936 #23545b #44a3a3 #c5e4dc #f44465 #ffe6a7` |
| grapeSoda | `#0c0714 #231140 #41207a #6a39b8 #9c6be0 #cda8f0 #f3e8fd` |
| punolit | `#11151f #1e3442 #35665f #89a36a #d5c686 #df9d8b #e8d7cf` |
| calmSunset | `#2c1534 #5c2751 #a94768 #df8584 #f3cda9 #f7e6b0 #fff7d6` |
| lineage | `#080916 #182044 #263b71 #426e91 #75a57d #d2bc72 #f6ead0` |
| signalRust | `#0c0b0a #26211c #4f4138 #8a6450 #c97f4f #f25c3a #ffe9d4` |
| magmaCore | `#0c0508 #330a12 #70101c #bf1f26 #f2542d #ffa552 #ffe8c2` |
| tidalDusk | `#0a1012 #103035 #1a5e60 #2f9a8c #e6c36b #ef9f63 #fcefd2` |
| ultraviolet | `#08051a #160d44 #2a1a80 #4730c4 #7a5ef2 #b49cff #e9e2ff` |
| voltYellow | `#0a0a10 #1d2433 #2f4866 #4a7ab8 #7fb3e8 #ffe83f #fdf8e2` |
| mintMagenta | `#070f0d #0f2e26 #1a5c47 #2f9a73 #8fe0bb #f23fa0 #fff0f7` |
| tealEmber | `#06100f #0e3331 #176561 #2aa39a #aee8df #ff7733 #ffeed9` |
| indigoCoral | `#08081a #161a4d #2a2f8f #4a55d6 #9aa3f2 #ff6f61 #fff1e8` |
| limeViolet | `#0b0d06 #222e0d #3f5c14 #6f9a1f #b8e040 #8a3ff2 #f4eaff` |
| roseCyan | `#120710 #3a0f2e #73195c #b8268f #f060c4 #2ee6e6 #e8feff` |
| amberInk | `#0b0a12 #1f1d33 #3a3866 #5c59a8 #9a97d9 #ffb347 #fff3da` |
| crimsonMint | `#120709 #3f0d18 #7c142b #c41f44 #f25c77 #5ce8b8 #eafff6` |
| cobaltTangerine | `#06091a #0e1f56 #1a3b9e #2f63e0 #85aaf2 #ff9433 #fff0dc` |
| orchidLime | `#100818 #2e1247 #5a2080 #9438c4 #d685f0 #cfe83f #f9ffe0` |
| pinkPitch | `#0d0c0c #1f1d1f #3b373b #6b6168 #b3a6ad #ff3f8e #ffe6f1` |
| acidTeal | `#0c1206 #1f330d #3f6618 #6fa826 #b8e84a #1fb8c9 #e0fbff` |
| goldGrape | `#0e0814 #291245 #4d1f7d #7d33b8 #b370e0 #ffd23f #fff6dc` |
| rustTurquoise | `#120b08 #3b1c10 #73331a #b85426 #e88a4f #2ec9b8 #e8fcf7` |
| cherryCola | `#100808 #330f12 #661a21 #a82a35 #e0525c #ffc26b #fff0d9` |
| duskNeon | `#0a0814 #1d1640 #352a73 #5444a8 #8a73d9 #3fffb8 #eafff5` |
| peachAbyss | `#050811 #0d1c3a #173366 #2a52a3 #6f8fd9 #ffb38a #fff0e2` |
| saffronSea | `#071013 #0f2c38 #1a5366 #2f85a3 #73c2d9 #ffc63f #fff6da` |
| furnacePink | `#0f070c #360d2b #6e1452 #b81f7d #f23fb0 #ffae3f #ffeed4` |
| glacierPunch | `#070b10 #13283d #234a73 #3f78b3 #8fc1e8 #f2543f #ffe9e0` |
