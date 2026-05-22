import asyncio
import os
from playwright.async_api import async_playwright

OUTPUT_DIR = "/home/agents/xvision/docs/design/trading-charts"

# Theme-specific URLs
TARGETS = [
    ("shadcn-stripe-design", "https://www.shadcn.io/design/stripe"),
    ("shadcn-wise-design", "https://www.shadcn.io/design/wise"),
    ("bydfi-color-schemes", "https://www.bydfi.com/en/questions/what-are-the-best-color-schemes-for-tradingview-charts-in-the-cryptocurrency-industry"),
    ("databrain-fintech-viz", "https://www.usedatabrain.com/blog/fintech-data-visualization"),
    ("pixel-show-density", "https://pixel-show.com/blog/designing-data-dense-dashboards"),
    ("lightweight-charts-custom", "https://tradingview.github.io/lightweight-charts/tutorials/customization/chart-colors"),
    ("pineify-candle-colors", "https://pineify.app/resources/blog/how-to-change-candlestick-color-on-tradingview-a-complete-guide"),
    ("penpot-tokens", "https://penpot.app/blog/the-developers-guide-to-design-tokens-and-css-variables/"),
]

async def capture(page, name, url):
    path = os.path.join(OUTPUT_DIR, f"{name}.png")
    try:
        await page.goto(url, wait_until="domcontentloaded", timeout=30000)
        await page.wait_for_timeout(3000)
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
