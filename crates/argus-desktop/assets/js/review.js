// The review dialog: a checkout's changed files beside the diff of one,
// lines picked and commented on in place. Rust holds the diff, the cursor
// and the range (`src/review.rs`); the page draws them and says what was
// pressed or clicked. A redraw keeps the scroll, and the comment being
// written, where they were.
import { $, el, ghosts, icon } from './dom.js';
import { actionFor } from './keymap.js';
import { state } from './state.js';
import { call } from './tauri.js';
function keycap(key) {
    return el('span', { class: 'kbd' }, key);
}
function segment(label, on, pick, ...rest) {
    return el('button', { class: `segb${on ? ' on' : ''}`, role: 'radio', 'aria-checked': String(on), onclick: pick }, label, ...rest);
}
function count(n) {
    return el('span', { class: 'fg-faint' }, n === null ? '…' : String(n));
}
function head(r) {
    return el('div', { class: 'dhead review-head' }, el('div', { class: 'type-title', id: 'review-title' }, 'Review · ', r.name, ' ', el('span', { class: 'mono fg-ref' }, r.branch)), r.base === 'commit'
        ? el('span', { class: 'meta mono' }, r.commit ?? '')
        : el('div', { class: 'seg', role: 'radiogroup', 'aria-label': 'Side' }, segment('Unstaged ', r.base === 'unstaged', () => click({ on: 'side', staged: false }), count(r.unstaged)), segment('Staged ', r.base === 'staged', () => click({ on: 'side', staged: true }), count(r.staged))), el('div', { class: 'seg', role: 'radiogroup', 'aria-label': 'Layout' }, segment('Unified', !r.split, () => !r.split || call('review_action', { action: 'split' })), segment('Split', r.split, () => r.split || call('review_action', { action: 'split' }))), r.files.length > 0 &&
        el('span', { class: 'meta' }, `${r.files.length} file${r.files.length === 1 ? '' : 's'} · `, el('span', { class: 'fg-ok' }, `+${r.added}`), ' ', el('span', { class: 'fg-danger' }, `−${r.removed}`)), el('span', { class: 'grow' }), el('button', { class: 'btn sm', onclick: () => call('review_refresh', {}) }, 'Refresh'), el('button', {
        class: 'icon',
        'aria-label': 'Close the review',
        onclick: () => call('review_action', { action: 'closeLayer' }),
    }, icon('close')));
}
function click(target) {
    call('review_click', { target });
}
function jump(gesture) {
    call('review_jump', { gesture });
}
/**
 * The Ctrl-P field: its keys are its own, and Rust says what matches. The
 * text is the field's own once it is open: Rust's echo of a letter can
 * land after the next is typed, and putting it back would drop that one.
 */
