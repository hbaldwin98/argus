// A diff view block: a unified diff drawn as the review viewer draws one.
// The agent's text is read here, into the review viewer's own rows, so a
// diff looks the same wherever it is and `review.ts` stays the one place
// that knows how a line is laid out.
import { code } from './code.js';
import { el } from './dom.js';
import { diffRow } from './review.js';
const HUNK = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/;
function side(kind, marker, text, old, now) {
    return { old, new: now, marker, kind, parts: [[text, '']] };
}
function push(rows, kind, text, left) {
    rows.push({ row: rows.length, kind, text, left, right: null, cursor: false, picked: false });
}
/** The diff's rows, or null when it holds no hunk to draw. Counts in a
 *  hunk header say where the hunk ends, which is how a removed line that
 *  reads `-- x` is told from the next file's `--- a/x`. */
export function parsePatch(text) {
    const lines = text.replace(/\r\n/g, '\n').split('\n');
    if (lines[lines.length - 1] === '')
        lines.pop();
    const out = { rows: [], added: 0, removed: 0, file: null };
    let hunks = 0;
    let oldLeft = 0;
    let newLeft = 0;
    let oldNo = 0;
    let newNo = 0;
    for (const line of lines) {
        if (oldLeft > 0 || newLeft > 0) {
            const c = line[0];
            const body = line.slice(1);
            if (c === '\\') {
                push(out.rows, 'note', line, null);
            }
            else if (c === '+' && newLeft > 0) {
                push(out.rows, 'line', '', side('added', '+', body, null, newNo));
                newNo += 1;
                newLeft -= 1;
                out.added += 1;
            }
            else if (c === '-' && oldLeft > 0) {
                push(out.rows, 'line', '', side('removed', '-', body, oldNo, null));
                oldNo += 1;
                oldLeft -= 1;
                out.removed += 1;
            }
            else if (oldLeft > 0 && newLeft > 0 && (c === ' ' || line === '')) {
                push(out.rows, 'line', '', side('context', '', body, oldNo, newNo));
                oldNo += 1;
                newNo += 1;
                oldLeft -= 1;
                newLeft -= 1;
            }
            else
                return null;
            continue;
        }
        const hunk = HUNK.exec(line);
        if (hunk) {
            hunks += 1;
            oldNo = Number(hunk[1]);
            newNo = Number(hunk[3]);
            oldLeft = hunk[2] === undefined ? 1 : Number(hunk[2]);
            newLeft = hunk[4] === undefined ? 1 : Number(hunk[4]);
            push(out.rows, 'hunk', line, null);
        }
        else if (line.startsWith('+++ ')) {
            const name = line.slice(4).split('\t')[0] ?? '';
            if (out.file === null && name !== '/dev/null')
                out.file = name.replace(/^b\//, '');
        }
        else if (line.startsWith('diff ') || line.startsWith('\\')) {
            push(out.rows, 'note', line, null);
        }
        // `index`, `---`, mode and rename lines say nothing the header does not.
    }
    // Only no hunk at all is not a diff; a hunk cut short is still read.
    return hunks > 0 ? out : null;
}
export function patch(block) {
    const parsed = parsePatch(block.patch);
    if (!parsed)
        return code('diff', block.patch);
    const name = block.path ?? parsed.file;
    return el('div', { class: 'vdiff' }, el('div', { class: 'fhdr' }, name && el('span', { class: 'mono vdiff-path' }, name), el('span', { class: 'grow' }), el('span', { class: 'meta' }, el('span', { class: 'fg-ok' }, `+${parsed.added}`), ' ', el('span', { class: 'fg-danger' }, `−${parsed.removed}`))), el('div', {
        class: 'dbody vdiff-body',
        role: 'region',
        'aria-label': name ? `Diff of ${name}` : 'Diff',
    }, parsed.rows.map((r) => diffRow(r))));
}
