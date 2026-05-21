import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { CardHeader } from "./Card";

describe("CardHeader", () => {
  it("renders falsy but valid action content", () => {
    render(<CardHeader title="Risk" actions={0} />);

    expect(screen.getByText("0")).toBeInTheDocument();
  });
});
