#!/usr/bin/env node
// Draw the README cover: the logo from docs/images/logo.svg (lettering and
// raven), recoloured and centred. Two variants, for the light and the dark
// GitHub theme: ink on parchment (cover) and parchment on ink (cover-dark).
// Writes docs/images/<name>.svg and renders <name>.png (2560x1280) with
// Chrome; CHROME overrides the browser path.
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

// The logo as a nested <svg>, centred, `width` wide; its own viewBox does
// the scaling.
function logo(width, color) {
  const source = readFileSync(join(IMAGES, 'logo.svg'), 'utf8');
  const viewBox = source.match(/viewBox="([^"]+)"/)[1];
  const [, , vw, vh] = viewBox.split(/\s+/).map(Number);
  const inner = source
    .replace(/^[\s\S]*?<svg[^>]*>/, '')
    .replace(/<\/svg>\s*$/, '')
    .replace(/fill="currentColor"/g, `fill="${color}"`)
    .trim();
  const height = (width * vh) / vw;
  const x = (W - width) / 2, y = (H - height) / 2;
  return `<svg x="${x}" y="${y.toFixed(1)}" width="${width}" height="${height.toFixed(1)}" viewBox="${viewBox}">
${inner}
</svg>`;
}

function cover({ background, mark }) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">
<rect width="${W}" height="${H}" fill="${background}"/>
${logo(880, mark)}
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
