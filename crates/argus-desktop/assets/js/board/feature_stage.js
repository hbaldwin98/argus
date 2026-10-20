// The Feature stage: one repository's features, as a list beside a peek or
// as a board. Where a feature stands is read by Rust off the daemon's tree
// and board; a hold or an archive is asked for and shows when the pushed
// board says so. Only Held and Archived take a dropped card: the other
// columns follow the work. A feature opened from the index is
// `lenses/work.ts`'s, and a new one written as a note `capture.ts`'s;
// either is drawn here in place of the index. Which features are listed,
// in what order, and where the cursor steps are Rust's
// (`src/board/features.rs`, `src/board/feature_stage.rs`); this draws
// them and forwards the keys.
import { captureOpen, drawCapture, openCapture } from './capture.js';
import { notice } from '../chrome.js';
import { closeExpanded } from './lenses/sequence.js';
import { $, closeButton, el, escKey, ghosts, icon, standingPill, statusPill, } from '../kit/dom.js';
import { closeLayer, placeMenu } from '../kit/menu.js';
import { ticketLinks } from './tracker/tickets.js';
import { actionFor } from '../keys/keymap.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
import { drawWork } from './lenses/work.js';
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
// What an empty column says: the words are in the column, not in a note
// under the board, so a column that follows the work says what lands in it.
const COLUMN_EMPTY = {
    queued: ['Nothing queued', 'Written-down features wait here'],
    active: ['Nothing active', 'A feature moves here when work starts'],
    done: ['Nothing done yet', 'Lands here when every task is done'],
    held: ['Nothing held', 'Drag a card here to set it aside'],
    archived: ['Nothing archived', 'Drag a card here to accept it'],
};
function isGroup(word) {
    return word !== undefined && word in GROUP_LABEL;
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
        return el('span', { class: 'meta fg-attention' }, `${row.open} open`);
    if (row.decided > 0)
        return el('span', { class: 'meta fg-ok' }, `${row.decided} decided`);
    return null;
}
function agentLine(agent) {
    return [el('span', { class: 'ttl' }, agent.title), statusPill(agent.status)];
}
function after(row) {
    return el('span', { class: 'mono' }, row.after.map((n) => `#${n}`).join(', '));
}
/** What a row says under its title: why it is where it is, or what comes
 *  next and how its agent is doing. */
