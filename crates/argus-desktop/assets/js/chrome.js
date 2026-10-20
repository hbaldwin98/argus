// The window's frame around the stage: the sidebar and its nav, the strip's
// words, the sidebar's foot, the empty stage, and notices that pass.
import { drawCheckouts } from './checkouts/stage.js';
import { checkoutsStrip } from './checkouts/stage.js';
import { featuresStrip } from './board/feature_stage.js';
import { codeStrip, drawCode } from './code/stage.js';
import { clock } from './kit/clock.js';
import { drawComposer } from './agents/compose.js';
import { drawConsole } from './panes/console.js';
import { $, el, icon, statusWord } from './kit/dom.js';
import { drawExtensionNav, drawExtensionPage, extensionPageTitle } from './extensions/views.js';
import { drawSavedNav } from './views/saved.js';
import { poppedViewHead, poppedViewTitle } from './views/agent.js';
import { drawFeatures } from './board/feature_stage.js';
import { chordsFor, keycaps, presses } from './keys/chords.js';
import { contextMenu, openMenu } from './kit/menu.js';
import { closesOnMiddle } from './kit/middle.js';
import { drawHeading, drawRemoving } from './chrome/projects.js';
import { renameDialog } from './panes/rename.js';
import { screen } from './panes/screen.js';
import { dragFromRail, drawSplit } from './panes/split.js';
import { openSettings } from './settings/preferences.js';
import { dragEdge } from './layout/edge.js';
import { state } from './state.js';
import { featureEdges } from './board/lenses/work.js';
import { codeEdges } from './code/stage.js';
import { call } from './connection/tauri.js';
const LOUD = new Set(['waiting', 'needs-review', 'failed']);
export function drawChrome() {
    const stopped = state.status.connection === 'stopped';
    railWidth(state.rail.width);
    featureEdges(state.rail.outlineWidth, state.rail.briefHeight);
    codeEdges(state.rail.codeTreeWidth, state.rail.codeOutlineWidth);
    const stage = state.rail.stage;
    $('fstage').hidden = stage !== 'feature';
    $('cstage').hidden = stage !== 'checkouts';
    $('kstage').hidden = stage !== 'code';
    $('xstage').hidden = stage !== 'extension';
    $('vstage').hidden = stage !== 'view';
    $('body').hidden = !state.rail.workspaceShown;
    drawFeatures();
    drawCheckouts();
    drawCode();
    drawExtensionPage();
    drawComposer();
    drawSplit();
    drawConsole();
    drawNav();
    drawHeading();
    drawRemoving();
    drawRail();
    drawHead();
    drawBar();
    drawStage();
    // What the window last heard, dimmed: the heading, the rail and the
    // pane's header as much as its screen.
    for (const id of ['heading', 'rail', 'head'])
        $(id).classList.toggle('held', stopped);
    $('stopped').hidden = !stopped;
}
/** The sidebar's way between stages, each with its key from the keymap,
 *  and Search with the palette's; then the extensions' views, a fold of
 *  their own after the built-in stages. */
function drawNav() {
    const stage = state.rail.stage;
    for (const [name, action] of [
        ['new', 'compose'],
        ['workspace', 'workspace'],
        ['feature', 'feature'],
        ['checkouts', 'checkouts'],
        ['code', 'code'],
        ['console', 'console'],
    ]) {
        const nav = $(`nav-${name}`);
        nav.classList.toggle('on', stage === name);
        if (stage === name)
            nav.setAttribute('aria-current', 'page');
        else
            nav.removeAttribute('aria-current');
        $(`nav-${name}-keys`).replaceChildren(keycaps(chordsFor(action, 'anywhere')));
    }
    $('nav-search-keys').replaceChildren(keycaps(chordsFor('palette', 'anywhere')));
    drawExtensionNav();
    drawSavedNav();
}
const STAGE_NAME = {
    feature: 'Features',
    checkouts: 'Checkouts',
    code: 'Code',
    new: 'New',
    console: 'Console',
    extension: 'Extensions',
    view: 'View',
};
/** The repository the Feature and Checkouts stages read. */
function chosenRepository() {
    const { project, repository } = state.rail;
    return project?.repositories.find((repo) => repo.id === repository)?.name ?? null;
}
function statusColour(word) {
    if (word && LOUD.has(word))
        return 'var(--attention)';
    if (word === 'working')
        return 'var(--accent)';
    return null;
}
/** The few statuses a pane row says in words beside its dot: only what asks
 *  for a look, or that it has ended. Working shows in the dot alone. */
