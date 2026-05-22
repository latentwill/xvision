import asyncio
import os
from playwright.async_api import async_playwright

OUTPUT_DIR = "/home/agents/xvision/docs/design/trading-charts"

# URLs that are likely to render chart visuals without login
TARGETS = [
    ("tradingview-case-study", "https://rondesignlab.com/cases/tradingview-platform-for-traders"),
    ("pixel-show-data-dense", "https://pixel-show.com/blog/designing-data-dense-dashboards"),
    ("muzli-crypto-bot", "https://me.muz.li/anuwarux/ai-crypto-trading-bot-dashboard-design"),
    ("devoq-trading-platform", "https://www.devoq.io/fintech-trading-platform-ui-design-creating-high-converting-prop-trading-funded-account-experiences/"),
    ("usedatabrain-chart-libs", "https://www.usedatabrain.com/blog/javascript-chart-libraries"),
    ("oldschool-engineer-dashboard", "https://oldschool-engineer.dev/side%20projects/2026/04/29/dashboard-looks-like-a-real-trading-tool.html"),
    ("cssscript-lightweight", "https://www.cssscript.com/financial-chart/"),
    ("lightweight-charts-colors", "https://tradingview.github.io/lightweight-charts/tutorials/customization/chart-colors"),
    ("lightweight-charts-mtf", "https://nsulistiyawan.github.io/lightweight-charts-mtf/"),
    ("mansknow-finaura", "https://mansknow.com/finaura-premium-fintech-dashboard-ui-kit/"),
    ("dribbble-trading", "https://dribbble.com/search/trading-dashboard-design"),
    ("dribbble-dark-crypto", "https://dribbble.com/search/dark-crypto-dashboard"),
    ("figma-crypto-dashboard", "https://www.figma.com/community/file/1314709098687178801/crypto-trading-dashboard-dark-light-mode"),
]

async def capture(page, name, url):
    path = os.path.join(OUTPUT_DIR, f"{name}.png")
    try:
        await page.goto(url, wait_until="networkidle", timeout=60000)
        await page.screenshot(path=path, full_page=True)
        print(f"✅ {name}: {path}")
    except Exception as e:
        print(f"❌ {name}: {e}")

async def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True)
        context = await browser.new_context(viewport={"width": 1440, "height": 900})
        page = await context.new_page()
        for name, url in TARGETS:
            await capture(page, name, url)
        await browser.close()

if __name__ == "__main__":
    asyncio.run(main())