function rowNote(row) {
    if (row.held) {
        return el('span', null, el('span', { class: 'fg-faint' }, 'held: '), `“${row.held}”`);
    }
    if (row.after.length)
        return el('span', null, el('span', { class: 'fg-faint' }, 'after '), after(row));
    const next = row.next &&
        el('span', { class: 'next' }, el('span', { class: 'fg-faint' }, 'next '), el('span', { class: 'mono fg-faint' }, row.next.label), ` ${row.next.text}`);
    if (next || row.agent)
        return [next, next && row.agent && ' · ', row.agent && agentLine(row.agent)];
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
/** Puts back the filter as typed once the index is redrawn, while the field
 *  has the keys: the echo being drawn may be older than the last keystroke. */
function keepTyped() {
    const field = document.activeElement;
    if (!(field instanceof HTMLInputElement) || field.id !== 'feature-filter')
        return () => { };
    const { value, selectionStart, selectionEnd } = field;
    return () => {
        const again = document.getElementById('feature-filter');
        if (!(again instanceof HTMLInputElement))
            return;
        again.value = value;
        again.focus();
        again.setSelectionRange(selectionStart, selectionEnd);
    };
}
export function drawFeatures() {
    if (state.rail.stage !== 'feature') {
        closeExpanded();
        return;
    }
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
    closeExpanded();
    if (!f || !f.repository) {
        stage.replaceChildren(empty(icon('folder'), el('div', { class: 'type-title' }, 'No repository'), el('div', { class: 'fg-muted' }, 'Choose one on the left.')));
        return;
    }
    const typed = keepTyped();
    const head = indexHead(f);
    if (!f.loaded) {
        stage.replaceChildren(head, ghosts([[0.5], [0.8], [0.65], [0.75], [0.55], [0.7]]));
        typed();
        return;
    }
    if (f.active + f.queued + f.done + f.held + f.archived === 0) {
        stage.replaceChildren(head, empty(icon('folder'), el('div', { class: 'type-title' }, `No features in ${f.repository} yet`), el('div', { class: 'fg-muted' }, 'Agents open them with ', el('span', { class: 'mono fg' }, 'argus-hook feature open'), ', or write one as a note with ', keycap('N'), '.'), el('button', { class: 'btn primary', onclick: openCapture }, 'New feature')));
        typed();
        return;
    }
    stage.replaceChildren(head, f.layout === 'board' ? board(f) : list(f), keyBar(f));
    typed();
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
/** What the strip carries on the Features stage: the counts beside the
 *  title, and the layout switch and the primary action at the right. */
export function featuresStrip() {
    const f = state.features;
    if (!f?.repository || f.open)
        return null;
    const parts = [
        f.active > 0 && el('span', { class: 'fg-attention' }, `${f.active} active`),
        f.queued > 0 && el('span', null, `${f.queued} queued`),
        f.done > 0 && el('span', { class: 'fg-ok' }, `${f.done} done`),
    ].filter((part) => part !== false);
    const counts = parts.flatMap((part, i) => (i ? [' · ', part] : [part]));
    const seg = el('div', { class: 'seg', role: 'radiogroup', 'aria-label': 'Layout' }, ['list', 'board'].map((layout) => el('button', {
        class: `segb${f.layout === layout ? ' on' : ''}`,
        role: 'radio',
        'aria-checked': String(f.layout === layout),
        onclick: () => call('set_layout', { layout }),
    }, layout === 'list' ? 'List' : 'Board')));
    return {
        // The keys are in the counts' tooltip rather than a row of their own.
        meta: [
            el('span', {
                class: 'meta head-meta',
                title: 'N new · Enter open · V list or board · M move to… · G go to its pane',
            }, counts),
        ],
        tools: [seg, el('button', { class: 'btn primary', onclick: openCapture }, 'New feature')],
    };
}
function indexHead(f) {
    const filter = el('input', {
        class: 'field filter',
        id: 'feature-filter',
        placeholder: 'Filter by title or #',
        'aria-label': 'Filter features',
        oninput: (e) => {
            if (e.target instanceof HTMLInputElement)
                call('filter_features', { filter: e.target.value });
        },
    });
    filter.value = f.filter;
    return el('div', { class: `fhead index-head${f.layout === 'board' ? ' on-board' : ''}` }, el('div', { class: 'actions index-tools' }, filter, f.layout === 'list' &&
        f.folded > 0 &&
        el('button', { class: 'btn', onclick: () => call('feature_action', { action: 'folded' }) }, f.showFolded ? 'Fold held and archived ' : 'Show held and archived ', el('span', { class: 'fg-faint' }, String(f.folded)))));
}
/** The keys the index answers to, quietly under it; the board adds how a
 *  card is set aside. */
function keyBar(f) {
    const keys = [
        ['N', 'new'],
        ['Enter', 'open'],
        ['V', f.layout === 'board' ? 'list' : 'board'],
        ['M', 'move'],
        ['G', 'go to pane'],
    ];
    return el('div', { class: 'kbar' }, keys.map(([key, what]) => el('span', { class: 'kbar-key' }, keycap(key), ` ${what}`)), el('span', { class: 'grow' }), f.layout === 'board' && el('span', null, 'Drag a card to Held to set it aside'));
}
function list(f) {
    const rows = f.rows;
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
        }, el('span', { class: `n${row.peeked ? ' fg-accent' : ''}` }, `#${row.number}`), el('span', { class: `ttl${row.peeked ? ' strong' : ''} ${quiet}` }, row.title), tasks(row, 'fg-faint'), el('span', null, questions(row)), el('span', { class: `mono ${quiet ? 'fg-faint' : 'fg-ref'} small-mono cut` }, row.branch), el('span', { class: 'sub2' }, rowNote(row))));
    }
    if (!rows.length) {
        let why = 'Every feature here is held or archived.';
        if (f.filter && f.folded === 0)
            why = `Nothing matches “${f.filter}”.`;
        else if (f.filter && !f.showFolded)
            why = `Only held or archived features match “${f.filter}”.`;
        section.append(el('div', { class: 'meta group-label' }, why));
    }
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
const AGENT_MARK = '◆';
function lines(label, items, mark, tone) {
    if (!items.length)
        return null;
    return el('div', { class: 'stack-4' }, el('div', { class: `label ${tone.head}` }, label), items.map((line) => [
        // A pane that needs you comes as "◆" with its status as the aside
        // (`board/features.rs`): one row of icon, title and the shared pill,
        // without the "?" that belongs to an open decision.
        line.label === AGENT_MARK
            ? el('div', { class: 'agent-line' }, el('span', { class: tone.mark }, icon('console')), el('span', { class: 'cut' }, line.text), line.aside && statusPill(line.aside))
            : el('div', null, el('span', { class: tone.mark }, `${mark} `), el('span', { class: `mono ${tone.label}` }, line.label), ' ', tone.text ? el('span', { class: tone.text }, line.text) : line.text),
        line.label !== AGENT_MARK && line.aside && el('div', { class: 'meta indented' }, line.aside),
    ]));
}
function peekPanel(peek) {
    if (!peek) {
        return el('aside', { class: 'peek', 'aria-label': 'Peek' }, el('div', { class: 'meta' }, 'Choose a feature to peek at it.'));
    }
    const row = rowBySlug(peek.slug);
    return el('aside', { class: 'peek', 'aria-label': `Peek at #${peek.number}` }, el('div', null, el('div', { class: 'label' }, el('span', { class: 'fg-accent' }, `#${peek.number}`), standingPill(peek.standing), el('span', { class: 'grow' }), peek.branch && el('span', { class: 'label-aside' }, `from ${peek.branch}`)), el('div', { class: 'peek-title' }, peek.title), peek.held && el('div', { class: 'fg-attention' }, `held: “${peek.held}”`), peek.brief && el('div', { class: 'fg-muted peek-brief' }, peek.brief)), ticketLinks(peek.links), lines('Needs you', peek.needsYou, '?', NEEDS_YOU), lines('In flight', peek.inFlight, '◐', IN_FLIGHT), lines('Next up', peek.nextUp ? [peek.nextUp] : [], '○', NEXT_UP), lines('Last decided', peek.lastDecided ? [peek.lastDecided] : [], '✓', LAST_DECIDED), el('div', { class: 'grow' }), el('div', { class: 'actions' }, el('button', { class: 'btn primary', onclick: () => call('open_feature', { slug: peek.slug }) }, 'Open'), el('button', { class: 'btn', disabled: peek.pane == null, onclick: () => goToPane(peek.pane) }, 'Go to its pane'), el('span', { class: 'grow' }), row &&
        el('button', {
            class: 'icon',
            'aria-label': 'More: hold, archive, order',
            title: 'Hold, archive, or order it after another',
            onclick: (e) => {
                const at = e.currentTarget.getBoundingClientRect();
                moveMenu(row, at.left, at.bottom, peek);
            },
        }, '⋯')));
}
/** The features the peeked one may come after, ticked where it does
 *  already; choosing one says so, or takes it back. */