const PANE_SAYS = new Set(['waiting', 'needs-review', 'failed', 'exited', 'done']);
function drawRail() {
    const rail = $('rail');
    const { project, loaded } = state.rail;
    // Off the Workspace the stage's own nav item is the selection, so the
    // pane on screen is marked more quietly.
    rail.classList.toggle('offstage', state.rail.stage !== 'workspace');
    if (!loaded) {
        // The stage is the one loading surface; the rail only says what it waits on.
        rail.replaceChildren(el('div', { class: 'rail-wait' }, 'Waiting for argusd'));
        return;
    }
    if (!project) {
        rail.replaceChildren(el('div', { class: 'empty-rail' }, 'No projects yet.'));
        return;
    }
    const rows = [];
    for (const repo of project.repositories) {
        rows.push(repoRow(repo));
        for (const co of repo.checkouts) {
            rows.push(checkoutRow(repo.name, co));
            if (co.panes.length > 0) {
                rows.push(el('div', { class: 'nest' }, co.panes.map((pane) => paneRow(co.id, pane))));
            }
        }
    }
    rail.replaceChildren(...rows);
    // A long tree puts the pane on screen below the fold.
    const on = rail.querySelector('.pane.on');
    const shownPane = state.rail.shown?.pane ?? null;
    if (on && shownPane !== scrolledTo)
        on.scrollIntoView({ block: 'nearest' });
    scrolledTo = shownPane;
}
/** The pane last scrolled to: once per pane, not per redraw, so a person
 *  reading the rail is not fought. */
