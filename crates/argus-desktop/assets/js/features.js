// The Feature stage: one repository's features, as a list beside a peek or
// as a board. Where a feature stands is read by Rust off the daemon's tree
// and board; a hold or an archive is asked for and shows when the pushed
// board says so. Only Held and Archived take a dropped card: the other
// columns follow the work. A feature opened from the index is `work.ts`'s,
// and a new one written as a note `capture.ts`'s; either is drawn here in
// place of the index.
import { captureOpen, drawCapture, openCapture } from './capture.js';
import { notice } from './chrome.js';
import { $, el, ghosts, icon } from './dom.js';
import { actionFor } from './keymap.js';
import { state } from './state.js';
import { call } from './tauri.js';
import { drawWork } from './work.js';
const GROUP_LABEL = {
    active: 'Active',
    queued: 'Queued',
    done: 'Done',
    held: 'Held',
    archived: 'Archived',
};
const GROUP_CLASS = {
    active: 'fg-attention',
    done: 'fg-ok',
};
const COLUMN_NOTE = {
    queued: 'written down, not started',
    done: 'every task done',
    held: 'set by you',
    archived: 'set by you',
};
function isGroup(word) {
    return word !== undefined && word in GROUP_LABEL;
}
function visibleRows() {
    const f = state.features;
    if (!f)
        return [];
    const query = state.filter.trim().toLowerCase().replace(/^#/, '');
    return f.rows.filter((row) => {
        const folded = row.group === 'held' || row.group === 'archived';
        if (f.layout === 'list' && !f.showFolded && folded)
            return false;
        if (!query)
            return true;
        return row.title.toLowerCase().includes(query) || String(row.number) === query;
    });
}
function rowBySlug(slug) {
    return state.features?.rows.find((row) => row.slug === slug);
}
function progress(row) {
    const fill = el('span', null);
    const share = row.tasksTotal ? row.tasksDone / row.tasksTotal : 0;
    fill.style.width = `${Math.round(share * 100)}%`;
    return el('span', { class: 'pbar' }, fill);
}
function tasks(row, quiet) {
    if (row.standing === 'new')
        return el('span', { class: `meta ${quiet}` }, 'new');
    if (row.tasksTotal === 0)
        return el('span', { class: `meta ${quiet}` }, 'no tasks');
    return el('span', { class: 'meta' }, progress(row), ` ${row.tasksDone}/${row.tasksTotal}`);
}
function questions(row) {
    if (row.open > 0)
        return el('span', { class: 'pill fg-attention start' }, `${row.open} open`);
    if (row.decided > 0)
        return el('span', { class: 'meta fg-ok' }, `${row.decided} decided`);
    return el('span', { class: 'meta fg-faint' }, '—');
}
function agentLine(agent) {
    return [
        el('span', { class: 'glyph' }, '◆'),
        el('span', { class: 'ttl' }, agent.title),
        el('span', { class: `st ${agent.status}` }, agent.status),
    ];
}
function after(row) {
    return el('span', { class: 'mono' }, row.after.map((n) => `#${n}`).join(', '));
}
/** What a row says under its title: why it is where it is. */
function rowNote(row) {
    if (row.held) {
        return el('span', null, el('span', { class: 'fg-faint' }, 'held: '), `“${row.held}”`);
    }
    if (row.after.length)
        return el('span', null, el('span', { class: 'fg-faint' }, 'after '), after(row));
    if (row.agent)
        return agentLine(row.agent);
    if (row.standing === 'new')
        return el('span', { class: 'fg-faint' }, 'nothing written yet');
    if (row.group === 'done') {
        return el('span', { class: 'fg-faint' }, 'every task done · archive it to accept it');
    }
    if (row.group === 'archived')
        return el('span', { class: 'fg-faint' }, 'archived');
    return null;
}
function empty(...body) {
    return el('div', { class: 'empty' }, el('div', { class: 'empty-body' }, body));
}
export function drawFeatures() {
    if (state.rail.stage !== 'feature')
        return;
    if (captureOpen()) {
        drawCapture();
        return;
    }
    const stage = $('fstage');
    const f = state.features;
    if (f?.open) {
        drawWork();
        return;
    }
    state.editing = null;
    if (!f || !f.repository) {
        stage.replaceChildren(empty(icon('folder'), el('div', { class: 'type-title' }, 'No repository'), el('div', { class: 'fg-muted' }, 'Choose one on the left.')));
        return;
    }
    const head = indexHead(f);
    if (!f.loaded) {
        stage.replaceChildren(head, ghosts([[0.5], [0.8], [0.65], [0.75], [0.55], [0.7]]));
        return;
    }
    if (!f.rows.length) {
        stage.replaceChildren(head, empty(icon('folder'), el('div', { class: 'type-title' }, `No features in ${f.repository} yet`), el('div', { class: 'fg-muted' }, 'Agents open them with ', el('span', { class: 'mono fg' }, 'argus-hook feature open'), ', or write one as a note with ', keycap('N'), '.'), el('button', { class: 'btn primary', onclick: openCapture }, 'New feature')));
        return;
    }
    stage.replaceChildren(head, f.layout === 'board' ? board() : list(f));
    dress();
}
function keycap(key) {
    return el('span', { class: 'kbd' }, key);
}
/** The key a Move to… item has, to say beside it. */
function moveKey(to) {
    if (to === 'archived' || to === 'reopen')
        return keycap('Shift A');
    if (to === 'held' || to === 'unhold')
        return keycap('H');
    return null;
}
function indexHead(f) {
    const parts = [
        f.active > 0 && el('span', { class: 'fg-attention' }, `${f.active} active`),
        f.queued > 0 && el('span', null, `${f.queued} queued`),
        f.done > 0 && el('span', { class: 'fg-ok' }, `${f.done} done`),
    ].filter((part) => part !== false);
    const counts = parts.flatMap((part, i) => (i ? [' · ', part] : [part]));
    const folded = f.held + f.archived;
    const filter = el('input', {
        class: 'field filter',
        id: 'feature-filter',
        placeholder: 'Filter by title or #',
        'aria-label': 'Filter features',
        oninput: (e) => {
            if (!(e.target instanceof HTMLInputElement))
                return;
            state.filter = e.target.value;
            drawFeatures();
            const again = document.getElementById('feature-filter');
            if (again instanceof HTMLInputElement) {
                again.focus();
                again.setSelectionRange(again.value.length, again.value.length);
            }
        },
    });
    filter.value = state.filter;
    const seg = el('div', { class: 'seg', role: 'radiogroup', 'aria-label': 'Layout' }, ['list', 'board'].map((layout) => el('button', {
        class: `segb${f.layout === layout ? ' on' : ''}`,
        role: 'radio',
        'aria-checked': String(f.layout === layout),
        onclick: () => call('set_layout', { layout }),
    }, layout === 'list' ? 'List' : 'Board')));
    return el('div', { class: `fhead index-head${f.layout === 'board' ? ' on-board' : ''}` }, el('div', { class: 'crumbs' }, el('span', { class: 'fg' }, 'Features'), el('span', { class: 'grow' }), el('span', { class: 'meta' }, keycap('N'), ' new · ', keycap('Enter'), ' open · ', keycap('V'), ' list or board · ', keycap('M'), ' move to… · ', keycap('G'), ' go to its pane')), el('div', { class: 'actions index-tools' }, el('span', { class: 'type-title' }, `Features in ${f.repository}`), el('span', { class: 'meta' }, counts), el('span', { class: 'grow' }), filter, f.layout === 'list' &&
        folded > 0 &&
        el('button', { class: 'btn', onclick: () => call('toggle_folded', {}) }, f.showFolded ? 'Fold held and archived ' : 'Show held and archived ', el('span', { class: 'fg-faint' }, String(folded))), seg, el('button', { class: 'btn primary', onclick: openCapture }, 'New feature')));
}
function list(f) {
    const rows = visibleRows();
    const section = el('section', { class: 'index-list', 'aria-label': 'Features' }, el('div', { class: 'irow label' }, el('span', null), el('span', null, 'Feature'), el('span', null, 'Tasks'), el('span', null, 'Questions'), el('span', null, 'Checkout')));
    let group = null;
    for (const row of rows) {
        if (row.group !== group) {
            group = row.group;
            section.append(el('div', { class: 'label group-label' }, el('span', { class: GROUP_CLASS[group] ?? '' }, GROUP_LABEL[group])));
        }
        const quiet = ['done', 'held', 'archived'].includes(row.group) ? 'fg-muted' : '';
        section.append(el('button', {
            class: `irow${row.peeked ? ' on' : ''}`,
            'aria-current': row.peeked ? 'true' : null,
            'data-slug': row.slug,
            onclick: () => call('peek', { slug: row.slug }),
            ondblclick: () => call('open_feature', { slug: row.slug }),
            oncontextmenu: (e) => {
                e.preventDefault();
                call('peek', { slug: row.slug });
                moveMenu(row, e.clientX, e.clientY);
            },
        }, el('span', { class: `n${row.peeked ? ' fg-accent' : ''}` }, `#${row.number}`), el('span', { class: `ttl${row.peeked ? ' strong' : ''} ${quiet}` }, row.title), tasks(row, 'fg-faint'), questions(row), el('span', { class: `mono ${quiet ? 'fg-faint' : 'fg-ref'} small-mono` }, row.branch || '—'), el('span', { class: 'sub2' }, rowNote(row))));
    }
    if (!rows.length)
        section.append(el('div', { class: 'meta group-label' }, 'Nothing matches the filter.'));
    section.append(el('div', { class: 'meta index-foot' }, "Held and archived features are folded away. A feature's standing is read off its tasks and the agents on its checkouts; only holding and archiving are set by hand."));
    return el('div', { class: 'split' }, section, peekPanel(f.peek));
}
const NEEDS_YOU = {
    mark: 'fg-attention',
    head: 'fg-attention',
    label: 'fg-attention',
    text: '',
};
const IN_FLIGHT = { mark: 'fg-attention', head: '', label: 'fg-faint', text: '' };
const NEXT_UP = { mark: 'fg-faint', head: '', label: 'fg-faint', text: '' };
const LAST_DECIDED = { mark: 'fg-ok', head: '', label: 'fg-ref', text: 'fg-muted' };
function lines(label, items, mark, tone) {
    if (!items.length)
        return null;
    return el('div', { class: 'stack-4' }, el('div', { class: `label ${tone.head}` }, label), items.map((line) => [
        el('div', null, el('span', { class: tone.mark }, `${mark} `), el('span', { class: `mono ${tone.label}` }, line.label), ' ', tone.text ? el('span', { class: tone.text }, line.text) : line.text),
        line.aside && el('div', { class: 'meta indented' }, line.aside),
    ]));
}
function peekPanel(peek) {
    if (!peek) {
        return el('aside', { class: 'peek', 'aria-label': 'Peek' }, el('div', { class: 'meta' }, 'Choose a feature to peek at it.'));
    }
    const row = rowBySlug(peek.slug);
    return el('aside', { class: 'peek', 'aria-label': `Peek at #${peek.number}` }, el('div', null, el('div', { class: 'label' }, el('span', { class: 'fg-accent' }, `#${peek.number}`), el('span', { class: GROUP_CLASS[peek.standing] ?? '' }, peek.standing), el('span', { class: 'grow' }), peek.branch && el('span', { class: 'label-aside' }, `from ${peek.branch}`)), el('div', { class: 'peek-title' }, peek.title), peek.held && el('div', { class: 'fg-attention' }, `held: “${peek.held}”`), peek.brief && el('div', { class: 'fg-muted peek-brief' }, peek.brief)), peek.links.length > 0 &&
        el('div', { class: 'actions' }, peek.links.map((link) => el('span', { class: 'track', title: link.url }, link.tracker, ' ', el('b', null, link.key)))), lines('Needs you', peek.needsYou, '?', NEEDS_YOU), lines('In flight', peek.inFlight, '◐', IN_FLIGHT), lines('Next up', peek.nextUp ? [peek.nextUp] : [], '○', NEXT_UP), lines('Last decided', peek.lastDecided ? [peek.lastDecided] : [], '✓', LAST_DECIDED), el('div', { class: 'grow' }), el('div', { class: 'actions' }, el('button', { class: 'btn primary', onclick: () => call('open_feature', { slug: peek.slug }) }, 'Open'), el('button', { class: 'btn', disabled: peek.pane == null, onclick: () => goToPane(peek.pane) }, 'Go to its pane'), el('span', { class: 'grow' }), row && moveButtons(row)));
}
function moveButtons(row) {
    return [
        row.group === 'held'
            ? el('button', { class: 'btn', onclick: () => call('hold', { slug: row.slug, reason: null }) }, 'Lift the hold')
            : el('button', { class: 'btn', onclick: () => holdDialog(row) }, 'Hold…'),
        row.group === 'archived'
            ? el('button', { class: 'btn', onclick: () => call('archive', { slug: row.slug, archived: false }) }, 'Reopen')
            : el('button', { class: 'btn', onclick: () => call('archive', { slug: row.slug, archived: true }) }, 'Archive'),
    ];
}
function board() {
    const rows = visibleRows();
    const column = (group) => {
        const cards = rows.filter((row) => row.group === group);
        const note = COLUMN_NOTE[group];
        const section = el('section', {
            class: 'kcol',
            'aria-label': `${GROUP_LABEL[group]}, ${cards.length} features`,
            'data-group': group,
        }, el('div', { class: 'khead' }, el('span', { class: GROUP_CLASS[group] ?? '' }, GROUP_LABEL[group]), el('span', { class: 'fg-faint' }, String(cards.length)), el('span', { class: 'grow' }), note && el('span', { class: 'regular meta' }, note)));
        const shown = group === 'archived' && !state.archivedOpen ? [] : cards;
        for (const row of shown)
            section.append(card(row));
        if (group === 'archived' && cards.length) {
            section.append(el('button', {
                class: 'kmore',
                onclick: () => {
                    state.archivedOpen = !state.archivedOpen;
                    drawFeatures();
                },
            }, state.archivedOpen ? 'Fold the archived' : `Show the ${cards.length} archived`));
        }
        return section;
    };
    return el('div', { class: 'board' }, column('queued'), column('active'), column('done'), el('div', { class: 'side' }, column('archived'), column('held'), el('div', { class: 'meta' }, 'Queued, Active and Done follow the work. Drag a card to Held or Archived.')));
}
function card(row) {
    // A held card's reason stands in for its questions, as the canvas draws it.
    let corner;
    if (row.group === 'held')
        corner = null;
    else if (row.after.length)
        corner = el('span', { class: 'meta' }, 'after ', after(row));
    else if (row.standing === 'new')
        corner = el('span', { class: 'pill fg-faint' }, 'new');
    else
        corner = questions(row);
    let work;
    if (row.standing === 'new')
        work = el('span', { class: 'meta' }, 'nothing written yet');
    else if (row.tasksTotal === 0)
        work = el('span', { class: 'meta' }, 'no tasks yet');
    else {
        work = el('span', { class: 'krow' }, progress(row), el('span', { class: 'meta' }, `${row.tasksDone}/${row.tasksTotal}`), el('span', { class: 'grow' }), row.branch && el('span', { class: 'mono fg-ref small-mono' }, row.branch));
    }
    const node = el('button', {
        class: `kcard${row.peeked ? ' on' : ''}`,
        'data-slug': row.slug,
        onclick: () => {
            if (!drag.justDropped)
                call('peek', { slug: row.slug });
        },
        oncontextmenu: (e) => {
            e.preventDefault();
            call('peek', { slug: row.slug });
            moveMenu(row, e.clientX, e.clientY);
        },
        onpointerdown: (e) => {
            // Measured now: a board pushed before the drag starts redraws the
            // card, and the old element measures as nowhere.
            if (e.button === 0) {
                drag.press = {
                    slug: row.slug,
                    x: e.clientX,
                    y: e.clientY,
                    box: node.getBoundingClientRect(),
                };
            }
        },
    }, el('span', { class: 'krow' }, el('span', { class: `n${row.peeked ? ' fg-accent' : ''}` }, `#${row.number}`), el('span', { class: 'grow' }), corner), el('span', { class: 'kttl' }, row.title), work, row.agent && el('span', { class: 'krow meta' }, agentLine(row.agent)), row.held && el('span', { class: 'meta' }, `held: “${row.held}”`));
    return node;
}
// ---- Dragging a card -------------------------------------------------------
//
// By pointer events rather than HTML drag and drop: one behaviour in every
// webview (WebKitGTK's drop does not reliably arrive), and the card under
// the pointer is drawn as the canvas draws it.
const drag = { press: null, ghost: null, offset: { x: 0, y: 0 }, over: null, justDropped: false };
function cardNode(slug) {
    return document.querySelector(`.kcard[data-slug="${CSS.escape(slug)}"]`);
}
function groupAt(x, y) {
    const column = document.elementFromPoint(x, y)?.closest('.kcol[data-group]');
    const group = column?.dataset.group;
    return isGroup(group) ? group : null;
}
/** Whether a dropped card asks for anything: only a move the row offers and
 * a person sets. The columns that follow the work light up for nothing, as
 * on the canvas, and say why once the card lands on them. */
function takes(row, group) {
    return ((group === 'held' || group === 'archived') &&
        row.moves.some((move) => move.to === group && !move.disabled));
}
function dropWords(row, group) {
    return group === 'archived'
        ? `Drop to archive: you accept #${row.number} as finished`
        : `Drop to hold: you say why #${row.number} waits`;
}
/** Marks the drag on the board as drawn now: the card's place keeps a
 * placeholder, and a column that takes it says what the drop asks. Run after
 * every draw too, since a board pushed mid-drag is drawn plain; the card
 * itself moves only when a pushed board says it did. */
function dress() {
    const press = drag.press;
    const row = press && drag.ghost ? rowBySlug(press.slug) : undefined;
    if (!row)
        return;
    const target = drag.over && takes(row, drag.over) ? drag.over : null;
    const words = `#${row.number} is being dragged${target ? ` to ${GROUP_LABEL[target]}` : ''}`;
    const place = document.querySelector('.ghostcard') ?? cardNode(row.slug);
    place?.replaceWith(el('div', { class: 'kcard ghostcard', 'aria-hidden': 'true' }, el('span', { class: 'meta' }, words)));
    for (const column of document.querySelectorAll('.kcol[data-group]')) {
        const on = target !== null && column.dataset.group === target;
        column.classList.toggle('drop', on);
        column.querySelector('.dropzone')?.remove();
        if (on) {
            column
                .querySelector('.khead')
                ?.after(el('div', { class: 'dropzone' }, dropWords(row, target)));
        }
    }
}
function onPointerMove(e) {
    const press = drag.press;
    if (!press)
        return;
    if (!drag.ghost) {
        if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 6)
            return;
        const node = cardNode(press.slug);
        if (!node)
            return;
        const ghost = node.cloneNode(true);
        if (!(ghost instanceof HTMLElement))
            return;
        ghost.classList.remove('on');
        ghost.classList.add('dragging', 'ghost-card');
        ghost.style.width = `${press.box.width}px`;
        drag.offset = { x: press.x - press.box.left, y: press.y - press.box.top };
        document.body.append(ghost);
        drag.ghost = ghost;
        drag.over = null;
        dress();
    }
    drag.ghost.style.left = `${e.clientX - drag.offset.x}px`;
    drag.ghost.style.top = `${e.clientY - drag.offset.y}px`;
    const over = groupAt(e.clientX, e.clientY);
    if (over !== drag.over) {
        drag.over = over;
        dress();
    }
}
function onPointerUp(e) {
    const press = drag.press;
    drag.press = null;
    if (!press || !drag.ghost)
        return;
    const group = groupAt(e.clientX, e.clientY);
    drag.ghost.remove();
    drag.ghost = null;
    drag.over = null;
    // Back where the last board put it: a drop that is taken moves the card
    // when the daemon's board says so.
    drawFeatures();
    // The click that follows the release is the drag's, not a peek.
    drag.justDropped = true;
    setTimeout(() => {
        drag.justDropped = false;
    }, 0);
    const row = rowBySlug(press.slug);
    if (row && group)
        drop(row, group);
}
/** A card dropped on a column. Only the two a person sets take it; the
 * rest say why they follow the work instead. */