function afterMenu(peek, x, y) {
    const menu = el('div', { class: 'menu', role: 'menu', 'aria-label': `#${peek.number} comes after` }, el('div', { class: 'mlabel' }, `#${peek.number} comes after…`), peek.after.map((choice) => el('button', {
        class: 'mi',
        role: 'menuitemcheckbox',
        'aria-checked': String(choice.waits),
        onclick: () => {
            closeLayer();
            call('wait', { slug: peek.slug, on: choice.slug, waits: !choice.waits });
        },
    }, el('span', { class: choice.waits ? 'fg-ok' : 'fg-faint' }, choice.waits ? '✓' : ' '), el('span', { class: 'mono fg-faint' }, `#${choice.number}`), el('span', { class: 'grow' }, choice.title), choice.waits && el('span', { class: 'meta' }, 'take back'))));
    placeMenu(menu, { x, y });
}
function board(f) {
    const column = (group) => {
        const cards = f.rows.filter((row) => row.group === group);
        // A folded Archived column still counts what it folds.
        const folded = group === 'archived' && !f.showFolded;
        const count = folded ? f.folded : cards.length;
        const section = el('section', {
            class: `kcol${folded ? ' folded' : ''}`,
            'aria-label': `${GROUP_LABEL[group]}, ${count} features`,
            'data-group': group,
        });
        if (folded) {
            // One row the width of a column, so it still takes a dropped card.
            section.append(el('button', {
                class: 'kfold',
                'aria-expanded': 'false',
                onclick: () => call('feature_action', { action: 'folded' }),
            }, icon('archive'), el('span', null, GROUP_LABEL[group]), el('span', { class: 'kcount' }, String(count)), el('span', { class: 'grow' }), icon('right')));
            return section;
        }
        section.append(el('div', { class: 'khead' }, el('span', { class: `kdot ${group}`, 'aria-hidden': 'true' }), el('span', null, GROUP_LABEL[group]), el('span', { class: 'kcount' }, String(count))));
        for (const row of cards)
            section.append(card(row));
        if (!cards.length) {
            const [title, why] = COLUMN_EMPTY[group];
            section.append(el('div', { class: 'kempty' }, group === 'done' && icon('settled'), el('div', { class: 'kempty-title' }, title), el('div', { class: 'fg-faint' }, why)));
        }
        if (group === 'archived') {
            section.append(el('button', { class: 'kmore', onclick: () => call('feature_action', { action: 'folded' }) }, 'Fold the archived'));
        }
        return section;
    };
    const lanes = el('div', { class: 'board' }, column('queued'), column('active'), column('done'), el('div', { class: 'side' }, column('held'), column('archived')));
    return withScrollCue(lanes);
}
/** Columns keep a floor width, so a narrow window scrolls the board; this
 *  says so, with a fade at the edge and a chip naming what is off to the
 *  right, so a column is never lost without a sign. */