let scrolledTo = null;
function repoRow(repo) {
    const colour = statusColour(repo.status);
    const statusDot = colour ? el('span', { class: 'rdot' }) : null;
    if (statusDot && colour)
        statusDot.style.background = colour;
    const chosen = repo.id === state.rail.repository;
    return el('div', {
        class: `repo${repo.quiet ? ' quiet' : ''}${chosen ? ' sel' : ''}`,
        // The folded rail keeps only a divider here; the name is its tooltip.
        title: repo.name,
        oncontextmenu: contextMenu(repo.name, () => [
            { heading: repo.name },
            {
                label: 'Features',
                run: () => {
                    if (!chosen)
                        call('choose_repository', { repository: repo.id });
                    call('show_features', { open: true });
                },
            },
            {
                label: 'Recent agent sessions',
                run: () => call('open_sessions', { repository: repo.id }),
            },
            'sep',
            {
                label: repo.open ? 'Collapse' : 'Expand',
                run: () => call('toggle_repository', { repository: repo.id }),
            },
        ]),
    }, el('button', {
        class: 'repobtn',
        'aria-expanded': String(repo.open),
        'aria-current': chosen ? 'true' : null,
        onclick: () => call('choose_repository', { repository: repo.id }),
    }, el('span', { class: 'chev' }, icon(repo.open ? 'down' : 'right')), el('span', { class: 'rname ttl' }, repo.name), statusDot, el('span', { class: 'cnt' }, repo.used ?? (repo.panes || ''))), el('button', {
        class: 'icon',
        title: 'Recent agent sessions',
        'aria-label': `Recent agent sessions in ${repo.name}`,
        onclick: () => call('open_sessions', { repository: repo.id }),
    }, icon('clock')));
}
function checkoutRow(repository, co) {
    const picked = state.railPick?.checkout === co.id && state.railPick.pane === null;
    const open = railStart(co.id);
    const where = co.kind === 'main' ? 'main checkout' : 'worktree';
    // Folded, the row speaks for the panes it hides: how many, and the
    // loudest of them, so one needing a person is not put out of sight.
    const colour = co.open ? null : statusColour(co.status);
    const hiddenDot = colour ? el('span', { class: 'rdot' }) : null;
    if (hiddenDot && colour)
        hiddenDot.style.background = colour;
    return el('div', {
        class: `co${picked ? ' picked' : ''}${open ? ' hot' : ''}`,
        title: co.branch ? `${where} on ${co.branch}` : where,
        onclick: () => pickRail(co.id, null),
        oncontextmenu: contextMenu(`${repository} ${co.name}`, () => checkoutMenu(co)),
    }, co.count > 0 &&
        el('button', {
            class: 'cofold',
            title: co.open ? 'Fold its panes away' : 'Show its panes',
            'aria-label': `${co.open ? 'Fold' : 'Unfold'} ${repository} ${co.name}`,
            'aria-expanded': String(co.open),
            onclick: (e) => {
                e.stopPropagation();
                foldCheckout(co.id);
            },
        }, icon(co.open ? 'down' : 'right')), icon('branch'), el('span', { class: 'name ttl' }, co.name), co.shared !== null &&
        el('span', { class: 'shared', title: `shared by ${co.shared} agents` }, icon('people'), `${co.shared}`), co.git != null && el('span', { class: `git${co.clean ? ' clean' : ''}` }, co.git), !co.open &&
        el('span', {
            class: 'cofolded',
            title: `${co.count} ${co.count === 1 ? 'pane' : 'panes'} folded away`,
        }, hiddenDot, `${co.count}`), el('button', {
        class: `icon${open ? ' hot' : ''}`,
        title: 'Start a shell or an agent',
        'aria-label': `Start a shell or an agent on ${repository} ${co.name}`,
        'aria-expanded': String(open),
        onclick: (e) => {
            e.stopPropagation();
            toggleStart(co.id, 'rail');
        },
    }, icon('plus')), open && startMenu(`${repository} ${co.name}`, co.id));
}
function checkoutMenu(co) {
    return [
        { heading: co.branch || co.name },
        ...starts().map((item) => ({
            label: item.agent ? `New agent · ${item.agent}` : 'New shell',
            run: () => start(co.id, item.agent),
        })),
        'sep',
        { label: 'Review changes', run: () => call('open_review', { checkout: co.id }) },
        { label: 'History', run: () => call('open_history', { checkout: co.id }) },
        { label: 'Show in Checkouts', run: () => call('go_to_checkout', { checkout: co.id }) },
        ...(co.count > 0
            ? [{ label: co.open ? 'Collapse' : 'Expand', run: () => foldCheckout(co.id) }]
            : []),
        ...(co.kind === 'worktree'
            ? [
                'sep',
                {
                    label: 'Remove worktree…',
                    danger: true,
                    run: () => call('remove_checkout', { checkout: co.id }),
                },
            ]
            : []),
    ];
}
/** What a pane's row, its tab or the stage's header offers. */
export function paneMenu(pane, title, first = []) {
    return [
        { heading: title },
        ...first,
        { label: 'Rename…', run: () => renameDialog(pane, title) },
        'sep',
        { label: 'Close', danger: true, run: () => call('close', { pane }) },
    ];
}
/** The folded rail's tile: the initials of the task ('tc' for 'claude ·
 *  trim context'), since the agent's name before the dot is the same on
 *  most rows and would make the tiles alike. */
function paneMark(title) {
    const at = title.indexOf(' · ');
    const words = (at >= 0 ? title.slice(at + 3) : title)
        .replace(/[^\p{L}\p{N}]+/gu, ' ')
        .trim()
        .split(' ');
    const [first = '', second = ''] = words;
    const mark = second ? first.slice(0, 1) + second.slice(0, 1) : first.slice(0, 2);
    return mark.toLowerCase();
}
function paneRow(checkout, pane) {
    const word = PANE_SAYS.has(pane.status) ? statusWord(pane.status) : undefined;
    const picked = state.railPick?.pane === pane.id;
    const row = el('div', {
        class: 'pane-row',
        oncontextmenu: contextMenu(pane.title, () => paneMenu(pane.id, pane.title, [
            {
                label: 'Show',
                run: () => {
                    pickRail(checkout, pane.id);
                    call('select', { pane: pane.id });
                },
            },
            {
                label: 'Open beside',
                run: () => call('open_beside', { pane: pane.id }),
            },
        ])),
    }, el('button', {
        class: `pane${pane.on ? ' on' : ''}${picked ? ' picked' : ''}${pane.agent ? '' : ' sh'} ${pane.status}`,
        // The folded rail shows only the initials, so the tooltip carries the name.
        title: `${pane.title} · ${pane.note ?? `${pane.agent ? 'agent' : 'shell'}, ${pane.status}`}`,
        'data-mark': paneMark(pane.title),
        'aria-current': pane.on ? 'true' : null,
        // Dragged onto the stage it opens there, beside what is shown.
        onmousedown: (e) => dragFromRail(e, pane.id),
        // The pane comes on screen, but the keys stay in the rail
        // until Enter or a click on the terminal, so x closes it.
        onclick: () => {
            pickRail(checkout, pane.id);
            call('select', { pane: pane.id });
        },
    }, 
    // One status encoding per row: the pill's own dot when it has a word,
    // else the dot alone.
    !word && el('span', { class: `sdot ${pane.status}`, 'aria-hidden': 'true' }), el('span', { class: 'ttl' }, pane.title), el('span', { class: 'pane-end' }, word
        ? el('span', { class: `spill ${pane.status}` }, word)
        : el('span', { class: `st ${pane.status}` }, el('span', { class: 'sr-only' }, pane.status)))), closeButton(pane.id, pane.title));
    return closesOnMiddle(row, () => call('close', { pane: pane.id }));
}
// ---- The rail's picked row -------------------------------------------------
function pickRail(checkout, pane) {
    state.railPick = { checkout, pane };
    $('input').blur();
    drawRail();
}
/** Folds a checkout's panes away or back. A pane picked under it gives
 *  the pick to the checkout, since its row is about to go. */
