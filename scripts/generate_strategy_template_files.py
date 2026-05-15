#!/usr/bin/env python3
"""Generate strategy template JSON files from the Markdown strategy backlog.

The source strategy backlog lives under repo-root `strategies/`. This script
turns each non-README Markdown strategy spec into a structured template record
under `docs/strategies/templates/` so agents and future tooling can consume the
same source material without scraping prose ad hoc.
"""

from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOT = ROOT / "strategies"
OUTPUT_ROOT = ROOT / "docs" / "strategies" / "templates"
INDEX_PATH = OUTPUT_ROOT / "index.json"
README_PATH = ROOT / "docs" / "strategies" / "README.md"

SECTION_ALIASES = {
    "thesis": "thesis",
    "inputs": "inputs",
    "parameters": "parameters",
    "decision rule": "decision_rule",
    "expected regime": "expected_regime",
    "data dependencies": "data_dependencies",
    "status": "status",
    "references": "references",
}

REGIME_KEYWORDS = [
    ("trending_bull", ("bull", "uptrend", "long trend")),
    ("trending_bear", ("bear", "downtrend", "short trend")),
    ("range_bound", ("range", "sideways", "mean-reverting")),
    ("chop", ("chop", "wandering", "noisy")),
    ("high_volatility", ("high-vol", "high volatility", "volatile", "breakout")),
    ("event_driven", ("event", "news", "nansen", "onchain", "wallet", "funding", "liquidation")),
]

TOOL_KEYWORDS = [
    ("ohlcv", ("priceframe", "ohlcv", "price.", "close", "high", "low")),
    ("indicator_panel", ("indicatorpanel", "ema", "rsi", "macd", "atr", "adx", "bollinger", "mfi")),
    ("onchain_panel", ("onchainpanel", "nansen", "wallet", "smart_money", "stablecoin", "cex")),
    ("funding_rates", ("funding", "perp")),
    ("liquidations", ("liquidation",)),
]

ARCHETYPE_BY_FAMILY = {
    "EMA": "trend_follower",
    "FibonacciStrategy": "mean_reversion",
    "bollinger": "range_trade",
    "nansen": "onchain_smart_money",
    "random": "custom",
    "rsi_volume": "mean_reversion",
}

ARCHETYPE_BY_SLUG = [
    ("breakout", "breakout"),
    ("squeeze", "breakout"),
    ("momentum", "momentum"),
    ("cross", "trend_follower"),
    ("regime", "trend_follower"),
    ("meanrev", "mean_reversion"),
    ("reversal", "mean_reversion"),
    ("fade", "mean_reversion"),
    ("scalp", "scalping"),
    ("inflow", "onchain_smart_money"),
    ("outflow", "onchain_smart_money"),
    ("smart_money", "onchain_smart_money"),
    ("cex", "onchain_smart_money"),
]


def markdown_files() -> list[Path]:
    return sorted(
        p
        for p in SOURCE_ROOT.rglob("*.md")
        if p.name != "README.md" and not any(part.startswith(".") for part in p.parts)
    )


def slug_to_display(slug: str) -> str:
    words = [word for word in re.split(r"[\s_-]+", slug) if word]
    return " ".join(word.upper() if word in {"ema", "rsi", "mfi", "bb", "cex"} else word.capitalize() for word in words)


def strip_markdown(text: str) -> str:
    text = re.sub(r"`([^`]+)`", r"\1", text)
    text = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", text)
    text = re.sub(r"\*{1,3}([^*]+)\*{1,3}", r"\1", text)
    return text.strip()


def first_paragraph(text: str) -> str:
    paragraphs = [strip_markdown(p).replace("\n", " ") for p in re.split(r"\n\s*\n", text.strip())]
    return next((p for p in paragraphs if p), "")


def parse_sections(raw: str) -> tuple[str, dict[str, str], dict[str, str]]:
    title_match = re.search(r"^#\s+(.+?)\s*$", raw, flags=re.MULTILINE)
    title = title_match.group(1).strip() if title_match else ""

    metadata: dict[str, str] = {}
    for key, value in re.findall(r"^\*\*([^:*]+):\*\*\s*(.+?)\s*$", raw, flags=re.MULTILINE):
        metadata[key.strip().lower()] = strip_markdown(value)

    sections: dict[str, str] = {}
    headings = list(re.finditer(r"^##\s+(.+?)\s*$", raw, flags=re.MULTILINE))
    for idx, heading in enumerate(headings):
        start = heading.end()
        end = headings[idx + 1].start() if idx + 1 < len(headings) else len(raw)
        name = heading.group(1).strip().lower()
        canonical = SECTION_ALIASES.get(name, re.sub(r"[^a-z0-9]+", "_", name).strip("_"))
        sections[canonical] = raw[start:end].strip()

    return title, metadata, sections


def parse_status(metadata: dict[str, str], sections: dict[str, str]) -> str:
    status = metadata.get("status") or first_paragraph(sections.get("status", ""))
    status = status.split(".")[0].split("\n")[0].strip("` ")
    return status or "idea"


def parse_parameters(section: str) -> list[dict[str, str]]:
    params: list[dict[str, str]] = []
    for line in section.splitlines():
        if "|" not in line or "---" in line or "Param" in line:
            continue
        cells = [strip_markdown(cell) for cell in line.strip().strip("|").split("|")]
        if len(cells) < 2:
            continue
        name = cells[0].strip()
        if not name:
            continue
        item = {"name": name}
        if len(cells) > 1 and cells[1]:
            item["default"] = cells[1]
        if len(cells) > 2 and cells[2]:
            item["range"] = cells[2]
        params.append(item)
    return params


