#!/usr/bin/env node
// Draw the README cover: the wordmark in straight strokes beside the raven
// from docs/images/raven.svg, recoloured. Two variants, for the light and the
// dark GitHub theme: ink on parchment (cover) and parchment on ink
// (cover-dark). Writes docs/images/<name>.svg and renders <name>.png
// (2560x1280) with Chrome; CHROME overrides the browser path.
//
// Usage: node scripts/cover.mjs

import { readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const IMAGES = join(ROOT, 'docs', 'images');
const CHROME =
  process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';

const W = 1280, H = 640;
const PAPER = '#ece3cf';
const INK = '#1d1a17';

const VARIANTS = [
  { name: 'cover', background: PAPER, mark: INK },
  { name: 'cover-dark', background: INK, mark: PAPER },
];

// MUNIN in straight strokes: letters `h` high, strokes `sw` wide.
function wordmark(x0, top, h, sw, color) {
  const k = h / 160;
  const b = top + h, mid = top + 90 * k;
  const stroke = (d) =>
    `<path d="${d}" fill="none" stroke="${color}" stroke-width="${sw}" stroke-linejoin="miter" stroke-miterlimit="3"/>`;
  const paths = [];
  let x = x0;
  paths.push(stroke(`M${x},${b} V${top} L${x + 60 * k},${mid} L${x + 120 * k},${top} V${b}`));
  x += 170 * k;
  paths.push(stroke(`M${x},${top} V${b - 30 * k} L${x + 20 * k},${b} H${x + 80 * k} L${x + 100 * k},${b - 30 * k} V${top}`));
  x += 150 * k;
  paths.push(stroke(`M${x},${b} V${top} L${x + 100 * k},${b} V${top}`));
  x += 150 * k;
  paths.push(stroke(`M${x},${top} V${b}`));
  x += 50 * k;
  paths.push(stroke(`M${x},${b} V${top} L${x + 100 * k},${b} V${top}`));
  return paths.join('\n');
}

// The raven as a nested <svg>, so its own viewBox does the scaling.
function raven(x, y, width, color) {
  const source = readFileSync(join(IMAGES, 'raven.svg'), 'utf8');
  const viewBox = source.match(/viewBox="([^"]+)"/)[1];
  const [, , vw, vh] = viewBox.split(/\s+/).map(Number);
  const inner = source
    .replace(/^[\s\S]*?<svg[^>]*>/, '')
    .replace(/<\/svg>\s*$/, '')
    .replace(/fill="#[0-9a-fA-F]{6}"/g, `fill="${color}"`)
    .trim();
  const height = (width * vh) / vw;
  return `<svg x="${x}" y="${y}" width="${width}" height="${height.toFixed(1)}" viewBox="${viewBox}">
${inner}
</svg>`;
}

function cover({ background, mark }) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">
<rect width="${W}" height="${H}" fill="${background}"/>
${wordmark(180, 272, 96, 14, mark)}
${raven(650, 205, 470, mark)}
</svg>
`;
}

const profile = mkdtempSync(join(tmpdir(), 'munin-cover-'));
try {
  for (const variant of VARIANTS) {
    const svgPath = join(IMAGES, `${variant.name}.svg`);
    writeFileSync(svgPath, cover(variant));
    execFileSync(CHROME, [
      '--headless', '--disable-gpu', '--hide-scrollbars',
      `--user-data-dir=${profile}`,
      `--window-size=${W},${H}`,
      '--force-device-scale-factor=2',
      `--screenshot=${join(IMAGES, `${variant.name}.png`)}`,
      `file://${svgPath}`,
    ], { stdio: 'ignore' });
    console.log(`wrote docs/images/${variant.name}.svg and .png`);
  }
} finally {
  rmSync(profile, { recursive: true, force: true });
}