function foldCheckout(checkout) {
    if (state.railPick?.checkout === checkout && state.railPick.pane !== null) {
        state.railPick = { checkout, pane: null };
    }
    call('toggle_checkout', { checkout });
}
export function railPicked() {
    return state.railPick !== null;
}
export function unpickRail() {
    if (!state.railPick)
        return;
    state.railPick = null;
    drawRail();
}
/** Moves the pick `by` rows, through every checkout and its panes in the
 *  rail's order, stopping at the ends. A pane picked comes on screen, as
 *  a click on it does. */
function stepRail(by) {
    const pick = state.railPick;
    if (!pick)
        return;
    const rows = (state.rail.project?.repositories ?? []).flatMap((repo) => repo.checkouts.flatMap((co) => [
        { checkout: co.id, pane: null },
        ...co.panes.map((pane) => ({ checkout: co.id, pane: pane.id })),
    ]));
    const at = rows.findIndex((row) => row.checkout === pick.checkout && row.pane === pick.pane);
    const next = rows[Math.max(0, Math.min(rows.length - 1, at + by))];
    if (!next)
        return;
    pickRail(next.checkout, next.pane);
    if (next.pane !== null)
        call('select', { pane: next.pane });
}
/** Runs `action` on the rail's picked row, saying whether it took it. */
export function railAction(action) {
    const pick = state.railPick;
    if (!pick)
        return false;
    switch (action) {
        case 'closeLayer':
            unpickRail();
            return true;
        case 'next':
        case 'previous':
            stepRail(action === 'next' ? 1 : -1);
            return true;
        case 'startShell':
            start(pick.checkout, null);
            return true;
        case 'startAgent': {
            const agent = state.rail.templates[0];
            if (agent)
                start(pick.checkout, agent);
            else
                notice('argusd offers no agent to start');
            return true;
        }
        case 'openReview':
            call('open_review', { checkout: pick.checkout });
            return true;
        case 'openHistory':
            call('open_history', { checkout: pick.checkout });
            return true;
        case 'fold':
            foldCheckout(pick.checkout);
            return true;
        case 'close':
            // On a checkout, the Checkouts stage opens on it and asks first.
            if (pick.pane === null) {
                call('remove_checkout', { checkout: pick.checkout });
                return true;
            }
            call('close', { pane: pick.pane });
            // The rail drops the pane when the daemon's tree does; the cursor
            // falls back to its checkout.
            pickRail(pick.checkout, null);
            return true;
        case 'choose':
            if (pick.pane === null) {
                toggleStart(pick.checkout, 'rail');
                return true;
            }
            unpickRail();
            $('input').focus();
            return true;
        default:
            return false;
    }
}
// ---- A checkout's + menu ---------------------------------------------------
// What the + menu offers, in its order: a shell, then each of the daemon's
// agent templates, the first its default.
function starts() {
    return [
        { label: 'Shell', agent: null },
        ...state.rail.templates.map((name) => ({ label: `Agent · ${name}`, agent: name })),
    ];
}
function start(checkout, agent) {
    closeStart();
    // The new pane takes the keys when it comes on screen.
    unpickRail();
    if (agent === null)
        call('spawn_shell', { checkout });
    else
        call('spawn_agent', { checkout, template: agent });
}
function railStart(checkout) {
    return state.start?.checkout === checkout && state.start.from === 'rail';
}
/** Opens a checkout's + menu where it was pressed, or closes it. */
export function toggleStart(checkout, from) {
    const open = state.start?.checkout === checkout && state.start.from === from;
    state.start = open ? null : { checkout, from, at: 0 };
    drawRail();
    drawCheckouts();
}
export function startMenu(name, checkout) {
    const items = starts();
    const at = state.start?.at ?? 0;
    return el('div', { class: 'menu', role: 'menu', 'aria-label': `Start on ${name}` }, items.flatMap((item, i) => [
        i === 1 && el('div', { class: 'msep' }),
        el('button', {
            class: `mi${i === at ? ' on' : ''}`,
            role: 'menuitem',
            onclick: (e) => {
                e.stopPropagation();
                start(checkout, item.agent);
            },
        }, el('span', { class: `glyph${item.agent ? '' : ' sh'}` }, item.agent ? '◆' : '›'), el('span', { class: 'grow' }, item.label), i < 2 && keycaps(chordsFor(i === 0 ? 'startShell' : 'startAgent', 'start').slice(0, 1))),
    ]));
}
export function startOpen() {
    return state.start !== null;
}
export function closeStart() {
    if (!state.start)
        return;
    state.start = null;
    drawRail();
    drawCheckouts();
}
/** Runs an open + menu's `action`, saying whether it took it. */
export function startAction(action) {
    const open = state.start;
    if (!open)
        return false;
    const items = starts();
    switch (action) {
        case 'closeLayer':
            closeStart();
            return true;
        case 'startShell':
            start(open.checkout, null);
            return true;
        case 'startAgent': {
            const agent = state.rail.templates[0];
            if (agent)
                start(open.checkout, agent);
            else
                notice('argusd offers no agent to start');
            return true;
        }
        case 'next':
        case 'previous':
            open.at = (open.at + (action === 'next' ? 1 : items.length - 1)) % items.length;
            drawRail();
            drawCheckouts();
            return true;
        case 'choose': {
            const item = items[open.at];
            if (item)
                start(open.checkout, item.agent);
            return true;
        }
        default:
            return false;
    }
}
/** The strip's words: the pane on screen, or the stage and the
 *  repository it reads. */
