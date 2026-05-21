import { describe, expect, it } from "vitest";
import { extractToc } from "./extractToc";

describe("extractToc", () => {
  it("returns H2 and H3 headings with github-slug ids", () => {
    const md = [
      "# Top-level page",
      "",
      "Intro paragraph.",
      "",
      "## What it does",
      "",
      "Body.",
      "",
      "### Sub-heading one",
      "",
      "More body.",
      "",
      "## Safety model",
    ].join("\n");

    expect(extractToc(md)).toEqual([
      { id: "what-it-does", text: "What it does", level: 2 },
      { id: "sub-heading-one", text: "Sub-heading one", level: 3 },
      { id: "safety-model", text: "Safety model", level: 2 },
    ]);
  });

  it("ignores H1, H4+ and headings inside fenced code blocks", () => {
    const md = [
      "## Real heading",
      "",
      "```bash",
      "## Not a heading",
      "```",
      "",
      "#### Too deep",
      "",
      "## Another real heading",
    ].join("\n");

    expect(extractToc(md).map((t) => t.text)).toEqual([
      "Real heading",
      "Another real heading",
    ]);
  });

  it("disambiguates duplicate slugs github-slugger style", () => {
    const md = ["## Setup", "## Setup", "## Setup"].join("\n");
    expect(extractToc(md).map((t) => t.id)).toEqual([
      "setup",
      "setup-1",
      "setup-2",
    ]);
  });
});
