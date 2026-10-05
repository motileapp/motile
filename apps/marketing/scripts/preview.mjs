// Draws the link preview image of both web projects from the /preview/ page. Run it with
// `pnpm --filter motile-marketing preview-image`.
import { writeFileSync } from "node:fs"
import { dev } from "astro"
import { chromium } from "playwright"

const WEB_FOLDERS = ["../public", "../../web/public"]

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
  const image = await page.screenshot()
  for (const folder of WEB_FOLDERS) {
    writeFileSync(new URL(`${folder}/preview.png`, import.meta.url), image)
  }
} finally {
  await browser.close()
  await server.stop()
}