export function drawHead() {
    const { shown } = state.rail;
    // Fullscreen, the console is what the strip names, whichever stage is
    // under it.
    const stage = state.console?.place === 'fullscreen' ? 'console' : state.rail.stage;
    const head = $('head');
    const more = $('head-more');
    if (stage !== 'workspace') {
        const repository = stage === 'console' || stage === 'extension' ? null : chosenRepository();
        const code = stage === 'code' ? codeStrip() : null;
        // An open feature is the stage's title; the way back is its crumb and Esc.
        const feature = stage === 'feature' ? (state.features?.open ?? null) : null;
        const trail = feature
            ? [STAGE_NAME.feature, el('span', { class: 'mono' }, `#${feature.number}`)]
            : (code?.crumb ?? []);
        const strip = stage === 'feature' ? featuresStrip() : stage === 'checkouts' ? checkoutsStrip() : null;
        const popped = stage === 'view' ? poppedViewHead() : null;
        more.replaceChildren(...(code?.tools ?? strip?.tools ?? popped?.tools ?? []));
        head.replaceChildren(headStack(repository
            ? el('span', null, repository, ...trail.flatMap((part) => [el('span', { class: 'fg-faint' }, '/'), part]))
            : // Every stage has a crumb, so every title sits on one baseline.
                stage === 'extension'
                    ? STAGE_NAME.extension
                    : null, el('span', { class: 'type-heading head-title' }, feature
            ? feature.title
            : stage === 'extension'
                ? extensionPageTitle()
                : stage === 'view'
                    ? (poppedViewTitle() ?? STAGE_NAME.view)
                    : STAGE_NAME[stage]), popped?.status ?? null, ...(strip?.meta ?? [])));
        return;
    }
    if (!shown) {
        head.replaceChildren();
        more.replaceChildren();
        return;
    }
    const size = screen.sizeLabel();
    head.replaceChildren(headStack(el('span', null, el('span', null, shown.repository), el('span', { class: 'fg-faint' }, '/'), el('span', { class: 'mono' }, shown.branch || shown.checkout)), el('span', { class: 'head-name' }, 
    // Rename lives in the overflow; right-click was an undiscoverable second way.
    el('span', { class: 'type-heading head-title' }, shown.title), 
    // A shell or editor has no work to report: "idle" on a file is noise, so
    // only an agent, or a program that ended or asks, gets the pill.
    shown.agent || PANE_SAYS.has(shown.status)
        ? el('span', { class: `spill ${shown.status}` }, statusWord(shown.status))
        : null, shown.resumed ? el('span', { class: 'meta ellipsis' }, resumedFrom(shown.resumed)) : '')), 
    // The grid is debugging detail: faint, and its words are in the tooltip.
    el('span', { class: 'meta mono head-size', id: 'size', title: 'Size in columns and rows' }, size));
    more.replaceChildren(el('button', {
        class: 'icon',
        type: 'button',
        title: 'More: rename or close this pane',
        'aria-label': `More about ${shown.title}`,
        'aria-haspopup': 'menu',
        onclick: (e) => {
            const box = e.currentTarget.getBoundingClientRect();
            openMenu(shown.title, paneMenu(shown.pane, shown.title), {
                x: box.right,
                align: 'end',
                y: box.bottom + 4,
            });
        },
    }, icon('more')));
}
/** The strip's words, stacked: the small crumb over the title, so a stage
 *  is named once. */