def infer_regimes(text: str) -> list[str]:
    lowered = text.lower()
    regimes = [name for name, needles in REGIME_KEYWORDS if any(needle in lowered for needle in needles)]
    return regimes or ["any"]


def infer_required_tools(text: str) -> list[str]:
    lowered = text.lower()
    tools = [name for name, needles in TOOL_KEYWORDS if any(needle in lowered for needle in needles)]
    return tools or ["ohlcv", "indicator_panel"]


def infer_template_family(family: str, slug: str) -> str:
    for needle, archetype in ARCHETYPE_BY_SLUG:
        if needle in slug:
            return archetype
    return ARCHETYPE_BY_FAMILY.get(family, "custom")


def build_prompt(display_name: str, sections: dict[str, str]) -> str:
    thesis = first_paragraph(sections.get("thesis", ""))
    inputs = sections.get("inputs", "").strip()
    decision_rule = sections.get("decision_rule", "").strip()
    expected_regime = first_paragraph(sections.get("expected_regime", ""))
    data_dependencies = sections.get("data_dependencies", "").strip()
    parts = [
        f"You are trading the `{display_name}` strategy.",
        "",
        "Thesis:",
        thesis,
        "",
        "Inputs:",
        inputs,
        "",
        "Decision rule:",
        decision_rule,
        "",
        "Expected regime:",
        expected_regime,
        "",
        "Data dependencies:",
        data_dependencies or "None beyond the provided market data.",
        "",
        "Return JSON only: {\"action\":\"long_open|short_open|flat|hold\","
        "\"conviction\":0.0,\"justification\":\"one concise reason\"}.",
    ]
    return "\n".join(part for part in parts if part is not None).strip() + "\n"


def convert(path: Path) -> dict:
    raw = path.read_text(encoding="utf-8")
    title, metadata, sections = parse_sections(raw)
    slug = path.stem
    family = path.parent.relative_to(SOURCE_ROOT).as_posix()
    display_name = slug_to_display(title or slug)
    combined_text = "\n".join(
        [
            sections.get("inputs", ""),
            sections.get("expected_regime", ""),
            sections.get("data_dependencies", ""),
            slug,
            family,
        ]
    )

    template = {
        "schema_version": "xvision.strategy_template.v1",
        "name": slug,
        "display_name": display_name,
        "source_doc": path.relative_to(ROOT).as_posix(),
        "family": family,
        "status": parse_status(metadata, sections),
        "base_template": infer_template_family(family, slug),
        "plain_summary": first_paragraph(sections.get("thesis", "")),
        "regime_fit": infer_regimes(combined_text),
        "required_tools": infer_required_tools(combined_text),
        "mechanical_params": parse_parameters(sections.get("parameters", "")),
        "sections": {
            key: sections.get(key, "").strip()
            for key in [
                "thesis",
                "inputs",
                "parameters",
                "decision_rule",
                "expected_regime",
                "data_dependencies",
                "status",
                "references",
            ]
            if sections.get(key, "").strip()
        },
        "trader_prompt": build_prompt(display_name, sections),
    }
    return template


def write_readme(count: int) -> None:
    README_PATH.write_text(
        "\n".join(
            [
                "# Strategy Template Files",
                "",
                "Generated from the Markdown strategy backlog in `strategies/`.",
                "",
                "Run:",
                "",
                "```sh",
                "python3 scripts/generate_strategy_template_files.py",
                "```",
                "",
                "The generated files live in `docs/strategies/templates/` and use",
                "`schema_version: xvision.strategy_template.v1`. The source Markdown",
                "files remain the editable strategy backlog.",
                "",
                f"Generated template count: {count}.",
                "",
            ]
        ),
        encoding="utf-8",
    )


def main() -> None:
    OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)

    templates = [convert(path) for path in markdown_files()]
    written_paths: set[Path] = set()
    for template in templates:
        family_dir = OUTPUT_ROOT / template["family"]
        family_dir.mkdir(parents=True, exist_ok=True)
        out_path = family_dir / f"{template['name']}.json"
        out_path.write_text(json.dumps(template, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        written_paths.add(out_path)

    index = {
        "schema_version": "xvision.strategy_template_index.v1",
        "source_root": SOURCE_ROOT.relative_to(ROOT).as_posix(),
        "template_count": len(templates),
        "templates": [
            {
                "name": template["name"],
                "display_name": template["display_name"],
                "family": template["family"],
                "base_template": template["base_template"],
                "status": template["status"],
                "path": (OUTPUT_ROOT / template["family"] / f"{template['name']}.json")
                .relative_to(ROOT)
                .as_posix(),
                "source_doc": template["source_doc"],
            }
            for template in templates
        ],
    }
    INDEX_PATH.write_text(json.dumps(index, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    written_paths.add(INDEX_PATH)
    write_readme(len(templates))
    written_paths.add(README_PATH)

    for existing in OUTPUT_ROOT.rglob("*.json"):
        if existing not in written_paths:
            existing.unlink()

    print(f"Wrote {len(templates)} strategy templates to {OUTPUT_ROOT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
