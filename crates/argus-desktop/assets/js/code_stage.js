// The Code stage: a checkout's files in a tree that marks what changed, one
// file read whole with its changes washed and ruled beside it, and what the
// file defines. Rust holds the files, the cursor, the fields and the search
// (`src/code_stage.rs`); the page draws them and says what was clicked or
// typed.
import { commentBox, settleDraft } from './comment_box.js';
import { $, el, ghosts, icon } from './dom.js';
import { actionFor } from './keymap.js';
import { state } from './state.js';
import { call } from './tauri.js';
const ways = {
    id: 'code-comment',
    send: (body) => call('code_comment', { body }),
    choose: (pane) => click({ on: 'recipient', pane }),
};
function click(target) {
    call('code_click', { target });
}
function kbd(key) {
    return el('span', { class: 'kbd' }, key);
}
function segment(label, on, pick) {
    return el('button', { class: `segb${on ? ' on' : ''}`, role: 'radio', 'aria-checked': String(on), onclick: pick }, label);
}
function head(c) {
    return el('div', { class: 'code-head' }, 
    // The window's title already says Code; the checkout, and its branch
    // only where it is named otherwise, is what this adds.
    el('span', { class: 'mono fg-ref' }, c.name), c.branch && c.branch !== c.name && el('span', { class: 'meta mono' }, c.branch), c.version && el('span', { class: 'pill mono' }, c.version), el('span', { class: 'grow' }), c.back &&
        el('button', { class: 'btn sm', onclick: () => click({ on: 'back' }) }, `Back to ${c.back}`, kbd('B')), c.path &&
        el('button', { class: 'btn sm', onclick: () => codeAction('openEditor') }, 'Open in editor', kbd('E')), el('button', {
        class: 'icon',
        title: 'Keys for this view  ?',
        'aria-label': 'Keys for this view',
        onclick: () => call('command', { action: 'shortcuts' }),
    }, kbd('?')));
}
/** A field whose letters and keys go to Rust as gestures. It keeps the
 *  caret across redraws, as the review's jump does. */