function headStack(crumb, ...rest) {
    return el('span', { class: 'head-stack' }, el('span', { class: 'crumbs' }, crumb), el('span', { class: 'head-line' }, ...rest));
}
/** What a pane this window resumed a session in was resumed from. */
function resumedFrom(from) {
    const at = clock(from.at);
    const turns = from.turns === null ? null : `${from.turns} turn${from.turns === 1 ? '' : 's'} before`;
    return [at ? `resumed from ${at}` : 'resumed', turns].filter((part) => part).join(' · ');
}
/** Ends the pane's program; the rail drops it when the daemon's tree does. */
function closeButton(pane, title) {
    return el('button', {
        class: 'icon close',
        title: 'Close',
        'aria-label': `Close ${title}`,
        onclick: () => call('close', { pane }),
    }, icon('close'));
}
function dot(colour) {
    const node = el('span', { class: 'dot' });
    if (colour)
        node.style.background = colour;
    return node;
}
/** The sidebar's foot: the tallies that need a look, a daemon's alarm,
 *  and the connection with the chime and the settings beside it. */
function drawBar() {
    const { connection, alarm, host } = state.status;
    const { needsYou, working } = state.rail;
    const tallies = [];
    if (connection === 'connected') {
        if (needsYou > 0) {
            tallies.push(el('span', { class: 'fg-attention strong' }, `▲ ${needsYou} need you`));
        }
        if (working > 0)
            tallies.push(el('span', { class: 'fg-accent' }, `⋮ ${working} working`));
    }
    // One shape in every state: only the dot and the words change, so the
    // footer does not rearrange while the daemon comes and goes. A remote
    // daemon is named, so what the rail lists is never taken for this machine's.
    const [colour, words] = connection === 'connected'
        ? [undefined, `argusd · ${host ?? 'local'}`]
        : connection === 'connecting'
            ? ['var(--attention)', 'argusd · connecting']
            : [undefined, 'argusd · stopped'];
    const status = dot(colour);
    if (connection === 'stopped')
        status.classList.add('hollow');
    status.title = words;
    const shown = connection === 'connected' && alarm;
    $('bar').replaceChildren(tallies.length > 0 ? el('div', { class: 'bar-row' }, tallies) : '', shown ? el('div', { class: 'bar-alarm fg-attention' }, alarm) : '', el('div', { class: 'bar-row' }, status, el('span', { class: 'grow ellipsis', title: host ?? '' }, words), consoleChip(), chimeSwitch(), settingsButton()));
}
/** The console's way back when hidden, and its count of consoles. */
function consoleChip() {
    const page = state.console;
    const shown = !!page && page.place !== 'hidden';
    const count = page?.tabs.length ?? 0;
    const keys = chordsFor('toggleConsole', 'anywhere')
        .map((chord) => presses(chord).join(' '))
        .join(', ');
    const words = shown ? 'Hide the console' : 'Show the console';
    return el('button', {
        class: `icon wide${shown ? ' hot' : ''}`,
        title: keys ? `${words} (${keys})` : words,
        'aria-label': words,
        'aria-pressed': String(shown),
        onclick: () => call('console', { gesture: { on: 'toggle' } }),
    }, icon('console'), count > 0 ? el('span', { class: 'icount' }, String(count)) : '');
}
function chimeSwitch() {
    const on = state.status.chime;
    const label = on ? 'Mute the chime when an agent needs you' : 'Chime when an agent needs you';
    return el('button', {
        class: 'icon',
        title: label,
        'aria-label': label,
        'aria-pressed': String(on),
        onclick: () => call('set_chime', { on: !on }),
    }, icon(on ? 'bell' : 'bell-off'));
}
/** The way to the settings, named with its key from the keymap. */
function settingsButton() {
    const keys = chordsFor('settings', 'anywhere')
        .map((chord) => presses(chord).join(' '))
        .join(', ');
    return el('button', {
        class: 'icon',
        title: keys ? `Settings (${keys})` : 'Settings',
        'aria-label': 'Settings',
        onclick: openSettings,
    }, icon('gear'));
}
/** The panes in the tree, with the checkout each lives in. */
function allPanes() {
    return (state.rail.project?.repositories ?? []).flatMap((repo) => repo.checkouts.flatMap((checkout) => checkout.panes.map((pane) => ({ pane, checkout }))));
}
/** What a pane whose program has ended offers; the tree redraws the rest. */
function drawEnded() {
    const bar = $('ended');
    const { shown } = state.rail;
    bar.hidden = shown?.status !== 'exited';
    if (bar.hidden || !shown)
        return;
    const found = allPanes().find(({ pane }) => pane.id === shown.pane);
    const template = found ? endedTemplate(found.pane) : null;
    bar.replaceChildren(icon('info'), el('span', { class: 'ended-text' }, el('strong', null, `${shown.title} exited`), found?.pane.note && el('span', { class: 'fg-muted' }, ` \u00b7 ${found.pane.note}`)), found
        ? el('button', {
            class: 'btn sm',
            onclick: () => start(found.checkout.id, template),
        }, template ? `Start ${template} here` : 'New shell here')
        : '', el('button', { class: 'btn sm', onclick: () => call('close', { pane: shown.pane }) }, 'Close pane'));
}
/** The template an ended agent pane's button starts. The page cannot know what
 *  the ended pane ran (a pane carries only whether it was an agent), so it offers
 *  what it can start, by name, and leaves the old pane to 'Close pane'. It is: the window's first, or
 *  none (a shell) when the pane was not an agent or argusd offers no template. */
