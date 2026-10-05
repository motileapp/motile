// Draws the favicons, touch and home-screen icons of the marketing site and the web app from the
// mark. Run it with `pnpm --filter motile-marketing icons`.
import { writeFileSync } from "node:fs"
import sharp from "sharp"

const MARK =
  "M18.477 1.929c1.107.2 1.988 1.241 1.701 2.468-.281 1.205.603 2.372 1.883 2.488 1.863.167 2.616 2.39 1.212 3.577a1.994 1.994 0 0 0 0 3.075c1.404 1.186.651 3.41-1.212 3.577-1.28.116-2.164 1.284-1.883 2.488.41 1.752-1.56 3.126-3.17 2.212-1.107-.63-2.538-.184-3.048.95-.508 1.128-1.81 1.484-2.818 1.068-.934-.385-.922-1.601-.598-2.559L16.52 3.63c.315-.929.993-1.874 1.958-1.7zm-8.437-.693C10.547.108 11.849-.248 12.857.168c.934.385.922 1.6.598 2.558L7.48 20.37c-.314.929-.992 1.874-1.957 1.7-1.107-.199-1.988-1.241-1.701-2.469.281-1.204-.603-2.372-1.884-2.488-1.863-.167-2.615-2.39-1.21-3.577a1.995 1.995 0 0 0 0-3.075c-1.405-1.187-.653-3.41 1.21-3.577 1.28-.116 2.165-1.283 1.884-2.488-.41-1.751 1.56-3.125 3.17-2.21 1.106.629 2.538.182 3.048-.95z"
const BLACK = "#0a0b0f"
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
const MARK_IN_APPLE_TILE = 0.68

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