function field(id, gestures, placeholder, query, extra = {}) {
    const send = (gesture) => call(gestures, { gesture });
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
            const flip = gestures === 'code_search' ? actionFor(['filter'], e) : null;
            if (flip === 'matchCase' || flip === 'wholeWord' || flip === 'useRegex') {
                e.preventDefault();
                const which = flip === 'matchCase' ? 'case' : flip === 'wholeWord' ? 'word' : 'regex';
                click({ on: 'searchOption', option: which });
                return;
            }
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
        ...extra,
    });
    input.value = live instanceof HTMLInputElement ? live.value : query;
    return input;
}
function chevron(open) {
    return el('span', { class: 'chev' }, icon(open ? 'down' : 'right'));
}
function counts(added, removed) {
    return el('span', { class: 'tright cnum' }, added > 0 && el('span', { class: 'fg-ok' }, `+${added}`), removed > 0 && el('span', { class: 'fg-danger' }, `−${removed}`));
}
function tree(c) {
    return el('div', { class: 'code-list', id: 'code-list' }, c.treeLoading
        ? ghosts([[0.6], [0.8], [0.5], [0.7]])
        : c.tree.map((r) => el('button', {
            class: `trow${r.folder ? ' dir' : ''}${r.on ? ' on' : ''}`,
            style: `--d: ${r.depth}`,
            title: r.path,
            onclick: () => click({ on: 'tree', path: r.path }),
        }, r.folder ? chevron(r.open) : el('span', null), el('span', { class: 'ttl' }, r.name), r.folder
            ? r.changed
                ? el('span', { class: 'tright' }, el('span', { class: 'tchg', 'aria-label': 'changes under it' }))
                : el('span', null)
            : r.changed
                ? counts(r.added, r.removed)
                : el('span', null))));
}
function jumpList(j) {
    return el('div', { class: 'code-list', id: 'code-list' }, j.matches.map((path, i) => el('button', { class: `trow${i === j.at ? ' on' : ''}`, onclick: () => click({ on: 'tree', path }) }, el('span', null), el('span', { class: 'ttl mono' }, path), el('span', null))), j.matches.length === 0 && el('div', { class: 'meta' }, `No file matches “${j.query}”.`));
}
function option(label, on, which, title) {
    return el('button', {
        class: `sopt${on ? ' on' : ''}`,
        'aria-pressed': String(on),
        title,
        onclick: () => click({ on: 'searchOption', option: which }),
    }, label);
}
function hitFile(f) {
    const top = el('button', { class: 'hfile', title: f.path, onclick: () => click({ on: 'hitFile', path: f.path }) }, chevron(f.open), el('span', { class: 'ttl' }, el('span', { class: 'fg-faint' }, f.dir), f.name), el('span', { class: 'hcount' }, String(f.count)));
    if (!f.open)
        return [top];
    return [
        top,
        ...f.hits.map((h) => el('button', {
            class: `hline${h.on ? ' on' : ''}`,
            onclick: () => click({ on: 'match', index: h.index }),
        }, el('span', { class: 'no' }, String(h.line)), el('span', { class: 'tx' }, h.parts.map(([text, hit]) => (hit ? el('mark', {}, text) : text))))),
    ];
}
function searchPanel(s) {
    let said;
    if (s.error)
        said = el('div', { class: 'sum fg-danger' }, s.error);
    else if (s.loading)
        said = el('div', { class: 'sum' }, 'Searching…');
    else if (!s.current)
        said = el('div', { class: 'sum' }, 'Enter searches the working tree.');
    else if (s.total === 0)
        said = el('div', { class: 'sum' }, `Nothing matches “${s.query}”.`);
    else {
        const files = s.files.length;
        said = el('div', { class: 'sum' }, `${s.total}${s.truncated ? '+' : ''} matches in ${files} ${files === 1 ? 'file' : 'files'}`);
    }
    return [
        el('div', { class: 'sbox' }, field('code-search', 'code_search', 'Search the checkout', s.query, { class: 'field mono' }), el('div', { class: 'sopts', role: 'group', 'aria-label': 'How to match' }, option('Aa', s.case, 'case', 'Match case  Alt C'), option('ab', s.word, 'word', 'Whole word  Alt W'), option('.*', s.regex, 'regex', 'Regular expression  Alt R'))),
        el('input', {
            class: 'field mono',
            id: 'code-glob',
            placeholder: 'Files to include, as globs',
            'aria-label': 'Files to include, as globs',
            value: s.glob,
            oninput: (e) => {
                if (e.target instanceof HTMLInputElement)
                    call('code_search_glob', { glob: e.target.value });
            },
            onkeydown: (e) => {
                const action = actionFor(['jump'], e);
                if (action === 'choose')
                    call('code_search', { gesture: { on: 'choose' } });
                else if (action === 'closeLayer') {
                    e.stopPropagation();
                    call('code_search', { gesture: { on: 'close' } });
                }
                else
                    return;
                e.preventDefault();
            },
        }),
        said,
        el('div', { class: 'code-list hits', id: 'code-list' }, s.current && s.files.flatMap(hitFile)),
    ];
}
function side(c) {
    const body = c.search
        ? searchPanel(c.search)
        : [
            field('code-jump', 'code_jump', 'Go to a file   Ctrl P', c.jump?.query ?? '', {
                // The field is always there; reaching it opens the list it filters.
                onfocus: () => c.jump || codeAction('filter'),
            }),
            !c.jump &&
                el('div', { class: 'code-tools' }, el('div', { class: 'seg', role: 'radiogroup', 'aria-label': 'Which files' }, segment('All', !c.changedOnly, () => click({ on: 'changedOnly', only: false })), segment(`Changed ${c.changed}`, c.changedOnly, () => click({ on: 'changedOnly', only: true }))), el('span', { class: 'grow' }), 
                // Search is a key, Ctrl F, and a key nobody can see is a feature
                // nobody finds.
                el('button', { class: 'btn sm', onclick: () => codeAction('search') }, 'Search', kbd('Ctrl F'))),
            c.jump ? jumpList(c.jump) : tree(c),
        ];
    return el('aside', { class: 'code-tree', 'aria-label': 'Files in the checkout' }, body);
}
function line(r) {
    const row = el('div', {
        class: `cline${r.added ? ' add' : ''}${r.cursor ? ' cursor' : ''}${r.picked ? ' picked' : ''}`,
        'data-no': r.no,
        onclick: (e) => click({ on: 'line', line: r.no, extend: e.shiftKey }),
    }, el('span', { class: 'dno' }, String(r.no)), el('span', { class: 'dmark' }, r.added ? '+' : ' '), el('span', { class: 'dcode' }, r.parts.map(([text, kind]) => (kind ? el('span', { class: `sx-${kind}` }, text) : text))));
    return r.removedBefore > 0 ? [gone(r.removedBefore), row] : [row];
}
function gone(n) {
    return el('div', { class: 'cgone' }, `− ${n} ${n === 1 ? 'line' : 'lines'} removed here`);
}
/** The change ruler, which is the file's scrollbar: each change where it
 *  is in the whole file, and the band of the file on screen as its thumb.
 *  Marks and band are measured off the same rendered rows, so a removal
 *  marker or a comment box between lines moves them together rather than
 *  sliding one past the other. */