function endedTemplate(pane) {
    return pane.agent ? (state.rail.templates[0] ?? null) : null;
}
function drawStage() {
    const { connection, detail, host } = state.status;
    const { loaded, project, shown } = state.rail;
    const empty = $('empty');
    $('terminal').hidden = !shown;
    $('body').classList.toggle('held', connection === 'stopped');
    drawEnded();
    empty.hidden = !!shown;
    empty.classList.remove('no-pane');
    if (shown)
        return;
    let body;
    // Each state's text is set to its own measure.
    let measure;
    if (!loaded) {
        measure = 'connecting';
        body = [
            el('div', { class: 'empty-icon spin' }, icon('spinner')),
            el('div', { class: 'empty-title' }, 'Connecting to argusd'),
            el('div', { class: 'fg-muted' }, `${host ? `on ${host}` : 'on this machine'} · the sidebar fills in when it answers`),
            detail && el('div', { class: 'err' }, detail),
            // Nothing is loaded to open the switcher from, so there is always one
            // way out: back to this machine, or to the settings.
            host
                ? el('button', { class: 'btn', onclick: () => call('set_host', { host: null }) }, 'Use this machine')
                : el('button', { class: 'btn', onclick: openSettings }, icon('gear'), 'Settings'),
        ];
    }
    else if (!project) {
        measure = 'no-projects';
        body = [
            el('div', { class: 'empty-icon tile' }, icon('folder')),
            el('div', { class: 'empty-title' }, 'No projects'),
            el('div', { class: 'fg-muted' }, 'Add a folder of repositories, or run argus init in one.'),
            el('button', { class: 'btn primary', onclick: () => call('open_add_project', {}) }, icon('plus'), 'Add a project…'),
        ];
    }
    else {
        measure = 'no-pane';
        empty.classList.add('no-pane');
        const repo = project.repositories.find((r) => r.id === state.rail.repository) ?? project.repositories[0];
        const co = repo?.checkouts[0];
        const keys = chordsFor('compose', 'anywhere');
        body = [
            el('div', { class: 'empty-icon tile' }, icon('terminal')),
            el('div', { class: 'empty-title' }, 'No pane on screen'),
            el('div', { class: 'fg-muted' }, 'Start an agent on a checkout, open a shell, or pick up a pane that is still running.'),
            el('div', { class: 'empty-actions' }, el('button', { class: 'btn primary', onclick: () => call('show_composer', { open: true }) }, icon('plus'), 'Start an agent', keys.length > 0 ? keycaps(keys.slice(0, 1)) : ''), co &&
                el('button', { class: 'btn', onclick: () => call('spawn_shell', { checkout: co.id }) }, icon('console'), co.branch ? `Open a shell on ${co.branch}` : 'Open a shell')),
            co &&
                repo &&
                el('div', { class: 'empty-where fg-muted' }, 'on ', el('span', { class: 'mono' }, co.branch || co.name), ` in ${repo.name}`),
            stillRunning(project),
        ];
    }
    empty.replaceChildren(el('div', { class: `empty-body ${measure}` }, body));
}
const RUNNING_ORDER = {
    waiting: 0,
    failed: 1,
    'needs-review': 2,
    working: 3,
};
/** The panes that outlived the screen they were on, those that need a person
 *  first: the way back into work the stage does not show. */
