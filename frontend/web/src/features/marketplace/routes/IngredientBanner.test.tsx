// src/features/marketplace/routes/IngredientBanner.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { IngredientBanner } from "./IngredientBanner";
import type { Ingredient } from "@/features/marketplace/data/types";

const INGREDIENTS: Ingredient[] = [
  { name: "Claude Haiku 4.5", kind: "model", installed: true },
  { name: "Birdeye MCP", kind: "mcp", installed: false },
  { name: "SOL Strategist", kind: "skill", installed: false },
  { name: "Mantlescan MCP", kind: "mcp", installed: true },
];

describe("IngredientBanner", () => {
  it("renders when ingredients are missing", () => {
    render(<IngredientBanner ingredients={INGREDIENTS} />);
    expect(screen.getByTestId("ingredient-banner")).toBeInTheDocument();
  });

  it("shows the missing count in the copy", () => {
    render(<IngredientBanner ingredients={INGREDIENTS} />);
    expect(screen.getByText(/2 of 4/)).toBeInTheDocument();
  });

  it("renders all ingredient pills", () => {
    render(<IngredientBanner ingredients={INGREDIENTS} />);
    expect(screen.getByText("Claude Haiku 4.5")).toBeInTheDocument();
    expect(screen.getByText("Birdeye MCP")).toBeInTheDocument();
  });

  it("each pill carries a kind label", () => {
    render(<IngredientBanner ingredients={INGREDIENTS} />);
    expect(screen.getAllByText("MODEL").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("MCP").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("SKILL")).toBeInTheDocument();
  });

  it("does NOT render when all ingredients are installed", () => {
    const allInstalled = INGREDIENTS.map((i) => ({ ...i, installed: true }));
    render(<IngredientBanner ingredients={allInstalled} />);
    expect(screen.queryByTestId("ingredient-banner")).not.toBeInTheDocument();
  });

  it("shows the Install missing CTA", () => {
    render(<IngredientBanner ingredients={INGREDIENTS} />);
    expect(screen.getByRole("button", { name: /install missing/i })).toBeInTheDocument();
  });
});