function ruler(c) {
    return el('div', {
        class: 'code-ruler',
        id: 'code-ruler',
        role: 'scrollbar',
        'aria-controls': 'code-body',
        'aria-orientation': 'vertical',
        'aria-label': 'Where this file changed',
        onpointerdown: grab,
    }, el('span', { class: 'view', id: 'code-view' }), c.ruler.map((m) => el('button', {
        class: `m ${m.kind}`,
        'data-line': m.line,
        'data-span': m.span,
        title: m.kind === 'hit' ? `A match at line ${m.line}` : `Changed at line ${m.line}`,
        'aria-label': m.kind === 'hit' ? `A match at line ${m.line}` : `A change at line ${m.line}`,
        onclick: () => click({ on: 'go', line: m.line }),
    })));
}
/** Puts each mark where its rows are in the scrolled body. */
function placeMarks() {
    const body = document.getElementById('code-body');
    const bar = document.getElementById('code-ruler');
    if (!body || !bar || body.scrollHeight === 0)
        return;
    const scale = bar.clientHeight / body.scrollHeight;
    const row = (no) => body.querySelector(`.cline[data-no="${no}"]`);
    for (const mark of bar.querySelectorAll('.m')) {
        const at = Number(mark.dataset['line']);
        const first = row(at);
        if (!first)
            continue;
        const last = row(at + Math.max(1, Number(mark.dataset['span'])) - 1) ?? first;
        mark.style.top = `${first.offsetTop * scale}px`;
        mark.style.height = `${Math.max(3, (last.offsetTop + last.offsetHeight - first.offsetTop) * scale)}px`;
    }
}
function trackView() {
    const body = document.getElementById('code-body');
    const bar = document.getElementById('code-ruler');
    const view = document.getElementById('code-view');
    if (!body || !bar || !view || body.scrollHeight === 0)
        return;
    const scale = bar.clientHeight / body.scrollHeight;
    view.style.top = `${body.scrollTop * scale}px`;
    view.style.height = `${Math.min(bar.clientHeight, body.clientHeight * scale)}px`;
}
/** A press on the ruler: on the band, it is dragged; anywhere else the band
 *  centres there and is dragged from it. A mark is a button of its own. */
function grab(e) {
    const body = document.getElementById('code-body');
    const bar = document.getElementById('code-ruler');
    const view = document.getElementById('code-view');
    if (!body || !bar || !view || e.button !== 0)
        return;
    if (e.target instanceof HTMLElement && e.target.classList.contains('m'))
        return;
    e.preventDefault();
    const top = bar.getBoundingClientRect().top;
    const scale = body.scrollHeight / bar.clientHeight;
    const band = view.getBoundingClientRect();
    const onBand = e.clientY >= band.top && e.clientY <= band.bottom;
    const hold = onBand ? e.clientY - band.top : band.height / 2;
    const move = (y) => {
        body.scrollTop = (y - top - hold) * scale;
    };
    if (!onBand)
        move(e.clientY);
    bar.setPointerCapture(e.pointerId);
    bar.classList.add('dragging');
    const drag = (m) => move(m.clientY);
    const drop = () => {
        bar.classList.remove('dragging');
        bar.removeEventListener('pointermove', drag);
        bar.removeEventListener('pointerup', drop);
        bar.removeEventListener('pointercancel', drop);
    };
    bar.addEventListener('pointermove', drag);
    bar.addEventListener('pointerup', drop);
    bar.addEventListener('pointercancel', drop);
}
window.addEventListener('resize', () => {
    if (state.rail.stage !== 'code')
        return;
    placeMarks();
    trackView();
});
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
            ...c.lines.flatMap((r, i) => c.comment && i === under ? [...line(r), commentBox(c.comment, ways)] : line(r)),
            ...(c.removedAtEnd > 0 ? [gone(c.removedAtEnd)] : []),
        ];
    }
    const facts = [
        c.lines.length > 0 && `Ln ${c.at}`,
        c.language,
        c.lines.length > 0 && `${c.lines.length} lines`,
    ].filter(Boolean);
    return el('section', { class: 'code-file' }, c.path &&
        el('div', { class: 'fhdr' }, el('span', { class: 'mono path' }, c.path), el('span', { class: 'grow' }), el('span', { class: 'meta' }, facts.join(' · '), (c.added > 0 || c.removed > 0) && ' · ', c.added > 0 && el('span', { class: 'fg-ok' }, `+${c.added}`), c.added > 0 && c.removed > 0 && ' ', c.removed > 0 && el('span', { class: 'fg-danger' }, `−${c.removed}`)), !c.outline &&
            el('button', { class: 'btn sm outline-show', onclick: () => click({ on: 'outline' }) }, 'Outline', kbd('O'))), el('div', { class: 'code-wrap' }, el('div', { class: 'code-body', id: 'code-body', onscroll: trackView }, body), c.lines.length > 0 && ruler(c)));
}
function outline(c) {
    return el('aside', { class: 'code-outline', 'aria-label': 'Outline' }, el('div', { class: 'ohd' }, el('span', { class: 'mlabel grow' }, 'Outline'), el('button', {
        class: 'icon sm',
        'aria-label': 'Close the outline  O',
        title: 'Close the outline  O',
        onclick: () => click({ on: 'outline' }),
    }, icon('close'))), c.symbols.length === 0
        ? el('div', { class: 'meta' }, c.path ? 'Nothing defined that the outline knows of.' : '')
        : c.symbols.map((s) => el('button', {
            class: `orow${s.on ? ' on' : ''}`,
            title: s.name,
            onclick: () => click({ on: 'go', line: s.line }),
        }, el('span', { class: 'okind' }, s.kind), el('span', { class: 't' }, s.name), s.changed ? el('span', { class: 'tchg', 'aria-label': 'changed' }) : el('span', null))), c.changesHere.length > 0 && el('div', { class: 'mlabel osec' }, 'Changes in this file'), c.changesHere.map((ch) => el('button', {
        class: `orow${ch.on ? ' on' : ''}`,
        title: ch.text,
        onclick: () => click({ on: 'go', line: ch.line }),
    }, el('span', { class: 'okind' }, String(ch.line)), el('span', { class: 't' }, ch.text), el('span', null))));
}
/** What the last redraw showed, to tell a move from a redraw. */
let drawn = { list: '', on: '', cursor: '' };
/** Where each file was scrolled, by path, and each list, by which list it
 *  is. Kept here rather than read off the page: a stage put away is hidden,
 *  and a hidden element forgets how far it was scrolled. */