function drop(row, group) {
    if (group === row.group)
        return;
    if (takes(row, group)) {
        if (group === 'held')
            holdDialog(row);
        else
            call('archive', { slug: row.slug, archived: true });
        return;
    }
    const move = row.moves.find((m) => m.to === group);
    notice(`${GROUP_LABEL[group]} follows the work${move?.disabled ? `: ${move.disabled}` : ''}.`);
}
// ---- Menus and the hold dialog --------------------------------------------
export function closeLayer() {
    $('layer').replaceChildren();
}
function moveMenu(row, x, y) {
    const agent = row.agent;
    const menu = el('div', { class: 'menu', role: 'menu', 'aria-label': `#${row.number} ${row.title}` }, el('div', { class: 'mlabel' }, `#${row.number} ${row.title}`), agent &&
        el('button', {
            class: 'mi',
            role: 'menuitem',
            onclick: () => {
                closeLayer();
                goToPane(agent.pane);
            },
        }, 'Go to its pane', el('span', { class: 'grow' }), keycap('G')), agent && el('div', { class: 'msep' }), el('div', { class: 'mlabel' }, 'Move to'), row.moves.map((move) => el('button', {
        class: 'mi',
        role: 'menuitem',
        disabled: !!move.disabled,
        onclick: () => {
            closeLayer();
            if (move.to === 'held')
                holdDialog(row);
            else if (move.to === 'unhold')
                call('hold', { slug: row.slug, reason: null });
            else if (move.to === 'archived')
                call('archive', { slug: row.slug, archived: true });
            else if (move.to === 'reopen')
                call('archive', { slug: row.slug, archived: false });
        },
    }, move.disabled === 'here now' && el('span', { class: 'fg-ok' }, '✓'), move.label, el('span', { class: 'grow' }), move.disabled ? el('span', { class: 'meta' }, move.disabled) : moveKey(move.to))));
    $('layer').replaceChildren(menu);
    const box = menu.getBoundingClientRect();
    menu.style.left = `${Math.min(x, window.innerWidth - box.width - 8)}px`;
    menu.style.top = `${Math.min(y, window.innerHeight - box.height - 8)}px`;
    menu.querySelector('.mi:not([disabled])')?.focus();
}
function holdDialog(row) {
    const reason = el('input', {
        class: 'field',
        id: 'hold-reason',
        placeholder: 'until the ratatui bump lands',
    });
    const hold = () => {
        call('hold', { slug: row.slug, reason: reason.value });
        closeLayer();
    };
    reason.addEventListener('keydown', (e) => {
        const action = actionFor(['writing'], e);
        if (action === 'choose')
            hold();
        if (action === 'closeLayer')
            closeLayer();
    });
    const dialog = el('div', {
        class: 'scrim',
        onclick: (e) => {
            if (e.target === e.currentTarget)
                closeLayer();
        },
    }, el('section', {
        class: 'dialog hold-dialog',
        role: 'dialog',
        'aria-modal': 'true',
        'aria-labelledby': 'hold-title',
    }, el('div', { class: 'dhead' }, el('div', { class: 'type-title', id: 'hold-title' }, `Hold #${row.number} ${row.title}`)), el('div', { class: 'dbody' }, el('label', { class: 'label', for: 'hold-reason' }, 'Why is it held?'), reason, el('div', { class: 'meta' }, 'Agents read the reason.')), el('div', { class: 'dfoot' }, el('span', { class: 'grow' }), el('button', { class: 'btn', onclick: closeLayer }, 'Cancel'), el('button', { class: 'btn primary', onclick: hold }, 'Hold'))));
    $('layer').replaceChildren(dialog);
    reason.focus();
}
function goToPane(pane) {
    if (pane != null)
        call('go_to_pane', { pane });
}
// ---- Keys --------------------------------------------------------------------
/** Runs a Feature or Board card binding, saying whether it had a feature to act on. */
export function featureAction(action) {
    const rows = visibleRows();
    const at = rows.findIndex((row) => row.peeked);
    const row = rows[at];
    const peekAt = (i) => {
        const next = rows[Math.max(0, Math.min(rows.length - 1, i))];
        if (next)
            call('peek', { slug: next.slug });
    };
    switch (action) {
        case 'next':
            peekAt(at + 1);
            return true;
        case 'previous':
            peekAt(at - 1);
            return true;
        case 'first':
            peekAt(0);
            return true;
        case 'last':
            peekAt(rows.length - 1);
            return true;
        case 'layout':
            call('set_layout', { layout: state.features?.layout === 'board' ? 'list' : 'board' });
            return true;
        case 'folded':
            call('toggle_folded', {});
            return true;
        case 'filter':
            document.getElementById('feature-filter')?.focus();
            return true;
        case 'newFeature':
            openCapture();
            return true;
        default:
            break;
    }
    if (!row)
        return false;
    switch (action) {
        case 'move': {
            const box = document
                .querySelector(`[data-slug="${CSS.escape(row.slug)}"]`)
                ?.getBoundingClientRect();
            moveMenu(row, box ? box.left + 40 : 200, box ? box.bottom : 200);
            return true;
        }
        case 'hold':
            if (row.group === 'held')
                call('hold', { slug: row.slug, reason: null });
            else
                holdDialog(row);
            return true;
        case 'archive':
            call('archive', { slug: row.slug, archived: row.group !== 'archived' });
            return true;
        case 'goToPane':
            goToPane(row.agent?.pane ?? state.features?.peek?.pane);
            return true;
        case 'choose':
            call('open_feature', { slug: row.slug });
            return true;
        default:
            return false;
    }
}
export function listen() {
    document.addEventListener('pointermove', onPointerMove);
    document.addEventListener('pointerup', onPointerUp);
    document.addEventListener('mousedown', (e) => {
        const layer = $('layer');
        if (layer.querySelector('.menu') && e.target instanceof Node && !layer.contains(e.target)) {
            closeLayer();
        }
    });
}
