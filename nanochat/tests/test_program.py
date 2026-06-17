# nanochat/tests/test_program.py
"""Verify xvision_program.md exists and is valid markdown."""
from __future__ import annotations

import re
from pathlib import Path

PROGRAM_PATH = Path(__file__).parent.parent / "xvision_program.md"


def test_program_md_exists():
    assert PROGRAM_PATH.exists(), (
        f"xvision_program.md not found at {PROGRAM_PATH}. "
        "This file must exist — it is the operator-editable agent instruction doc."
    )


def test_program_md_is_nonempty():
    content = PROGRAM_PATH.read_text()
    assert len(content.strip()) > 200, (
        "xvision_program.md is suspiciously short — ensure it contains real instructions."
    )


def test_program_md_has_required_sections():
    """The program doc must contain the mandatory structural sections."""
    content = PROGRAM_PATH.read_text()
    required_headings = [
        "Goal",
        "XVN_RESULT",
        "Forbidden",
        "Experiment loop",
    ]
    for heading in required_headings:
        assert heading in content, (
            f"xvision_program.md is missing a required section containing '{heading}'. "
            f"The autoresearcher agent needs this to know what it may and must not change."
        )


def test_program_md_no_broken_heading_levels():
    """Headings must not skip levels (h1 directly to h3 without h2 is confusing for agents)."""
    content = PROGRAM_PATH.read_text()
    lines = content.splitlines()
    heading_levels = [
        len(line) - len(line.lstrip("#"))
        for line in lines
        if re.match(r"^#{1,6} ", line)
    ]
    for i in range(1, len(heading_levels)):
        diff = heading_levels[i] - heading_levels[i - 1]
        assert diff <= 1, (
            f"Heading level jumps by {diff} at index {i} — "
            f"check for skipped levels in xvision_program.md (levels: {heading_levels})"
        )