function withScrollCue(lanes) {
    const chip = el('button', { class: 'board-more', type: 'button', hidden: true });
    const wrap = el('div', { class: 'board-wrap' }, lanes, el('div', { class: 'board-fade' }), chip);
    chip.addEventListener('click', () => lanes.scrollTo({ left: lanes.scrollWidth, behavior: 'smooth' }));
    const update = () => {
        const edge = lanes.getBoundingClientRect().right;
        const hidden = [...lanes.querySelectorAll('.kcol')].filter((col) => col.getBoundingClientRect().right > edge + 1);
        const more = lanes.scrollLeft + lanes.clientWidth < lanes.scrollWidth - 4;
        wrap.classList.toggle('more', more);
        chip.hidden = !more || hidden.length === 0;
        chip.replaceChildren(el('b', null, `${hidden.length} more`), el('span', null, hidden.map((col) => GROUP_LABEL[col.dataset.group]).join(' · ')), icon('right'));
    };
    lanes.addEventListener('scroll', update, { passive: true });
    // Measured once it is in the page: before that every column is nowhere.
    // A column cut by the edge counts as more, not only one wholly off.
    new ResizeObserver(update).observe(lanes);
    return wrap;
}
/** A card's footer on one line: the account is cut short with an ellipsis and
 *  the time after the last dot keeps its own span, so 'ago' never wraps alone. */
