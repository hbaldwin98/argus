// The grammars installed in this window, kept in the webview's IndexedDB.
// The webview keeps its storage under the instance's config directory, so
// a dev window has its own, and a grammar is downloaded once per instance.
// Storage that will not open — a private profile, a full disk — leaves
// grammars to last only as long as the window.

export interface Installed {
  name: string;
  label: string;
  /** Where it came from, `package@version`, for the person to read. */
  source: string;
  wasm: Uint8Array;
  query: string;
}

const DB = 'argus-grammars';
const STORE = 'grammars';

let db: Promise<IDBDatabase | null> | null = null;

function open(): Promise<IDBDatabase | null> {
  db ??= new Promise((done) => {
    try {
      const asked = indexedDB.open(DB, 1);
      asked.addEventListener('upgradeneeded', () => {
        asked.result.createObjectStore(STORE, { keyPath: 'name' });
      });
      asked.addEventListener('success', () => done(asked.result));
      asked.addEventListener('error', () => done(null));
    } catch {
      done(null);
    }
  });
  return db;
}

const memory = new Map<string, Installed>();

export async function installed(name: string): Promise<Installed | null> {
  const kept = memory.get(name);
  if (kept) return kept;
  const store = await open();
  if (!store) return null;
  return new Promise((done) => {
    const asked = store.transaction(STORE).objectStore(STORE).get(name);
    asked.addEventListener('success', () => done((asked.result as Installed | undefined) ?? null));
    asked.addEventListener('error', () => done(null));
  });
}

export async function keep(grammar: Installed): Promise<void> {
  memory.set(grammar.name, grammar);
  const store = await open();
  if (!store) return;
  await new Promise<void>((done) => {
    const tx = store.transaction(STORE, 'readwrite');
    tx.objectStore(STORE).put(grammar);
    tx.addEventListener('complete', () => done());
    tx.addEventListener('error', () => done());
  });
}
