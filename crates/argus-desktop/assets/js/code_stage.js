// The Code stage: a checkout's files at one version, in a tree that marks
// what changed, one file read whole with its changed lines washed as a
// review washes them, and its outline. Rust holds the files, the cursor
// and both fields (`src/code_stage.rs`); the page draws them and says what
// was clicked or typed.
import { $, el, ghosts, icon } from './dom.js';
import { actionFor } from './keymap.js';
import { state } from './state.js';
import { call } from './tauri.js';
function click(target) {
    call('code_click', { target });
}
function segment(label, on, pick) {
    return el('button', { class: `segb${on ? ' on' : ''}`, role: 'radio', 'aria-checked': String(on), onclick: pick }, label);
}
function head(c) {
    const version = (name, label) => segment(label, c.version === name, () => click({ on: 'version', version: name }));
    return el('div', { class: 'code-head' }, el('span', { class: 'meta' }, c.name), c.branch && el('span', { class: 'mono fg-ref' }, c.branch), el('span', { class: 'type-heading' }, 'Code'), c.version === 'commit'
        ? el('span', { class: 'pill mono' }, c.commit ?? 'commit')
        : el('div', { class: 'seg', role: 'radiogroup', 'aria-label': 'Which version of the files' }, version('working', 'Working tree'), version('index', 'Index'), version('head', 'HEAD')), el('span', { class: 'grow' }), c.back &&
        el('button', { class: 'btn sm', onclick: () => click({ on: 'back' }) }, `Back to ${c.back}`, el('span', { class: 'kbd' }, 'Esc')));
}
/** A field whose letters and keys go to Rust as gestures: go to a file, or
 *  search. It keeps the caret across redraws, as the review's jump does. */
