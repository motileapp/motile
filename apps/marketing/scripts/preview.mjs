// Draws the link preview image from the /preview/ page. Run it with
// `pnpm --filter motile-marketing preview-image`.
import { dev } from "astro"
import { chromium } from "playwright"

const server = await dev({
  root: new URL("..", import.meta.url),
  logLevel: "error",
  devToolbar: { enabled: false },
})
const browser = await chromium.launch()
try {
  const page = await browser.newPage({
    viewport: { width: 1200, height: 630 },
    deviceScaleFactor: 2,
    reducedMotion: "reduce",
  })
  await page.goto(`http://localhost:${server.address.port}/preview/`)
  await page.evaluate(() => document.fonts.ready)
  await page.screenshot({
    path: new URL("../public/preview.png", import.meta.url).pathname,
  })
} finally {
  await browser.close()
  await server.stop()
}
