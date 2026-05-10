#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = ["openai>=1.55", "python-dotenv>=1.0"]
# ///
"""Render xvn v1 UI mockups in dark and light Folio modes via GPT Image 2.

Reads ../gptprompts-v1.md, splits into 2 design-system blocks (DARK / LIGHT) and
the route prompts, then calls openai.images.generate() for each (route × mode)
combination. Saves outputs into this directory.

Usage:
    uv run _render.py                  # render all missing
    uv run _render.py --list           # enumerate without rendering
    uv run _render.py --dark           # only dark mode
    uv run _render.py --light          # only light mode
    uv run _render.py 1                # only route #1 (both modes)
    uv run _render.py wizard           # only routes whose slug contains "wizard"
    uv run _render.py --quality medium # cheaper drafts (default high)
    uv run _render.py --size 1024x1024 # square instead of landscape

Idempotent: skips files that already exist (use --force to overwrite).
Reads OPENAI_API_KEY from process env, then ./.env, then ~/.env.
"""
from __future__ import annotations

import argparse
import base64
import os
import re
import sys
import urllib.request
from pathlib import Path

from dotenv import load_dotenv
from openai import APIError, OpenAI

PROMPTS_FILE = Path(__file__).resolve().parent.parent / "gptprompts-v1.md"
OUT_DIR = Path(__file__).resolve().parent


def parse_prompts(text: str):
    def block(label: str) -> str | None:
        m = re.search(
            rf"## Folio Shared Design System — {label} MODE.*?\n```\n(.*?)\n```",
            text,
            re.S,
        )
        return m.group(1).strip() if m else None

    dark = block("DARK")
    light = block("LIGHT")

    routes = []
    # Heading line uses [^\n]+? to keep within a single line.
    # Body uses .*? with re.S to span until the next ``` fence.
    for m in re.finditer(
        r"^### (\d+)\.\s+([^\n]+?)\n+```\n(.*?)\n```",
        text,
        re.M | re.S,
    ):
        num = int(m.group(1))
        heading = m.group(2).strip()
        prompt = m.group(3).strip()
        if " — `" in heading:
            name, _, rest = heading.partition(" — `")
            route_path, _, _ = rest.partition("`")
        else:
            name, route_path = heading, ""
        slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")[:40]
        routes.append(
            {
                "num": num,
                "name": name.strip(),
                "route": route_path.strip(),
                "slug": slug,
                "prompt": prompt,
            }
        )
    return dark, light, routes


def render_one(client: OpenAI, full_prompt: str, out_path: Path, *, size: str, quality: str) -> Path:
    r = client.images.generate(
        model="gpt-image-2",
        prompt=full_prompt,
        size=size,
        quality=quality,
        n=1,
        moderation="low",
    )
    item = r.data[0]
    b64 = getattr(item, "b64_json", None)
    if b64:
        raw = base64.b64decode(b64)
    else:
        url = getattr(item, "url", None)
        if not url:
            raise RuntimeError("response item has neither b64_json nor url")
        with urllib.request.urlopen(url, timeout=300) as resp:
            raw = resp.read()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(raw)
    return out_path


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("filter", nargs="?", default=None, help="Route number or slug substring")
    p.add_argument("--list", action="store_true", help="Enumerate without rendering")
    p.add_argument("--dark", action="store_true", help="Only dark mode")
    p.add_argument("--light", action="store_true", help="Only light mode")
    p.add_argument("--size", default="2048x1152", help="Image size (default 2048x1152 = true 16:9 landscape, matches the prompts' stated aspect)")
    p.add_argument("--quality", default="high", choices=["low", "medium", "high", "auto"])
    p.add_argument("--force", action="store_true", help="Re-render even if file exists")
    args = p.parse_args()

    text = PROMPTS_FILE.read_text()
    dark, light, routes = parse_prompts(text)
    if not dark or not light:
        print("error: could not find both DARK and LIGHT design system blocks", file=sys.stderr)
        return 1
    if not routes:
        print("error: no route prompts found", file=sys.stderr)
        return 1

    if args.dark and not args.light:
        modes = [("dark", dark)]
    elif args.light and not args.dark:
        modes = [("light", light)]
    else:
        modes = [("dark", dark), ("light", light)]

    selected = routes
    if args.filter:
        f = args.filter.lower()
        selected = [r for r in routes if f == str(r["num"]) or f in r["slug"]]
        if not selected:
            print(f"error: no routes match filter '{args.filter}'", file=sys.stderr)
            for r in routes:
                print(f"  {r['num']:2d}. {r['slug']}  ({r['route'] or 'no route path'})")
            return 1

    pairs = [(r, mode, ds) for r in selected for (mode, ds) in modes]
    if args.list:
        print(f"{len(pairs)} renders ({len(selected)} routes × {len(modes)} mode(s))")
        for r, mode, _ in pairs:
            out = OUT_DIR / f"{r['num']:02d}-{r['slug']}-folio-{mode}.png"
            status = "EXISTS" if out.exists() else "would render"
            print(f"  [{status}] {out.name}  — {r['route'] or 'no route'}")
        return 0

    load_dotenv(Path.cwd() / ".env", override=False)
    load_dotenv(Path.home() / ".env", override=False)
    if not os.environ.get("OPENAI_API_KEY"):
        print(
            "error: OPENAI_API_KEY not set. Export it, or add to ./.env or ~/.env.",
            file=sys.stderr,
        )
        return 2

    client = OpenAI()
    print(
        f"Rendering {len(pairs)} images at {args.size} {args.quality}-quality "
        f"({len(selected)} routes × {len(modes)} mode(s))",
        flush=True,
    )

    failures = 0
    for i, (r, mode, ds) in enumerate(pairs, start=1):
        out = OUT_DIR / f"{r['num']:02d}-{r['slug']}-folio-{mode}.png"
        if out.exists() and not args.force:
            print(f"  [{i:2d}/{len(pairs)}] skip (exists): {out.name}")
            continue
        print(f"  [{i:2d}/{len(pairs)}] {out.name}  ...", flush=True)
        full = ds + "\n\n" + r["prompt"]
        try:
            render_one(client, full, out, size=args.size, quality=args.quality)
            print(f"             saved {out.stat().st_size // 1024}KB")
        except APIError as e:
            print(f"             API ERROR: {type(e).__name__}: {e}", file=sys.stderr)
            failures += 1
        except Exception as e:
            print(f"             ERROR: {type(e).__name__}: {e}", file=sys.stderr)
            failures += 1

    if failures:
        print(f"\ndone with {failures} failure(s)", file=sys.stderr)
        return 1
    print("\nall renders complete")
    return 0


if __name__ == "__main__":
    sys.exit(main())
