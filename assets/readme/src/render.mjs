// Renders the README images: node assets/readme/src/render.mjs [name ...]
// Needs Playwright with a Chromium it can launch (the one `playwright-cli` installs works).
// Output: assets/readme/<name>-<light|dark>.png at deviceScaleFactor 2 (the config-file form does not
// take effect, newContext does). icon.png is made separately:
//   sips -s format png -z 256 256 rs/bundle/AppIcon.icns --out assets/readme/icon.png
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const out = path.resolve(here, '..');
const require = createRequire(process.env.PLAYWRIGHT_FROM ?? '/opt/homebrew/lib/node_modules/@playwright/cli/');
const { chromium } = require('playwright-core');

const pages = {
  hero: { w: 840, h: 480 },
  'story-airpods': { w: 280, h: 300 },
  'story-mic': { w: 280, h: 300 },
  'story-volume': { w: 280, h: 300 },
};
const sprite = readFileSync(path.join(here, 'icons.svg'), 'utf8');
const wanted = process.argv.slice(2);
const names = wanted.length ? wanted : Object.keys(pages);

const browser = await chromium.launch({ args: ['--allow-file-access-from-files'] });
for (const name of names) {
  const { w, h } = pages[name];
  for (const scheme of ['light', 'dark']) {
    const ctx = await browser.newContext({ viewport: { width: w, height: h }, deviceScaleFactor: 2, colorScheme: scheme });
    const page = await ctx.newPage();
    await page.goto('file://' + path.join(here, name + '.html'));
    await page.evaluate((s) => document.body.insertAdjacentHTML('afterbegin', s), sprite);
    await page.evaluate(() => document.fonts.ready);
    const file = path.join(out, `${name}-${scheme}.png`);
    await page.screenshot({ path: file, omitBackground: name.startsWith('story') }); // stories stay transparent so they sit on either GitHub theme
    // Lossless re-pack (ImageMagick, optional): a few percent smaller than Chromium's encoder.
    execFileSync('magick', [file, '-strip', '-define', 'png:compression-level=9', '-define', 'png:compression-filter=5', file]);
    console.log('wrote', file);
    await ctx.close();
  }
}
await browser.close();
