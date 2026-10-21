// What grammar to offer for a file, and downloading it. One Argus knows is
// pinned: each file is checked against the SHA-256 recorded in `known` and
// refused if it differs. For any other extension npm is searched for a
// `tree-sitter-…` package that ships a prebuilt parser and highlight query;
// nothing vouches for what that finds, so it is only ever installed after
// the person has read its name and said yes.

import { knownFor, type Pinned } from './known.js';
import type { Installed } from './store.js';

export interface Offer {
  name: string;
  label: string;
  /** `package@version`. */
  source: string;
  wasm: Pinned | { url: string; size: number };
  queries: (Pinned | { url: string; size: number })[];
  /** Argus pinned and hashed it; otherwise npm search found it. */
  pinned: boolean;
  /** Who published a package npm search found. */
  publisher?: string;
}

/** Extensions whose language npm names differently. */
const NAMES: Record<string, string> = {
  kt: 'kotlin',
  kts: 'kotlin',
  yml: 'yaml',
  md: 'markdown',
  ml: 'ocaml',
  mli: 'ocaml',
  erl: 'erlang',
  hrl: 'erlang',
  pl: 'perl',
  pm: 'perl',
  r: 'r',
  sql: 'sql',
  tf: 'hcl',
  hcl: 'hcl',
  nix: 'nix',
  dart: 'dart',
  svelte: 'svelte',
  vue: 'vue',
  proto: 'proto',
  dockerfile: 'dockerfile',
  mk: 'make',
  cmake: 'cmake',
  gleam: 'gleam',
  odin: 'odin',
  sol: 'solidity',
  v: 'v',
  nim: 'nim',
  clj: 'clojure',
  fs: 'fsharp',
  fsx: 'fsharp',
  hx: 'haxe',
  graphql: 'graphql',
  gql: 'graphql',
};

function languageOf(path: string): string | null {
  const name = path.slice(path.lastIndexOf('/') + 1).toLowerCase();
  if (name === 'dockerfile') return 'dockerfile';
  if (name === 'makefile') return 'make';
  const dot = name.lastIndexOf('.');
  if (dot <= 0) return null;
  const ext = name.slice(dot + 1);
  return NAMES[ext] ?? (/^[a-z][a-z0-9]{0,11}$/.test(ext) ? ext : null);
}

/** The grammar to offer for `path`: the pinned one, or failing that the
 *  first npm package that ships what a grammar needs. */
export async function offerFor(path: string): Promise<Offer | null> {
  const known = knownFor(path);
  if (known)
    return {
      name: known.name,
      label: known.label,
      source: known.package,
      wasm: known.wasm,
      queries: known.queries,
      pinned: true,
    };
  const language = languageOf(path);
  return language ? search(language) : null;
}

interface Found {
  package: { name: string; version: string; publisher?: { username?: string } };
}

async function search(language: string): Promise<Offer | null> {
  try {
    const url = `https://registry.npmjs.org/-/v1/search?text=tree-sitter-${encodeURIComponent(language)}&size=10`;
    const found = ((await (await fetch(url)).json()) as { objects: Found[] }).objects;
    // Its own name or a grammar collective's, never one that only mentions it.
    const named = new RegExp(`^(@[a-z0-9-]+/)?tree-sitter-${language}$`);
    // One at a time, in npm's order, stopping at the first that will do.
    for (const { package: p } of found.filter((f) => named.test(f.package.name))) {
      // oxlint-disable-next-line no-await-in-loop
      const listing = await fetch(
        `https://data.jsdelivr.com/v1/packages/npm/${p.name}@${p.version}?structure=flat`,
      );
      if (!listing.ok) continue;
      // oxlint-disable-next-line no-await-in-loop
      const files = ((await listing.json()) as { files: { name: string; size: number }[] }).files;
      // The parser named for the language, or the only one: a package with
      // several (YAML's ships a schema's beside it) is not guessed at.
      const parsers = files.filter((f) => /^\/[^/]+\.wasm$/.test(f.name));
      const wasm =
        parsers.find((f) => f.name === `/tree-sitter-${language}.wasm`) ??
        (parsers.length === 1 ? parsers[0] : undefined);
      const query = files.find((f) => f.name === '/queries/highlights.scm');
      if (!wasm || !query) continue;
      const base = `https://cdn.jsdelivr.net/npm/${p.name}@${p.version}`;
      return {
        name: language,
        label: language,
        source: `${p.name}@${p.version}`,
        wasm: { url: base + wasm.name, size: wasm.size },
        queries: [{ url: base + query.name, size: query.size }],
        pinned: false,
        ...(p.publisher?.username ? { publisher: p.publisher.username } : {}),
      };
    }
  } catch {
    // Offline, or npm away: nothing to offer this time.
  }
  return null;
}

async function sha256(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
}

async function get(file: Pinned | { url: string }): Promise<Uint8Array<ArrayBuffer>> {
  const answer = await fetch(file.url);
  if (!answer.ok) throw new Error(`${file.url} answered ${answer.status}`);
  const bytes = new Uint8Array(await answer.arrayBuffer());
  if ('sha256' in file && (await sha256(bytes)) !== file.sha256)
    throw new Error(`${file.url} is not the file Argus checked`);
  return bytes;
}

/** Downloads what `offer` names, each pinned file checked. */
export async function download(offer: Offer): Promise<Installed> {
  const wasm = await get(offer.wasm);
  const queries = await Promise.all(offer.queries.map(get));
  const query = queries.map((q) => new TextDecoder().decode(q)).join('\n');
  return { name: offer.name, label: offer.label, source: offer.source, wasm, query };
}
