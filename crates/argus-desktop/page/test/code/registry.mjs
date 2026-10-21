// Checks every grammar Argus pins, over the network: each file downloads
// with the SHA-256 `known.ts` records, the parser loads in this runtime, and
// its highlight queries compile. Not part of `npm test`, which runs offline;
// run it after changing a pin or the runtime, after `npm run build`:
//
//   node test/code/registry.mjs

import { createHash } from 'node:crypto';
import { Language, Parser, Query } from '../../../assets/js/vendor/codemirror.js';
import { KNOWN } from '../../../assets/js/code/grammar/known.js';

await Parser.init();
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
let failed = 0;

async function get(file) {
  const bytes = new Uint8Array(await (await fetch(file.url)).arrayBuffer());
  if (sha(bytes) !== file.sha256) throw new Error(`${file.url}: not the file pinned`);
  return bytes;
}

// One at a time, so a failure is said beside the grammar it is of.
for (const known of KNOWN) {
  try {
    // oxlint-disable-next-line no-await-in-loop
    const language = await Language.load(await get(known.wasm));
    // oxlint-disable-next-line no-await-in-loop
    const queries = await Promise.all(known.queries.map(get));
    new Query(language, queries.map((q) => new TextDecoder().decode(q)).join('\n')).delete();
    console.log(`ok    ${known.name.padEnd(12)} ${known.package}`);
  } catch (e) {
    failed++;
    console.log(`FAIL  ${known.name.padEnd(12)} ${e.message}`);
  }
}
process.exitCode = failed ? 1 : 0;