const scrolledTo = { file: new Map(), list: new Map() };
function rememberScroll() {
    const c = state.code;
    const body = document.getElementById('code-body');
    const listed = document.getElementById('code-list');
    if (c?.path && body)
        scrolledTo.file.set(c.path, body.scrollTop);
    if (listed)
        scrolledTo.list.set(drawn.list, listed.scrollTop);
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
    // Each redraw builds the stage anew, so both lists keep where they were
    // scrolled, and are moved only to bring a row the person moved to into
    // view: scrolling on every redraw moved the tree from under a click.
    const list = c.search ? 'search' : c.jump ? 'jump' : 'tree';
    const hit = c.search?.files.flatMap((f) => f.hits).find((h) => h.on);
    const on = `${list}:${c.search ? (hit?.index ?? '') : (c.jump?.at ?? c.path ?? '')}`;
    const cursor = `${c.path ?? ''}:${c.at}`;
    settleDraft('code-comment', c.comment !== null);
    const typing = document.activeElement?.id;
    stage.replaceChildren(head(c), el('div', { class: 'code-split' }, side(c), file(c), c.outline && outline(c)));
    const body = document.getElementById('code-body');
    if (body) {
        body.scrollTop = scrolledTo.file.get(c.path ?? '') ?? 0;
        body.addEventListener('scroll', rememberScroll);
    }
    const listed = document.getElementById('code-list');
    if (listed) {
        listed.scrollTop = scrolledTo.list.get(list) ?? 0;
        listed.addEventListener('scroll', rememberScroll);
    }
    if (cursor !== drawn.cursor)
        stage.querySelector('.cline.cursor')?.scrollIntoView({ block: 'nearest' });
    if (on !== drawn.on)
        stage.querySelector('.code-list .on')?.scrollIntoView({ block: 'nearest' });
    drawn = { list, on, cursor };
    rememberScroll();
    placeMarks();
    trackView();
    // The field with the keys keeps them; one just opened takes them.
    const want = c.comment ? 'code-comment' : c.search ? 'code-search' : c.jump ? 'code-jump' : null;
    const keep = typing && document.getElementById(typing);
    const focus = keep && typing !== 'code-jump' ? keep : want ? document.getElementById(want) : null;
    if (focus instanceof HTMLInputElement || focus instanceof HTMLTextAreaElement) {
        if (document.activeElement !== focus)
            focus.focus();
        if (typing !== focus.id)
            focus.setSelectionRange(focus.value.length, focus.value.length);
    }
}
/** Runs a Code stage key. */
export function codeAction(action) {
    call('code_action', { action });
    return true;
}