function stillRunning(project) {
    const cards = project.repositories
        .flatMap((repo) => repo.checkouts.flatMap((co) => co.panes.map((pane) => ({ where: co.branch || co.name, pane }))))
        .filter(({ pane }) => pane.status !== 'exited' && pane.status !== 'done')
        .toSorted((a, b) => (RUNNING_ORDER[a.pane.status] ?? 9) - (RUNNING_ORDER[b.pane.status] ?? 9))
        .slice(0, 3);
    if (cards.length === 0)
        return null;
    return el('section', { class: 'running', 'aria-label': 'Still running' }, el('div', { class: 'running-head' }, 'Still running'), el('div', { class: 'running-cards' }, cards.map(({ where, pane }) => el('button', { class: 'rcard', onclick: () => call('select', { pane: pane.id }) }, el('span', { class: 'rcard-top' }, el('span', { class: 'mono ellipsis' }, where), el('span', { class: `spill ${pane.status}` }, statusWord(pane.status))), el('span', { class: 'rcard-title ellipsis' }, pane.title), pane.note && el('span', { class: 'rcard-note mono ellipsis' }, pane.note)))));
}
let noticeTimer;
/** Shows a line along the bottom of the window for a few seconds. */
export function notice(text) {
    const node = $('notice');
    const dismiss = () => {
        clearTimeout(noticeTimer);
        node.hidden = true;
    };
    node.replaceChildren(icon('info'), el('span', { class: 'notice-text' }, text), el('button', { class: 'icon', 'aria-label': 'Dismiss', onclick: dismiss }, icon('close')));
    node.hidden = false;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(dismiss, 6000);
}
export function reconnect() {
    call('reconnect', {});
}
// ---- The rail's width ------------------------------------------------------
/** The width the person dragged the rail to, or the stylesheet's own,
 *  which narrows in a small window, when they never did. */
function railWidth(width) {
    const root = document.documentElement.style;
    if (width === null)
        root.removeProperty('--rail-set');
    else
        root.setProperty('--rail-set', `${width}px`);
}
/** The rail's right edge, for Rust to hold to its bounds, keep and send
 *  back; a double-click gives back the usual width. */
export function listenToRailEdge() {
    const rail = $('rail-sash').parentElement;
    if (!rail)
        return;
    dragEdge($('rail-sash'), rail, {
        sizeAt: (x) => x - rail.getBoundingClientRect().left,
        draw: railWidth,
        report: (width) => call('rail_width', { width }),
    });
}