function footer(text) {
    const at = text.lastIndexOf(' · ');
    const when = at >= 0 && text.endsWith('ago') ? text.slice(at + 3) : null;
    return el('span', { class: 'kfoot' }, el('span', { class: 'kfoot-what' }, when === null ? text : text.slice(0, at)), when !== null && el('span', { class: 'kfoot-when' }, `· ${when}`));
}
function card(row) {
    let corner;
    if (row.agent)
        corner = statusPill(row.agent.status);
    else if (row.after.length)
        corner = el('span', { class: 'ktag' }, '→ after ', after(row));
    else if (row.standing === 'new')
        corner = el('span', { class: 'ktag' }, 'new');
    else
        corner = null;
    let work;
    if (row.standing === 'new')
        work = el('span', { class: 'meta' }, 'nothing written yet');
    else if (row.tasksTotal === 0)
        work = el('span', { class: 'meta' }, 'no tasks yet');
    else {
        work = el('span', { class: 'krow' }, progress(row), el('span', { class: 'meta kcount-text' }, `${row.tasksDone}/${row.tasksTotal}`), el('span', { class: 'grow' }), questions(row));
    }
    const node = el('button', {
        class: `kcard${row.peeked ? ' on' : ''}`,
        'data-slug': row.slug,
        onclick: () => {
            if (!drag.justDropped)
                call('peek', { slug: row.slug });
        },
        ondblclick: () => {
            if (!drag.justDropped)
                call('open_feature', { slug: row.slug });
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
    }, el('span', { class: 'krow' }, el('span', { class: 'n' }, `#${row.number}`), row.branch && el('span', { class: 'mono small-mono fg-ref cut' }, row.branch), el('span', { class: 'grow' }), corner), el('span', { class: 'kttl' }, row.title), work, row.held &&
        el('span', { class: 'khold' }, el('span', { class: 'pause', 'aria-hidden': 'true' }), row.held), footer(row.footer));
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
        runMove(row, group);
        return;
    }
    const move = row.moves.find((m) => m.to === group);
    notice(`${GROUP_LABEL[group]} follows the work${move?.disabled ? `: ${move.disabled}` : ''}.`);
}
// ---- Menus and the hold dialog --------------------------------------------
function moveMenu(row, x, y, peek) {
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
            runMove(row, move.to);
        },
    }, move.disabled === 'here now' && el('span', { class: 'fg-ok' }, '✓'), move.label, el('span', { class: 'grow' }), move.disabled ? el('span', { class: 'meta' }, move.disabled) : moveKey(move.to))), peek &&
        peek.after.length > 0 && [
        el('div', { class: 'msep' }),
        el('button', {
            class: 'mi',
            role: 'menuitem',
            onclick: () => {
                closeLayer();
                afterMenu(peek, x, y);
            },
        }, 'Comes after…'),
    ]);
    placeMenu(menu, { x, y });
}
/** Runs one of a row's moves that a person sets; a hold asks why first. */
function runMove(row, to) {
    if (to === 'held')
        holdDialog(row);
    else if (to === 'unhold')
        call('hold', { slug: row.slug, reason: null });
    else if (to === 'archived')
        call('archive', { slug: row.slug, archived: true });
    else if (to === 'reopen')
        call('archive', { slug: row.slug, archived: false });
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
        class: 'dialog',
        role: 'dialog',
        'aria-modal': 'true',
        'aria-labelledby': 'hold-title',
    }, el('div', { class: 'dhead' }, el('div', { class: 'type-title grow', id: 'hold-title' }, `Hold #${row.number} ${row.title}`), closeButton(closeLayer)), el('div', { class: 'dbody prose' }, el('label', { class: 'label', for: 'hold-reason' }, 'Why is it held?'), reason), el('div', { class: 'dfoot' }, el('span', { class: 'dwhy grow' }, 'Agents read the reason.'), el('button', { class: 'btn', onclick: closeLayer }, 'Cancel ', escKey()), el('button', { class: 'btn primary', onclick: hold }, 'Hold'))));
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
    switch (action) {
        case 'next':
        case 'previous':
        case 'first':
        case 'last':
        case 'layout':
        case 'folded':
            call('feature_action', { action });
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
    const peek = state.features?.peek;
    const row = peek ? rowBySlug(peek.slug) : undefined;
    if (!peek || !row)
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
        case 'archive': {
            const kinds = action === 'hold' ? ['held', 'unhold'] : ['archived', 'reopen'];
            const move = row.moves.find((m) => kinds.includes(m.to));
            if (move)
                runMove(row, move.to);
            return true;
        }
        case 'goToPane':
            goToPane(peek.pane);
            return true;
        case 'choose':
            call('open_feature', { slug: peek.slug });
            return true;
        default:
            return false;
    }
}
export function listen() {
    document.addEventListener('pointermove', onPointerMove);
    document.addEventListener('pointerup', onPointerUp);
}
