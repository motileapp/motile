#!/usr/bin/env node
// Writes crates/core/src/render/openui/tree/lucide.rs: every name open-intelligent-ui's Icon takes
// (lucide-react's icons and their aliases) with its character in the clients' lucide.ttf. The
// codepoints are lucide-static's, the aliases lucide-react's lazy loader's; a name whose character
// the font lacks is left out, so it draws nothing, as an unknown name does.
//
//   node scripts/lucide-names.mjs

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const VERSION = "0.562.0";
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const font = join(root, "packages/apple/Sources/MotileKit/Fonts/lucide.ttf");
const out = join(root, "crates/core/src/render/openui/tree/lucide.rs");

async function fetchText(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return response.text();
}

/** The characters a TrueType font has, from its format 4 cmap. */
function characters(path) {
  const data = readFileSync(path);
  const tables = data.readUInt16BE(4);
  let cmap = -1;
  for (let i = 0; i < tables; i++) {
    const at = 12 + i * 16;
    if (data.toString("latin1", at, at + 4) === "cmap") cmap = data.readUInt32BE(at + 8);
  }
  if (cmap < 0) throw new Error("no cmap");
  const found = new Set();
  const subtables = data.readUInt16BE(cmap + 2);
  for (let i = 0; i < subtables; i++) {
    const at = cmap + data.readUInt32BE(cmap + 4 + i * 8 + 4);
    if (data.readUInt16BE(at) !== 4) continue;
    const segments = data.readUInt16BE(at + 6) / 2;
    const ends = at + 14;
    const starts = ends + segments * 2 + 2;
    const deltas = starts + segments * 2;
    const offsets = deltas + segments * 2;
    for (let s = 0; s < segments; s++) {
      const end = data.readUInt16BE(ends + s * 2);
      const start = data.readUInt16BE(starts + s * 2);
      const delta = data.readInt16BE(deltas + s * 2);
      const offset = data.readUInt16BE(offsets + s * 2);
      for (let code = start; code <= end && code !== 0xffff; code++) {
        const glyph = offset
          ? data.readUInt16BE(offsets + s * 2 + offset + (code - start) * 2)
          : (code + delta) & 0xffff;
        if (glyph) found.add(code);
      }
    }
  }
  return found;
}

const info = JSON.parse(await fetchText(`https://cdn.jsdelivr.net/npm/lucide-static@${VERSION}/font/info.json`));
const loader = await fetchText(`https://cdn.jsdelivr.net/npm/lucide-react@${VERSION}/dist/esm/dynamicIconImports.js`);
const inFont = characters(font);

const codepoints = new Map(Object.entries(info).map(([name, entry]) => [name, parseInt(entry.encodedCode.slice(1), 16)]));
const names = [...loader.matchAll(/"([a-z0-9-]+)": \(\) => import\('\.\/icons\/([a-z0-9-]+)\.js'\)/g)].map(([, name, icon]) => [name, icon]);
if (names.length < codepoints.size) throw new Error("lucide-react's names weren't found");

const rows = names
  .filter(([, icon]) => inFont.has(codepoints.get(icon)))
  .sort(([a], [b]) => (a < b ? -1 : 1))
  .map(([name, icon]) => `    ("${name}", "${icon}", '\\u{${codepoints.get(icon).toString(16)}}'),`);

writeFileSync(
  out,
  `//! Written by \`scripts/lucide-names.mjs\` from lucide-static and lucide-react ${VERSION}: do not edit.

/// Every name and alias, sorted, with the icon's own name and its character in \`lucide.ttf\`.
pub const NAMES: &[(&str, &str, char)] = &[
${rows.join("\n")}
];
`,
);
console.log(`${rows.length} names, ${names.length - rows.length} without a character in the font`);
