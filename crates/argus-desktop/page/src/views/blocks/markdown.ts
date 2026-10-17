// Markdown, the small subset a ticket or a CI log is written in, drawn as
// elements. The text is an agent's, read from somewhere we do not control:
// every string goes in as a text node, raw HTML shows as the text it is,
// images are links, and only an http(s) URL becomes a link at all.

import { el, type Child } from '../../kit/dom.js';
import { viewLink } from '../link.js';

// Nesting is capped so no input can ask for a deep recursion; every scan
// for a closer is either capped or remembered, so a long run of unclosed
// markers stays linear.
const MAX_DEPTH = 5;
const MAX_LABEL = 300;
const MAX_URL = 1000;

const PUNCT = /[!-/:-@[-`{-~]/;
const BLANK = /^\s*$/;
const FENCE = /^ {0,3}(`{3,}|~{3,})/;
const HEADING = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$/;
const RULE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const QUOTE = /^ {0,3}> ?/;
const ITEM = /^( *)([-*+]|\d{1,9}[.)])[ \t]+(\S.*)$/;
const TABLE_SEP = /^ {0,3}\|?[ \t]*:?-+:?[ \t]*(?:\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$/;

/** Markdown drawn as elements, every string as text. */
export function markdown(text: string): HTMLElement {
  try {
    const lines = text.replace(/\r\n?/g, '\n').replace(/\t/g, '    ').split('\n');
    return el('div', { class: 'md' }, parseBlocks(lines, 0));
  } catch {
    // A pathological nesting the caps missed still shows its text.
    return el('div', { class: 'md' }, text);
  }
}

function startsBlock(line: string): boolean {
  return (
    FENCE.test(line) || HEADING.test(line) || RULE.test(line) || QUOTE.test(line) || ITEM.test(line)
  );
}

function parseBlocks(lines: string[], depth: number): HTMLElement[] {
  const out: HTMLElement[] = [];
  const n = lines.length;
  let i = 0;
  while (i < n) {
    const line = lines[i] ?? '';
    if (BLANK.test(line)) {
      i++;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      const mark = fence[1] ?? '```';
      const closer = new RegExp(`^ {0,3}${mark[0] === '`' ? '`' : '~'}{${mark.length},}[ \\t]*$`);
      const body: string[] = [];
      i++;
      while (i < n && !closer.test(lines[i] ?? '')) body.push(lines[i++] ?? '');
      i++;
      out.push(el('pre', { class: 'md-code' }, el('code', null, body.join('\n'))));
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      // A view is not a document: no heading is larger than a section's.
      const tag = (heading[1] ?? '#').length <= 3 ? 'h3' : 'h4';
      out.push(el(tag, null, inline(heading[2] ?? '', 0, false)));
      i++;
      continue;
    }

    if (RULE.test(line)) {
      out.push(el('hr', null));
      i++;
      continue;
    }

    if (depth < MAX_DEPTH && QUOTE.test(line)) {
      const inner: string[] = [];
      while (i < n && QUOTE.test(lines[i] ?? '')) inner.push((lines[i++] ?? '').replace(QUOTE, ''));
      out.push(el('blockquote', null, parseBlocks(inner, depth + 1)));
      continue;
    }

    if (ITEM.test(line)) {
      i = list(lines, i, out);
      continue;
    }

    if (
      line.includes('|') &&
      TABLE_SEP.test(lines[i + 1] ?? '') &&
      (lines[i + 1] ?? '').includes('|')
    ) {
      i = table(lines, i, out);
      continue;
    }

    const para: string[] = [line];
    i++;
    while (i < n && !BLANK.test(lines[i] ?? '') && !startsBlock(lines[i] ?? '')) {
      para.push(lines[i++] ?? '');
    }
    out.push(el('p', null, inline(para.join('\n'), 0, false)));
  }
  return out;
}

const kind = (marker: string): string => (/\d/.test(marker) ? 'ol' : 'ul');

interface Item {
  text: string[];
  sub: Item[];
  subMarker: string;
}

/** Reads the list starting at `from`, one nested level deep; returns the
 *  line after it. */
function list(lines: string[], from: number, out: HTMLElement[]): number {
  const first = ITEM.exec(lines[from] ?? '');
  const base = first?.[1]?.length ?? 0;
  const topKind = kind(first?.[2] ?? '-');
  const items: Item[] = [];
  let last: Item | null = null;
  let i = from;
  while (i < lines.length) {
    const line = lines[i] ?? '';
    if (BLANK.test(line)) {
      let j = i;
      while (j < lines.length && BLANK.test(lines[j] ?? '')) j++;
      const next = ITEM.exec(lines[j] ?? '');
      if (!next || RULE.test(lines[j] ?? '') || (next[1]?.length ?? 0) < base) break;
      i = j;
      continue;
    }
    const m = RULE.test(line) ? null : ITEM.exec(line);
    if (m) {
      const indent = m[1]?.length ?? 0;
      const marker = m[2] ?? '-';
      if (indent < base) break;
      const item: Item = { text: [m[3] ?? ''], sub: [], subMarker: marker };
      const parent = items[items.length - 1];
      if (indent - base >= 2 && parent) {
        if (parent.sub.length === 0) parent.subMarker = marker;
        parent.sub.push(item);
        last = item;
      } else {
        if (kind(marker) !== topKind) break;
        items.push(item);
        last = item;
      }
      i++;
      continue;
    }
    if (startsBlock(line) || !last) break;
    last.text.push(line.trim());
    i++;
  }
  out.push(renderList(items, topKind, first?.[2] ?? '-'));
  return i;
}

function renderList(items: Item[], tag: string, marker: string): HTMLElement {
  const start = parseInt(marker, 10);
  const lis = items.map((item) =>
    el(
      'li',
      null,
      inline(item.text.join('\n'), 0, false),
      item.sub.length > 0 ? renderList(item.sub, kind(item.subMarker), item.subMarker) : null,
    ),
  );
  const props = tag === 'ol' && start > 1 ? { start } : null;
  return el(tag === 'ol' ? 'ol' : 'ul', props, lis);
}

function cells(line: string): string[] {
  const row = line.trim();
  const out: string[] = [];
  let cell = '';
  for (let i = 0; i < row.length; i++) {
    const c = row[i] ?? '';
    if (c === '\\' && row[i + 1] === '|') {
      cell += '|';
      i++;
    } else if (c === '|') {
      out.push(cell.trim());
      cell = '';
    } else cell += c;
  }
  out.push(cell.trim());
  if (row.startsWith('|')) out.shift();
  if (row.endsWith('|') && !row.endsWith('\\|')) out.pop();
  return out;
}

function table(lines: string[], from: number, out: HTMLElement[]): number {
  const head = cells(lines[from] ?? '');
  const rows: string[][] = [];
  let i = from + 2;
  while (i < lines.length && !BLANK.test(lines[i] ?? '') && (lines[i] ?? '').includes('|')) {
    rows.push(cells(lines[i] ?? ''));
    i++;
  }
  const width = head.length;
  const row = (tag: 'th' | 'td', values: string[]): HTMLElement =>
    el(
      'tr',
      null,
      Array.from({ length: width }, (_, k) => el(tag, null, inline(values[k] ?? '', 1, false))),
    );
  out.push(
    el(
      'div',
      { class: 'md-table' },
      el(
        'table',
        null,
        el('thead', null, row('th', head)),
        el(
          'tbody',
          null,
          rows.map((r) => row('td', r)),
        ),
      ),
    ),
  );
  return i;
}

/** The URL itself when it is http(s), normalised; null for anything else. */
function safeUrl(raw: string): string | null {
  if (!/^https?:\/\/[^\s]/i.test(raw)) return null;
  try {
    const url = new URL(raw);
    return (url.protocol === 'http:' || url.protocol === 'https:') && url.hostname
      ? url.href
      : null;
  } catch {
    return null;
  }
}

interface Link {
  end: number;
  label: string;
  url: string;
}

/** `[label](url "title")` with the `[` at `open`, or null. */
function linkAt(src: string, open: number): Link | null {
  let depth = 0;
  let close = -1;
  const limit = Math.min(src.length, open + MAX_LABEL);
  for (let k = open + 1; k < limit; k++) {
    const c = src[k];
    if (c === '\\') k++;
    else if (c === '[') depth++;
    else if (c === ']') {
      if (depth === 0) {
        close = k;
        break;
      }
      depth--;
    }
  }
  if (close < 0 || src[close + 1] !== '(') return null;

  let k = close + 2;
  while (src[k] === ' ') k++;
  const from = k;
  let url: string;
  if (src[k] === '<') {
    const gt = src.indexOf('>', k);
    if (gt < 0 || gt - k > MAX_URL || src.slice(k, gt).includes('\n')) return null;
    url = src.slice(k + 1, gt);
    k = gt + 1;
  } else {
    let parens = 0;
    const cap = Math.min(src.length, from + MAX_URL);
    while (k < cap && !/\s/.test(src[k] ?? '')) {
      if (src[k] === '(') parens++;
      else if (src[k] === ')') {
        if (parens === 0) break;
        parens--;
      }
      k++;
    }
    url = src.slice(from, k);
  }
  while (src[k] === ' ') k++;
  const quote = src[k];
  if (quote === '"' || quote === "'") {
    const end = src.indexOf(quote, k + 1);
    if (end < 0 || end - k > MAX_LABEL) return null;
    k = end + 1;
    while (src[k] === ' ') k++;
  }
  if (src[k] !== ')') return null;
  return { end: k + 1, label: src.slice(open + 1, close), url };
}

function bareUrlEnd(src: string, from: number): number {
  let end = from;
  const cap = Math.min(src.length, from + MAX_URL);
  while (end < cap && !/[\s<]/.test(src[end] ?? '')) end++;
  const count = (ch: string): number => src.slice(from, end).split(ch).length - 1;
  while (end > from) {
    const c = src[end - 1] ?? '';
    if (/[.,;:!?'"*_~]/.test(c) || (c === ')' && count(')') > count('('))) end--;
    else break;
  }
  return end;
}

const WORD = /[A-Za-z0-9]/;

function inline(src: string, depth: number, inLink: boolean): Child[] {
  const out: Child[] = [];
  const n = src.length;
  // Delimiters known to have no closer anywhere ahead, so a long run of
  // unclosed ones is not rescanned from each.
  const dead = new Set<string>();
  let buf = '';
  let i = 0;
  const flush = (): void => {
    if (buf) out.push(buf);
    buf = '';
  };
  const emit = (nodes: Child[]): void => {
    flush();
    out.push(...nodes);
  };

  scan: while (i < n) {
    const c = src[i] ?? '';

    if (c === '\\' && i + 1 < n && PUNCT.test(src[i + 1] ?? '')) {
      buf += src[i + 1];
      i += 2;
      continue;
    }
    if (c === '\n') {
      buf = buf.replace(/ +$/, '');
      flush();
      out.push(el('br', null));
      i++;
      continue;
    }

    if (c === '`') {
      let run = 1;
      while (src[i + run] === '`') run++;
      const ticks = '`'.repeat(run);
      const key = `\`${run}`;
      if (!dead.has(key)) {
        let k = i + run;
        while (true) {
          k = src.indexOf(ticks, k);
          if (k < 0) {
            dead.add(key);
            break;
          }
          let end = k + run;
          while (src[end] === '`') end++;
          if (end - k === run) {
            let code = src.slice(i + run, k).replace(/\n/g, ' ');
            if (code.length > 2 && code.startsWith(' ') && code.endsWith(' '))
              code = code.slice(1, -1);
            emit([el('code', null, code)]);
            i = end;
            continue scan;
          }
          k = end;
        }
      }
      buf += ticks;
      i += run;
      continue;
    }

    if (depth < MAX_DEPTH) {
      const image = c === '!' && src[i + 1] === '[';
      if ((c === '[' || image) && !inLink) {
        const link = linkAt(src, image ? i + 1 : i);
        if (link) {
          const href = safeUrl(link.url);
          const label = inline(link.label, depth + 1, true);
          if (href) {
            emit([viewLink(href, label.length > 0 ? label : href)]);
          } else {
            emit(label);
          }
          i = link.end;
          continue;
        }
      }

      const delims = c === '*' ? ['**', '*'] : c === '_' ? ['__', '_'] : c === '~' ? ['~~'] : [];
      for (const d of delims) {
        if (!src.startsWith(d, i) || dead.has(d)) continue;
        const after = src[i + d.length];
        if (after === undefined || /\s/.test(after)) continue;
        if (c === '_' && i > 0 && WORD.test(src[i - 1] ?? '')) continue;
        let k = i + d.length;
        while (true) {
          k = src.indexOf(d, k);
          if (k < 0) {
            dead.add(d);
            break;
          }
          let run = 1;
          while (src[k + run] === c) run++;
          // A single marker steps over a longer run; a double one closes on
          // the last two of it, so `***x***` nests.
          if (d.length === 1 && run > 1) {
            k += run;
            continue;
          }
          const at = d.length === 2 ? k + run - 2 : k;
          const before = src[at - 1] ?? '';
          const ok =
            at > i + d.length &&
            !/\s/.test(before) &&
            !(c === '_' && WORD.test(src[at + d.length] ?? ''));
          if (ok) {
            const tag = d === '~~' ? 'del' : d.length === 2 ? 'strong' : 'em';
            emit([el(tag, null, inline(src.slice(i + d.length, at), depth + 1, inLink))]);
            i = at + d.length;
            continue scan;
          }
          k += run;
        }
      }
    }

    if (!inLink && (c === 'h' || c === 'H') && (i === 0 || !WORD.test(src[i - 1] ?? ''))) {
      if (/^https?:\/\//i.test(src.slice(i, i + 8))) {
        const end = bareUrlEnd(src, i);
        const raw = src.slice(i, end);
        const href = safeUrl(raw);
        if (href) {
          emit([viewLink(href, raw)]);
          i = end;
          continue;
        }
      }
    }

    buf += c;
    i++;
  }
  flush();
  return out;
}
