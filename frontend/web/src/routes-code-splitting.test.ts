import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const routesSource = readFileSync(join(process.cwd(), "src/routes.tsx"), "utf8");
const layoutSource = readFileSync(join(process.cwd(), "src/components/shell/Layout.tsx"), "utf8");

describe("dashboard code splitting", () => {
  it("loads route modules through dynamic imports instead of the main bundle", () => {
    expect(routesSource).toContain("lazy(");
    expect(routesSource).toContain('import("./routes/agents-flywheel")');
    expect(routesSource).toContain('import("./routes/eval-runs-detail")');
    expect(routesSource).toContain('import("./routes/scenarios-detail")');
    expect(routesSource).toContain('import("./routes/live")');
    expect(routesSource).not.toMatch(/^import \{ .*Route.* \} from "\.\/routes\//m);
  });

  it("keeps the chat rail markdown stack out of the initial shell chunk", () => {
    expect(layoutSource).toContain("lazy(");
    expect(layoutSource).toContain('import("./ChatRail")');
    expect(layoutSource).not.toContain('import { ChatRail } from "./ChatRail"');
  });
});
