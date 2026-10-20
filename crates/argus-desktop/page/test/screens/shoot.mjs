// Screenshots of every screen the window shows: each scene in scenes.json
// (written by `cargo test -p argus-desktop write_screens -- --ignored`) is
// played into the real page and captured, at each size asked for.
//
//   node test/screens/shoot.mjs [--scenes file] [--out dir] [--only a,b]
//                               [--sizes 1440x900,1024x720] [--live http://127.0.0.1:port]

import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { here, load, open, option, scenes, screenshot } from './browser.mjs';

const out = resolve(option('out', join(here, 'out')));
const sizes = option('sizes', '1440x900')
  .split(',')
  .map((size) => size.split('x').map(Number));

mkdirSync(out, { recursive: true });
const { tab, origin, live, close } = await open();
const played = scenes();
try {
  for (const scene of played) {
    for (const size of sizes) {
      await load(tab, origin, scene, size, live);
      const suffix = sizes.length > 1 ? `@${size[0]}x${size[1]}` : '';
      writeFileSync(join(out, `${scene.name}${suffix}.png`), await screenshot(tab));
    }
    console.log(`${scene.name}  ${scene.title}`);
  }
} finally {
  close();
}
console.log(`${played.length} scene(s) written to ${out}`);