function field(id, call_, placeholder, query) {
    const send = (gesture) => call(call_, { gesture });
    const live = document.getElementById(id);
    const input = el('input', {
        class: 'field',
        id,
        placeholder,
        'aria-label': placeholder,
        oninput: (e) => {
            if (e.target instanceof HTMLInputElement)
                send({ on: 'query', text: e.target.value });
        },
        onkeydown: (e) => {
            const action = actionFor(['jump'], e);
            if (action === 'next' || action === 'previous')
                send({ on: 'step', by: action === 'next' ? 1 : -1 });
            else if (action === 'choose')
                send({ on: 'choose' });
            else if (action === 'closeLayer') {
                e.stopPropagation();
                send({ on: 'close' });
            }
            else
                return;
            e.preventDefault();
        },
    });
    input.value = live instanceof HTMLInputElement ? live.value : query;
    return input;
}
function jumpList(j) {
    return el('div', { class: 'code-list' }, j.matches.map((path, i) => el('button', { class: `trow${i === j.at ? ' on' : ''}`, onclick: () => click({ on: 'tree', path }) }, el('span', { class: 'ttl mono' }, path))), j.matches.length === 0 && el('div', { class: 'meta' }, `No file matches “${j.query}”.`));
}
function searchList(s) {
    let said = null;
    if (s.loading)
        said = 'Searching…';
    else if (s.searched === null || s.searched !== s.query)
        said = 'Enter searches the working tree.';
    else if (s.matches.length === 0)
        said = `Nothing matches “${s.searched}”.`;
    return el('div', { class: 'code-list' }, said && el('div', { class: 'meta' }, said), s.searched === s.query &&
        s.matches.map((m, index) => el('button', {
            class: `mrow${index === s.at ? ' on' : ''}`,
            onclick: () => click({ on: 'match', index }),
        }, el('span', { class: 'mono fg-faint' }, `${m.path}:${m.line}`), el('span', { class: 'mono mtext' }, m.before, el('mark', {}, m.hit), m.after))), s.truncated && el('div', { class: 'meta' }, 'Only the first 500 are listed.'));
}
function tree(c) {
    const top = c.search
        ? field('code-search', 'code_search', 'Search the checkout  Ctrl F', c.search.query)
        : c.jump
            ? field('code-jump', 'code_jump', 'Go to a file', c.jump.query)
            : null;
    let body;
    if (c.search)
        body = searchList(c.search);
    else if (c.jump)
        body = jumpList(c.jump);
    else if (c.treeLoading)
        body = ghosts([[0.6], [0.8], [0.5], [0.7]]);
    else
        body = el('div', { class: 'code-list' }, c.tree.map((r) => el('button', {
            class: `trow${r.folder ? ' dir' : ''}${r.on ? ' on' : ''}`,
            style: `padding-left: ${8 + r.depth * 14}px`,
            title: r.path,
            onclick: () => click({ on: 'tree', path: r.path }),
        }, r.folder ? icon(r.open ? 'down' : 'right') : el('span', { class: 'tsp' }), r.changed
            ? el('span', { class: 'tchg', 'aria-label': 'changed' })
            : el('span', { class: 'tsp' }), el('span', { class: 'ttl' }, r.name), !r.folder &&
            r.changed &&
            el('span', { class: 'cnum' }, el('span', { class: 'fg-ok' }, `+${r.added}`), ' ', el('span', { class: 'fg-danger' }, `−${r.removed}`)))));
    return el('aside', { class: 'code-tree', 'aria-label': 'Files in the checkout' }, top ??
        el('div', { class: 'seg', role: 'radiogroup', 'aria-label': 'Which files' }, segment('All', !c.changedOnly, () => click({ on: 'changedOnly', only: false })), segment(`Changed ${c.changed}`, c.changedOnly, () => click({ on: 'changedOnly', only: true }))), body);
}
function line(r) {
    const row = el('div', {
        class: `cline${r.added ? ' add' : ''}${r.cursor ? ' cursor' : ''}${r.picked ? ' picked' : ''}`,
        onclick: (e) => click({ on: 'line', line: r.no, extend: e.shiftKey }),
    }, el('span', { class: 'dno' }, String(r.no)), el('span', { class: 'dmark' }, r.added ? '+' : ' '), el('span', { class: 'dcode' }, r.parts.map(([text, kind]) => (kind ? el('span', { class: `sx-${kind}` }, text) : text))));
    return r.removedBefore > 0 ? [gone(r.removedBefore), row] : [row];
}
function gone(n) {
    return el('div', { class: 'cgone' }, `− ${n} ${n === 1 ? 'line' : 'lines'} removed here`);
}
function file(c) {
    let body;
    if (c.failed && c.lines.length === 0)
        body = el('div', { class: 'review-empty fg-muted' }, c.failed);
    else if (!c.path)
        body = el('div', { class: 'review-empty fg-muted' }, 'Pick a file on the left, or Ctrl P.');
    else if (c.fileLoading && c.lines.length === 0)
        body = ghosts([[0.5], [0.9], [0.8], [0.7], [0.6]]);
    else if (c.note)
        body = el('div', { class: 'review-empty fg-muted' }, `${c.path}: ${c.note}`);
    else {
        // The comment box sits under the last picked line, or the cursor's.
        let under = -1;
        c.lines.forEach((r, i) => {
            if (r.picked || (r.cursor && under < 0))
                under = i;
        });
        body = [
            ...c.lines.flatMap((r, i) => c.composing && i === under ? [...line(r), commentBox(c)] : line(r)),
            ...(c.removedAtEnd > 0 ? [gone(c.removedAtEnd)] : []),
        ];
    }
    return el('section', { class: 'code-file' }, c.path &&
        el('div', { class: 'fhdr' }, el('span', { class: 'mono' }, c.path), el('span', { class: 'grow' }), (c.added > 0 || c.removed > 0) &&
            el('span', { class: 'meta' }, el('span', { class: 'fg-ok' }, `+${c.added}`), ' ', el('span', { class: 'fg-danger' }, `−${c.removed}`))), el('div', { class: 'cbody', id: 'code-body' }, body));
}
function sendComment() {
    call('code_comment', { body: state.codeDraft });
}
function commentBox(c) {
    const body = el('textarea', {
        class: 'field comment-body',
        id: 'code-comment',
        'aria-label': 'Comment',
        rows: '3',
        oninput: (e) => {
            if (e.target instanceof HTMLTextAreaElement)
                state.codeDraft = e.target.value;
        },
        onkeydown: (e) => {
            const action = actionFor(['writing'], e);
            if (action === 'save') {
                e.preventDefault();
                sendComment();
            }
            else if (action === 'closeLayer') {
                e.preventDefault();
                e.stopPropagation();
                call('code_comment', { body: null });
            }
        },
    });
    body.value = state.codeDraft;
    const chosen = c.recipients.find((p) => p.pane === c.recipient);
    return el('div', { class: 'comment-card' }, el('div', { class: 'actions' }, el('span', { class: 'fg-accent strong' }, 'Comment'), el('span', { class: 'meta' }, `${c.pick ?? ''} · ${c.path ?? ''}`)), body, el('div', { class: 'actions' }, el('span', { class: 'meta' }, 'Send to'), c.recipients.length > 1
        ? c.recipients.map((p) => el('button', {
            class: `chipf${p.pane === c.recipient ? ' on' : ''}`,
            onclick: () => click({ on: 'recipient', pane: p.pane }),
        }, el('span', { class: 'glyph' }, '◆'), p.title))
        : el('span', { class: 'chipf on' }, el('span', { class: 'glyph' }, '◆'), chosen?.title ?? ''), el('span', { class: 'grow' }), el('button', { class: 'btn', onclick: () => call('code_comment', { body: null }) }, 'Cancel'), el('button', { class: 'btn primary', onclick: sendComment }, 'Comment', el('span', { class: 'kbd on-accent' }, 'Ctrl Enter'))));
}
function outline(c) {
    return el('aside', { class: 'code-outline', 'aria-label': 'Outline' }, el('div', { class: 'mlabel' }, 'Outline'), c.symbols.length === 0
        ? el('div', { class: 'meta' }, c.path ? 'Nothing defined that the outline knows of.' : '')
        : c.symbols.map((s) => el('button', {
            class: `orow${s.on ? ' on' : ''}`,
            onclick: () => click({ on: 'symbol', line: s.line }),
        }, el('span', { class: 'okind' }, s.kind), el('span', { class: 'mono' }, s.name))));
}
export function drawCode() {
    if (state.rail.stage !== 'code')
        return;
    const stage = $('kstage');
    const c = state.code;
    if (!c?.open) {
        stage.replaceChildren(ghosts([[0.3], [0.9], [0.85], [0.6]]));
        return;
    }
    const scrolled = document.getElementById('code-body')?.scrollTop ?? 0;
    if (!c.composing)
        state.codeDraft = '';
    const typing = document.activeElement?.id === 'code-comment';
    stage.replaceChildren(head(c), el('div', { class: 'code-split' }, tree(c), file(c), outline(c)), el('div', { class: 'dfoot meta' }, 'J K line · ] [ change · V pick · C comment · Ctrl P file · Ctrl F search · E editor · R read again'));
    const body = document.getElementById('code-body');
    if (body)
        body.scrollTop = scrolled;
    stage.querySelector('.cline.cursor')?.scrollIntoView({ block: 'nearest' });
    stage.querySelector('.code-list .on')?.scrollIntoView({ block: 'nearest' });
    // A field just opened takes the keys; one being typed in keeps them.
    const want = c.composing
        ? 'code-comment'
        : c.search
            ? 'code-search'
            : c.jump
                ? 'code-jump'
                : null;
    if (want === 'code-comment' && !typing) {
        const box = document.getElementById(want);
        if (box instanceof HTMLTextAreaElement) {
            box.focus();
            box.setSelectionRange(box.value.length, box.value.length);
        }
        return;
    }
    const input = want ? document.getElementById(want) : null;
    if (input instanceof HTMLInputElement) {
        input.focus();
        input.setSelectionRange(input.value.length, input.value.length);
    }
}
/** Runs a Code stage key. */
export function codeAction(action) {
    call('code_action', { action });
    return true;
}
