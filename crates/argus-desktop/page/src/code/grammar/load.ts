// A file's grammar, ready to parse with: tree-sitter's runtime started once,
// each language loaded and its query compiled once, from what is installed.

import { Language, Parser, Query } from '../../vendor/codemirror.js';
import { knownFor } from './known.js';
import { keep, installed, type Installed } from './store.js';
import { download, type Offer } from './fetch.js';

export interface Loaded {
  name: string;
  label: string;
  language: Language;
  query: Query;
}

let runtime: Promise<void> | null = null;

function start(): Promise<void> {
  runtime ??= Parser.init({
    // The runtime's own WebAssembly sits beside the bundle it came in.
    locateFile: () => new URL('../../vendor/web-tree-sitter.wasm', import.meta.url).href,
  });
  return runtime;
}

const ready = new Map<string, Promise<Loaded>>();
/** The ones loaded, for an editor to colour with in its first paint. */
const done = new Map<string, Loaded>();

function load(grammar: Installed): Promise<Loaded> {
  let loading = ready.get(grammar.name);
  if (!loading) {
    loading = start().then(async () => {
      const language = await Language.load(grammar.wasm);
      return {
        name: grammar.name,
        label: grammar.label,
        language,
        query: new Query(language, grammar.query),
      };
    });
    ready.set(grammar.name, loading);
    void loading.then(
      (loaded) => done.set(loaded.name, loaded),
      () => {},
    );
    // A grammar that failed to load may be installed again.
    loading.catch(() => ready.delete(grammar.name));
  }
  return loading;
}

/** The grammar installed for `path`, or `null` when there is none. One
 *  npm search found is kept under the language its extension names. */
export async function installedFor(path: string): Promise<Loaded | null> {
  const name = knownFor(path)?.name ?? languageKey(path);
  if (!name) return null;
  const grammar = await installed(name);
  return grammar ? load(grammar) : null;
}

/** The grammar for `path` if it is loaded already, without waiting. */
export function loadedFor(path: string): Loaded | null {
  const name = knownFor(path)?.name ?? languageKey(path);
  return name ? (done.get(name) ?? null) : null;
}

function languageKey(path: string): string | null {
  const name = path.slice(path.lastIndexOf('/') + 1).toLowerCase();
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1) : null;
}

/** Downloads, loads and keeps what `offer` names: kept only once it has
 *  loaded and its query compiled, so a grammar that will not run is never
 *  installed. */
export async function install(offer: Offer, path: string): Promise<Loaded> {
  const grammar = await download(offer);
  // One that npm search found is kept under its extension, which is what
  // the next file of the kind is looked up by.
  const name = offer.pinned ? offer.name : (languageKey(path) ?? offer.name);
  const named = { ...grammar, name };
  const loaded = await load(named);
  await keep(named);
  return loaded;
}
