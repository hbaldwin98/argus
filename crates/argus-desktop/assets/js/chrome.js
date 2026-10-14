// The window's frame around the stage: the rail, the stage tabs, the pane
// header, the status bar, the empty stage, and notices that pass.
import { drawCheckouts } from './checkouts.js';
import { $, el, ghosts, icon } from './dom.js';
import { drawFeatures } from './features.js';
import { screen } from './screen.js';
import { state } from './state.js';
import { call } from './tauri.js';
const LOUD = new Set(['waiting', 'needs-review', 'failed']);
export function drawChrome() {
    const stopped = state.status.connection === 'stopped';
    const stage = state.rail.stage;
    for (const [id, on] of [
        ['tab-workspace', stage === 'workspace'],
        ['tab-feature', stage === 'feature'],
        ['tab-checkouts', stage === 'checkouts'],
    ]) {
        $(id).classList.toggle('on', on);
        $(id).setAttribute('aria-selected', String(on));
    }
    $('fstage').hidden = stage !== 'feature';
    $('cstage').hidden = stage !== 'checkouts';
    $('body').hidden = stage !== 'workspace';
    drawFeatures();
    drawCheckouts();
    drawTabsMeta();
    drawHeading();
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
/** The Feature and Checkouts stages name the repository they read, at
 *  the tabs' end. */
function drawTabsMeta() {
    const { project, repository, stage } = state.rail;
    const chosen = project?.repositories.find((repo) => repo.id === repository);
    $('tabs-meta').textContent = stage !== 'workspace' && chosen ? chosen.name : '';
}
export function drawHeading() {
    const { project, projects } = state.rail;
    const heading = $('heading');
    if (!project) {
        heading.replaceChildren();
        return;
    }
    const several = projects.length > 1;
    const button = el('button', {
        class: 'projbtn',
        'aria-label': several ? 'Switch project' : null,
        'aria-expanded': several ? String(state.menuOpen) : null,
        onclick: () => {
            if (!several)
                return;
            state.menuOpen = !state.menuOpen;
            drawHeading();
        },
    }, el('span', null, project.name), several && icon('down'), project.needsYou > 0 && el('span', { class: 'badge' }, `${project.needsYou} need you`));
    const menu = several &&
        state.menuOpen &&
        el('div', { class: 'menu', role: 'menu' }, projects.map((choice) => el('button', {
            class: `mi${choice.id === project.id ? ' on' : ''}`,
            role: 'menuitem',
            onclick: () => {
                state.menuOpen = false;
                call('select_project', { project: choice.id });
                drawHeading();
            },
        }, el('span', { class: 'grow' }, choice.name), choice.needsYou > 0 && el('span', { class: 'st waiting' }, `${choice.needsYou}`))));
    heading.replaceChildren(button, menu || '');
}
function statusColour(word) {
    if (word && LOUD.has(word))
        return 'var(--attention)';
    if (word === 'working')
        return 'var(--accent)';
    return null;
}
function drawRail() {
    const rail = $('rail');
    const { project, loaded } = state.rail;
    if (!loaded) {
        // A repository, its checkout, two panes, another checkout.
        rail.replaceChildren(ghosts([
            [110, 16],
            [160, 24],
            [190, 32],
            [140, 32],
            [120, 24],
        ]));
        return;
    }
    if (!project) {
        rail.replaceChildren(el('div', { class: 'empty-rail' }, 'No projects yet.'));
        return;
    }
    const rows = [];
    for (const repo of project.repositories) {
        if (repo.divider)
            rows.push(el('div', { class: 'rule', 'aria-hidden': 'true' }));
        const colour = statusColour(repo.status);
        const statusDot = colour ? el('span', { class: 'rdot' }) : null;
        if (statusDot && colour)
            statusDot.style.background = colour;
        const chosen = repo.id === state.rail.repository;
        rows.push(el('div', { class: `repo${repo.quiet ? ' quiet' : ''}${chosen ? ' sel' : ''}` }, el('button', {
            class: 'repobtn',
            'aria-expanded': String(repo.open),
            'aria-current': chosen ? 'true' : null,
            onclick: () => call('choose_repository', { repository: repo.id }),
        }, el('span', { class: 'chev' }, icon(repo.open ? 'down' : 'right')), statusDot, el('span', { class: 'rname ttl' }, repo.name), el('span', { class: 'cnt' }, repo.used ?? (repo.panes || '')))));
        for (const co of repo.checkouts) {
            const coPicked = state.railPick?.checkout === co.id && state.railPick.pane === null;
            rows.push(el('div', {
                class: `co${coPicked ? ' picked' : ''}`,
                onclick: () => pickRail(co.id, null),
            }, el('span', { class: 'name', title: co.branch || null }, co.name), el('span', null, co.kind), co.git != null && el('span', { class: `git${co.clean ? ' clean' : ''}` }, co.git), el('button', {
                class: `icon${railStart(co.id) ? ' hot' : ''}`,
                title: 'Start a shell or an agent',
                'aria-label': `Start a shell or an agent on ${repo.name} ${co.name}`,
                'aria-expanded': String(railStart(co.id)),
                onclick: (e) => {
                    e.stopPropagation();
                    toggleStart(co.id, 'rail');
                },
            }, icon('plus')), railStart(co.id) && startMenu(`${repo.name} ${co.name}`, co.id)));
            for (const pane of co.panes) {
                rows.push(el('div', { class: 'pane-row' }, el('button', {
                    class: `pane${pane.on ? ' on' : ''}${state.railPick?.pane === pane.id ? ' picked' : ''} ${pane.status}`,
                    title: pane.note,
                    'aria-current': pane.on ? 'true' : null,
                    // The pane comes on screen, but the keys stay in the rail
                    // until Enter or a click on the terminal, so x closes it.
                    onclick: () => {
                        pickRail(co.id, pane.id);
                        call('select', { pane: pane.id });
                    },
                }, el('span', { class: `glyph${pane.agent ? '' : ' sh'}` }, pane.agent ? '◆' : '›'), el('span', { class: 'ttl' }, pane.title), el('span', { class: `st ${pane.status}` }, pane.status)), closeButton(pane.id, pane.title)));
            }
        }
    }
    rail.replaceChildren(...rows);
}
// ---- The rail's picked row -------------------------------------------------
function pickRail(checkout, pane) {
    state.railPick = { checkout, pane };
    $('input').blur();
    drawRail();
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
        case 'close':
            if (pick.pane === null)
                return false;
            call('close', { pane: pick.pane });
            // The rail drops the pane when the daemon's tree does; the cursor
            // falls back to its checkout, so a second x closes nothing else.
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
        }, el('span', { class: `glyph${item.agent ? '' : ' sh'}` }, item.agent ? '◆' : '›'), el('span', { class: 'grow' }, item.label), i === 0 && el('span', { class: 'kbd' }, 'S'), i === 1 && el('span', { class: 'kbd' }, 'A')),
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
function drawHead() {
    const shown = state.rail.shown;
    const head = $('head');
    head.hidden = !shown || state.rail.stage !== 'workspace';
    if (!shown)
        return;
    head.replaceChildren(el('span', { class: 'crumbs' }, el('span', null, shown.repository), el('span', { class: 'fg-faint' }, '/'), el('span', { class: 'mono fg-ref' }, shown.branch || shown.checkout), el('span', { class: 'fg-faint' }, '/')), el('span', { class: `glyph${shown.agent ? '' : ' sh'}` }, shown.agent ? '◆' : '›'), el('span', { class: 'type-heading head-title' }, shown.title), el('span', { class: `pill ${shown.status}` }, shown.status), el('span', { class: 'grow' }), el('span', { class: 'meta mono', id: 'size' }, screen.sizeLabel()), closeButton(shown.pane, shown.title));
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
function drawBar() {
    const { connection, alarm } = state.status;
    const { needsYou, working } = state.rail;
    const left = [];
    const right = [];
    if (connection === 'connected') {
        if (needsYou > 0) {
            left.push(el('span', { class: 'fg-attention strong' }, `▲ ${needsYou} need you`));
        }
        if (working > 0)
            left.push(el('span', { class: 'fg-accent' }, `⋮ ${working} working`));
        if (alarm)
            left.push(el('span', { class: 'fg-attention' }, alarm));
        right.push(el('span', { class: 'actions' }, dot(), 'Connected'));
    }
    else if (connection === 'connecting') {
        left.push(el('span', { class: 'actions' }, dot('var(--attention)'), 'Connecting'));
    }
    else {
        left.push(el('span', { class: 'actions' }, dot('var(--fg-faint)'), 'Stopped'));
        right.push(el('button', { class: 'btn sm', onclick: reconnect }, 'Reconnect'));
    }
    $('bar').replaceChildren(...left, el('span', { class: 'grow' }), ...right);
}
function drawStage() {
    const { connection, detail } = state.status;
    const { loaded, project, shown } = state.rail;
    const empty = $('empty');
    $('terminal').hidden = !shown;
    $('body').classList.toggle('held', connection === 'stopped');
    empty.hidden = !!shown;
    if (shown)
        return;
    let body;
    // Each state's text is set to its own measure.
    let measure;
    if (!loaded) {
        measure = 'connecting';
        body = [
            ghosts([[0.6], [0.9], [0.75], [0.85], [0.4]]),
            el('div', { class: 'meta' }, 'Connecting to argusd…'),
            detail && el('div', { class: 'err' }, detail),
        ];
    }
    else if (!project) {
        measure = 'no-projects';
        body = [
            icon('folder'),
            el('div', { class: 'type-title' }, 'No projects'),
            el('div', { class: 'fg-muted' }, 'Run this in a folder of repositories:'),
            el('div', { class: 'cmd' }, 'argus init'),
        ];
    }
    else {
        measure = 'pick';
        body = [
            icon('terminal'),
            el('div', { class: 'type-title' }, 'Pick a pane'),
            el('div', { class: 'fg-muted' }, 'or start one with + on a checkout.'),
        ];
    }
    empty.replaceChildren(el('div', { class: `empty-body ${measure}` }, body));
}
let noticeTimer;
/** Shows a line along the bottom of the window for a few seconds. */
export function notice(text) {
    const node = $('notice');
    node.textContent = text;
    node.hidden = false;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => {
        node.hidden = true;
    }, 6000);
}
export function reconnect() {
    call('reconnect', {});
}
