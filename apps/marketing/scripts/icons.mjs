// Draws every icon from the mark: the favicons, touch and home-screen icons of the marketing site
// and the web app, and the macOS app icon. Run it with `pnpm --filter motile-marketing icons`.
import { writeFileSync } from "node:fs"
import sharp from "sharp"

const MARK =
  "M17.72 1.928c1.417-.251 2.809.97 2.458 2.47-.281 1.204.603 2.371 1.883 2.487 1.863.167 2.616 2.39 1.212 3.577a1.995 1.995 0 0 0 0 3.075c1.404 1.186.651 3.41-1.212 3.577-1.28.116-2.164 1.284-1.883 2.488.41 1.752-1.561 3.126-3.17 2.212-1.107-.63-2.538-.184-3.048.95-.629 1.396-2.472 1.609-3.472.64-.363-.351-.358-.907-.195-1.385l6.47-19.112c.157-.464.474-.894.956-.98zm-7.68-.692c.628-1.396 2.47-1.61 3.471-.641.363.351.358.908.196 1.386l-6.471 19.11c-.157.465-.473.896-.956.981-1.418.251-2.809-.97-2.458-2.47.281-1.204-.603-2.372-1.884-2.488-1.863-.167-2.615-2.39-1.21-3.577a1.995 1.995 0 0 0 0-3.075c-1.405-1.187-.653-3.41 1.21-3.577 1.28-.116 2.165-1.283 1.884-2.488-.41-1.751 1.56-3.125 3.17-2.21 1.106.629 2.537.182 3.048-.95z"
const BLACK = "#0a0a0a"
const WEB_FOLDERS = ["public", "../web/public"]

/** The mark, white, centred in a `canvas`-wide square and `ratio` of its width. */
function mark(canvas, ratio) {
  const size = canvas * ratio
  const offset = Number(((canvas - size) / 2).toFixed(2))
  return `<path transform="translate(${offset} ${offset}) scale(${Number((size / 24).toFixed(4))})" d="${MARK}" fill="#fff"/>`
}

const svg = (canvas, body) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${canvas} ${canvas}">${body}</svg>\n`

/** A rounded tile, for places that show the icon as it is. */
const tile = (ratio) =>
  svg(
    32,
    `<rect width="32" height="32" rx="8" fill="${BLACK}"/>${mark(32, ratio)}`
  )

/** A full square, for places that cut their own shape out of it. */
const square = (ratio) =>
  svg(32, `<rect width="32" height="32" fill="${BLACK}"/>${mark(32, ratio)}`)

// How much of a tile's width the mark takes up: a margin of 5 on each side of 32.
const MARK_IN_TILE = 22 / 32
const MARK_IN_APPLE_TILE = 0.7

// An 824-point tile in a 1024-point canvas, as Apple's icon grid has it.
const mac = svg(
  1024,
  `<defs>
    <linearGradient id="fill" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#333"/><stop offset="1" stop-color="${BLACK}"/></linearGradient>
    <linearGradient id="edge" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.22"/><stop offset="1" stop-color="#fff" stop-opacity="0.04"/></linearGradient>
  </defs>
  <rect x="100" y="100" width="824" height="824" rx="185" fill="url(#fill)"/>
  <rect x="102" y="102" width="820" height="820" rx="183" fill="none" stroke="url(#edge)" stroke-width="4"/>
  ${mark(1024, (MARK_IN_APPLE_TILE * 824) / 1024)}`
)

/** Rasterised at twice the size it is asked for, then scaled down. */
function png(source, size) {
  const canvas = Number(source.match(/viewBox="0 0 (\d+)/)[1])
  const density = (72 * 2 * size) / canvas
  return sharp(Buffer.from(source), { density })
    .resize(size, size)
    .png()
    .toBuffer()
}

const opaque = async (source, size) =>
  sharp(await png(source, size))
    .flatten({ background: BLACK })
    .png()
    .toBuffer()

/** An .ico file holding the PNGs as they are. */
function ico(images) {
  const header = Buffer.alloc(6)
  header.writeUInt16LE(1, 2)
  header.writeUInt16LE(images.length, 4)
  let offset = header.length + 16 * images.length
  const entries = images.map(({ size, data }) => {
    const entry = Buffer.alloc(16)
    entry.writeUInt8(size, 0)
    entry.writeUInt8(size, 1)
    entry.writeUInt16LE(1, 4)
    entry.writeUInt16LE(32, 6)
    entry.writeUInt32LE(data.length, 8)
    entry.writeUInt32LE(offset, 12)
    offset += data.length
    return entry
  })
  return Buffer.concat([
    header,
    ...entries,
    ...images.map((image) => image.data),
  ])
}

const icoImages = await Promise.all(
  [16, 32, 48].map(async (size) => ({
    size,
    data: await png(tile(MARK_IN_TILE), size),
  }))
)
const files = {
  "favicon.svg": tile(MARK_IN_TILE),
  "favicon.ico": ico(icoImages),
  "apple-touch-icon.png": await opaque(square(MARK_IN_APPLE_TILE), 180),
  "icon-192.png": await png(tile(MARK_IN_TILE), 192),
  "icon-512.png": await png(tile(MARK_IN_TILE), 512),
  // A launcher may crop this one to a circle 80% of its width.
  "icon-maskable-512.png": await opaque(square(0.54), 512),
}
for (const folder of WEB_FOLDERS) {
  for (const [name, data] of Object.entries(files))
    writeFileSync(`${folder}/${name}`, data)
}
writeFileSync("../macos/Resources/AppIcon.png", await png(mac, 1024))