function jumpField(query) {
    const live = document.getElementById('review-jump');
    const field = el('input', {
        class: 'field',
        id: 'review-jump',
        placeholder: 'Jump to a file',
        'aria-label': 'Jump to a file',
        oninput: (e) => {
            if (e.target instanceof HTMLInputElement)
                jump({ on: 'query', text: e.target.value });
        },
        onkeydown: (e) => {
            // Ctrl-N and Ctrl-P step as in fzf, and must not reach the webview,
            // where Ctrl-P prints.
            const action = actionFor(['jump'], e);
            if (action === 'next' || action === 'previous') {
                jump({ on: 'step', by: action === 'next' ? 1 : -1 });
            }
            else if (action === 'choose')
                jump({ on: 'choose' });
            else if (action === 'closeLayer') {
                // The field closes, not the dialog under it.
                e.stopPropagation();
                jump({ on: 'close' });
            }
            else
                return;
            e.preventDefault();
        },
    });
    field.value = live instanceof HTMLInputElement ? live.value : query;
    return field;
}
function files(r) {
    const order = r.jump ? r.jump.matches : r.files.map((_, i) => i);
    const lit = r.jump ? r.jump.at : r.file;
    return el('aside', { class: 'review-files', 'aria-label': 'Changed files' }, r.jump ? jumpField(r.jump.query) : el('div', { class: 'mlabel' }, 'Changed files'), el('div', { class: 'review-file-list' }, order.map((i) => {
        const f = r.files[i];
        return (f &&
            el('button', {
                class: `rfile${i === lit ? ' on' : ''}`,
                title: f.path,
                onclick: () => click({ on: 'file', file: i }),
            }, el('span', { class: 'rname' }, el('span', { class: 'fg-faint' }, f.dir), f.name), el('span', { class: 'fg-ok' }, `+${f.added}`), el('span', { class: 'fg-danger' }, `−${f.removed}`)));
    }), r.jump &&
        r.jump.matches.length === 0 &&
        el('div', { class: 'meta' }, `No file matches “${r.jump.query}”.`)));
}
function code(side) {
    return el('span', { class: 'dcode' }, side.parts.map(([text, kind]) => (kind ? el('span', { class: `sx-${kind}` }, text) : text)));
}
function no(n) {
    return el('span', { class: 'dno' }, n == null ? '' : String(n));
}
function numbers(side, both) {
    return both
        ? [no(side?.old), no(side?.new)]
        : [no(side?.kind === 'added' ? side.new : (side?.old ?? side?.new))];
}
function half(side, both) {
    return el('div', { class: `dside ${side?.kind ?? 'blank'}` }, numbers(side, both), el('span', { class: 'dmark' }, side?.marker ?? ''), side ? code(side) : el('span', { class: 'dcode' }));
}
/** A row of a diff; `pick` is what a click on a line does, if anything. */
export function diffRow(r, pick) {
    if (r.kind === 'hunk' || r.kind === 'note') {
        return el('div', { class: `drow ${r.kind}` }, r.text);
    }
    const line = r.kind === 'line';
    return el('div', {
        class: `drow ${r.kind}${r.cursor ? ' cursor' : ''}${r.picked ? ' picked' : ''}`,
        onclick: pick ?? null,
    }, line ? half(r.left, true) : [half(r.left, false), half(r.right, false)]);
}
function commentBox(r) {
    const body = el('textarea', {
        class: 'field comment-body',
        id: 'review-comment',
        'aria-label': 'Comment',
        rows: '3',
        oninput: (e) => {
            if (e.target instanceof HTMLTextAreaElement)
                state.reviewDraft = e.target.value;
        },
        onkeydown: (e) => {
            const action = actionFor(['writing'], e);
            if (action === 'save') {
                e.preventDefault();
                send();
            }
            else if (action === 'closeLayer') {
                e.preventDefault();
                e.stopPropagation();
                call('review_comment', { body: null });
            }
        },
    });
    body.value = state.reviewDraft;
    const chosen = r.recipients.find((p) => p.pane === r.recipient);
    return el('div', { class: 'comment-card' }, el('div', { class: 'actions' }, el('span', { class: 'fg-accent strong' }, 'Comment'), el('span', { class: 'meta' }, `${r.pick ?? ''} · ${r.base}`)), body, el('div', { class: 'actions' }, el('span', { class: 'meta' }, 'Send to'), r.recipients.length > 1
        ? r.recipients.map((p) => el('button', {
            class: `chipf${p.pane === r.recipient ? ' on' : ''}`,
            onclick: () => click({ on: 'recipient', pane: p.pane }),
        }, el('span', { class: 'glyph' }, '◆'), p.title))
        : el('span', { class: 'chipf on' }, el('span', { class: 'glyph' }, '◆'), chosen?.title ?? ''), el('span', { class: 'grow' }), el('button', { class: 'btn', onclick: () => call('review_comment', { body: null }) }, 'Cancel'), el('button', { class: 'btn primary', onclick: send }, 'Comment ', keycap('Ctrl Enter'))));
}
function send() {
    call('review_comment', { body: state.reviewDraft });
}
function diff(r) {
    const path = r.file === null ? null : r.files[r.file];
    let body;
    if (r.loading)
        body = ghosts([[0.4], [0.9], [0.8], [0.85], [0.6], [0.75]]);
    else if (r.empty)
        body = el('div', { class: 'review-empty fg-muted' }, r.empty);
    else {
        // The comment box sits under the last picked line, or the cursor's.
        const rows = r.rows;
        let under = -1;
        rows.forEach((d, i) => {
            if (d.picked || (d.cursor && under < 0))
                under = i;
        });
        body = rows.flatMap((d, i) => [
            diffRow(d, (e) => click({ on: 'row', row: d.row, extend: e.shiftKey })),
            r.composing && i === under && commentBox(r),
        ]);
    }
    return el('section', { class: 'review-diff' }, path &&
        el('div', { class: 'fhdr' }, el('span', { class: 'mono' }, path.path), el('span', { class: 'grow' }), r.pick && el('span', { class: 'meta' }, `${r.pick} picked`)), el('div', { class: `dbody${r.split ? ' split' : ''}`, id: 'review-body' }, body));
}
export function drawReview() {
    const r = state.review;
    const host = $('review');
    if (!r?.open) {
        host.hidden = true;
        host.replaceChildren();
        state.reviewDraft = '';
        return;
    }
    // What a redraw would lose: where each list was scrolled, and whether
    // the comment had the keys.
    const scrolled = ['review-body', 'review-files'].map((id) => document.getElementById(id)?.scrollTop ?? 0);
    const typing = document.activeElement?.id === 'review-comment';
    if (!r.composing)
        state.reviewDraft = '';
    host.hidden = false;
    const fileList = files(r);
    fileList.id = 'review-files';
    host.replaceChildren(el('div', { class: 'scrim' }, el('section', {
        class: 'dialog review-dialog',
        role: 'dialog',
        'aria-modal': 'true',
        'aria-labelledby': 'review-title',
    }, head(r), el('div', { class: 'review-split' }, fileList, diff(r)), el('div', { class: 'dfoot meta' }, keycap('J'), keycap('K'), ' line · ', keycap('V'), ' pick · ', keycap('C'), ' comment · ', keycap('B'), ' side · ', keycap('S'), ' split · ', keycap('Ctrl P'), ' jump · ', keycap('E'), ' editor · ', keycap('Esc'), ' close'))));
    ['review-body', 'review-files'].forEach((id, i) => {
        const node = document.getElementById(id);
        if (node)
            node.scrollTop = scrolled[i] ?? 0;
    });
    host.querySelector('.drow.cursor')?.scrollIntoView({ block: 'nearest' });
    host.querySelector('.rfile.on')?.scrollIntoView({ block: 'nearest' });
    // The jump field is redrawn as each letter arrives, and keeps the keys
    // and the caret while it is open.
    const field = document.getElementById('review-jump');
    const box = document.getElementById('review-comment');
    const keys = field instanceof HTMLInputElement
        ? field
        : box instanceof HTMLTextAreaElement && (typing || r.composing)
            ? box
            : null;
    if (keys) {
        keys.focus();
        keys.setSelectionRange(keys.value.length, keys.value.length);
    }
}
export function reviewOpen() {
    return state.review?.open === true;
}
