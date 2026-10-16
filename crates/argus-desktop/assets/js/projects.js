// The sidebar's project heading and what it opens: the switcher's menu of
// projects, workspaces and what can be done to them, the question before a
// project is removed, and the add-a-project dialog. Rust holds the
// workspaces, the question and the dialog (`src/switcher.rs`,
// `src/add_project.rs`); nothing here changes until the daemon says so.
import { $, el, ghosts, icon } from './dom.js';
import { actionFor } from './keymap.js';
import { state } from './state.js';
import { call } from './tauri.js';
// ---- The switcher ------------------------------------------------------------
function closeMenu() {
    state.menuOpen = false;
    state.workspacesOpen = false;
    state.hostsOpen = false;
    drawHeading();
}
/** What a project's row says of its agents: who needs you, else who is
 *  working, else that it is quiet. */
function tally(choice) {
    if (choice.needsYou > 0)
        return el('span', { class: 'st waiting' }, `${choice.needsYou} need you`);
    if (choice.working > 0)
        return el('span', { class: 'st working' }, `${choice.working} working`);
    return el('span', { class: 'meta' }, 'quiet');
}
function projectRow(choice, current) {
    const on = choice.id === current;
    return el('button', {
        class: `mi${on ? ' on' : ''}`,
        role: 'menuitemradio',
        'aria-checked': String(on),
        onclick: () => {
            closeMenu();
            call('select_project', { project: choice.id });
        },
    }, el('span', { class: 'glyph fg-ok' }, on ? '✓' : ''), el('span', { class: 'grow ellipsis' }, choice.name), tally(choice));
}
/** The workspaces, unfolded under Switch workspace…: opening one is the
 *  daemon's, and every client follows it. */
function workspaceRows() {
    if (!state.workspacesOpen)
        return null;
    return state.rail.workspaces.map((w) => el('button', {
        class: `mi sub${w.open ? ' on' : ''}`,
        role: 'menuitemradio',
        'aria-checked': String(w.open),
        onclick: () => {
            closeMenu();
            if (!w.open)
                call('open_workspace', { workspace: w.id });
        },
    }, el('span', { class: 'glyph fg-ok' }, w.open ? '✓' : ''), el('span', { class: 'grow ellipsis' }, w.name), el('span', { class: 'meta' }, `${w.projects} project${w.projects === 1 ? '' : 's'}`)));
}
/** One host in the list; choosing it switches the daemon the window reaches. */
function hostRow(name, current, host) {
    return el('button', {
        class: `mi sub${current ? ' on' : ''}`,
        role: 'menuitemradio',
        'aria-checked': String(current),
        onclick: () => {
            closeMenu();
            if (!current)
                call('set_host', { host });
        },
    }, el('span', { class: 'glyph fg-ok' }, current ? '✓' : ''), el('span', { class: 'grow ellipsis' }, name));
}
/** The machines whose daemon the window can reach over ssh, unfolded under
 *  Connect to a host…: this machine's, the ssh config's, and the ones used
 *  before. Choosing one drops the rail until that daemon answers. */
function hostRows() {
    if (!state.hostsOpen)
        return null;
    const { hosts } = state.rail;
    const remote = hosts.some((h) => h.current);
    return [
        hostRow('This machine', !remote, null),
        ...hosts.map((h) => hostRow(h.name, h.current, h.name)),
        hosts.length === 0 &&
            el('div', { class: 'mi sub meta' }, 'No hosts in ~/.ssh/config, or set ARGUS_DESKTOP_HOST'),
    ];
}
function projectMenu() {
    const { project, projects, workspaces } = state.rail;
    const open = workspaces.find((w) => w.open);
    const connected = state.rail.hosts.find((h) => h.current);
    return el('div', { class: 'menu projects-menu', role: 'menu', 'aria-label': 'Projects' }, projects.length > 0 && el('div', { class: 'mlabel' }, 'Projects'), projects.map((choice) => projectRow(choice, project?.id ?? null)), projects.length > 0 && el('div', { class: 'msep' }), el('button', {
        class: 'mi',
        role: 'menuitem',
        onclick: () => {
            closeMenu();
            call('open_add_project', {});
        },
    }, el('span', { class: 'glyph sh' }, '+'), 'Add a project…'), workspaces.length > 0 &&
        el('button', {
            class: 'mi',
            role: 'menuitem',
            'aria-expanded': String(state.workspacesOpen),
            onclick: () => {
                state.workspacesOpen = !state.workspacesOpen;
                drawHeading();
            },
        }, el('span', { class: 'glyph sh' }, '⌂'), 'Switch workspace…', el('span', { class: 'grow' }), open && el('span', { class: 'meta' }, open.name)), workspaceRows(), el('button', {
        class: 'mi',
        role: 'menuitem',
        'aria-expanded': String(state.hostsOpen),
        title: 'Reach the daemon on another machine over ssh, as argus --host does',
        onclick: () => {
            state.hostsOpen = !state.hostsOpen;
            drawHeading();
        },
    }, el('span', { class: 'glyph sh' }, '⇆'), 'Connect to a host…', el('span', { class: 'grow' }), connected && el('span', { class: 'meta' }, connected.name)), hostRows(), project && el('div', { class: 'msep' }), project &&
        el('button', {
            class: 'mi danger',
            role: 'menuitem',
            onclick: () => {
                closeMenu();
                call('remove_project', { project: project.id });
            },
        }, el('span', { class: 'glyph sh' }), `Remove ${project.name}…`));
}
export function drawHeading() {
    const { project, loaded } = state.rail;
    const heading = $('heading');
    if (!loaded) {
        heading.replaceChildren();
        return;
    }
    // With no project the heading still opens the menu: it is how the first
    // one is added.
    const button = el('button', {
        class: 'projbtn',
        'aria-label': 'Switch project',
        'aria-haspopup': 'menu',
        'aria-expanded': String(state.menuOpen),
        onclick: () => {
            state.menuOpen = !state.menuOpen;
            state.workspacesOpen = false;
            state.hostsOpen = false;
            drawHeading();
        },
    }, el('span', { class: 'ellipsis' }, project?.name ?? 'Projects'), icon('down'), project &&
        project.needsYou > 0 &&
        el('span', { class: 'badge' }, `${project.needsYou} need you`));
    heading.replaceChildren(button, state.menuOpen ? projectMenu() : '');
}
// ---- Removing one ------------------------------------------------------------
/** Says no to the question, as Escape or a click outside does. */
export function answerRemoveNo() {
    if (!state.rail.removing)
        return false;
    call('answer_remove_project', { yes: false });
    return true;
}
function removeDialog(r) {
    const repositories = `${r.repositories} ${r.repositories === 1 ? 'repository' : 'repositories'}`;
    return el('div', {
        class: 'scrim',
        id: 'project-remove',
        onclick: (e) => {
            if (e.target === e.currentTarget)
                answerRemoveNo();
        },
    }, el('section', {
        class: 'dialog hold-dialog confirm-dialog',
        role: 'alertdialog',
        'aria-modal': 'true',
        'aria-labelledby': 'remove-title',
    }, el('div', { class: 'dhead' }, el('div', { class: 'type-title', id: 'remove-title' }, 'Remove ', el('span', null, r.name), '?')), el('div', { class: 'dbody' }, el('div', { class: 'fg-muted' }, `Its ${repositories} leave the sidebar. The folders stay where they are, and adding it again brings them back.`), r.panes > 0 &&
        el('div', { class: 'meta' }, `${r.panes} pane${r.panes === 1 ? ' is' : 's are'} running in it; argusd removes it only once they are closed.`)), el('div', { class: 'dfoot' }, el('span', { class: 'grow' }), el('button', { class: 'btn', id: 'remove-keep', onclick: answerRemoveNo }, 'Keep it'), el('button', { class: 'btn danger', onclick: () => call('answer_remove_project', { yes: true }) }, 'Remove'))));
}
/** The question over everything while Rust holds one. */
export function drawRemoving() {
    const shown = document.getElementById('project-remove');
    const r = state.rail.removing;
    if (!r) {
        shown?.remove();
        return;
    }
    if (shown)
        return;
    $('layer').replaceChildren(removeDialog(r));
    document.getElementById('remove-keep')?.focus();
}
// ---- Adding one --------------------------------------------------------------
export function addProjectOpen() {
    return state.addProject?.open === true;
}
const KINDS = [
    ['folder', 'A folder of repositories'],
    ['repository', 'One repository'],
    ['new', 'A new repository'],
];
/** What a field says now: its own text once drawn, since an echo landing
 *  after the next letter would drop it. */
function typed(id, sent) {
    const node = document.getElementById(id);
    return node instanceof HTMLInputElement ? node.value : sent;
}
function fields(a) {
    return {
        name: typed('ap-name', a.name),
        skip: typed('ap-skip', a.skip),
        folder: typed('ap-folder', a.folder),
    };
}
let fieldsTimer;
/** A changed skip scans the folder again, so what was typed is sent once
 *  typing pauses rather than a scan a letter. */
function fieldsChanged() {
    const a = state.addProject;
    if (!a)
        return;
    clearTimeout(fieldsTimer);
    fieldsTimer = setTimeout(() => call('add_project', { gesture: { on: 'fields', ...fields(a) } }), 250);
}
function add() {
    const a = state.addProject;
    if (!a || a.blocked || a.adding)
        return;
    clearTimeout(fieldsTimer);
    call('add_project', { gesture: { on: 'add', ...fields(a) } });
}
function closeAdd() {
    clearTimeout(fieldsTimer);
    call('add_project', { gesture: { on: 'close' } });
}
function field(id, value, props) {
    const input = el('input', { class: 'field', id, ...props });
    input.value = typed(id, value);
    return input;
}
function kinds(a) {
    return el('div', { class: 'seg', role: 'radiogroup', 'aria-label': 'What to add' }, KINDS.map(([kind, label]) => el('button', {
        class: `segb${a.kind === kind ? ' on' : ''}`,
        role: 'radio',
        'aria-checked': String(a.kind === kind),
        onclick: () => call('add_project', { gesture: { on: 'kind', kind } }),
    }, label)));
}
function pathRow(a) {
    const parent = a.parent;
    return el('div', { class: 'actions ap-path' }, el('button', {
        class: 'icon',
        'aria-label': 'Up one folder',
        title: 'Up one folder',
        disabled: !parent,
        onclick: () => parent && call('add_project', { gesture: { on: 'browse', path: parent } }),
    }, icon('up')), el('label', { class: 'sr-only', for: 'ap-path' }, 'Folder'), field('ap-path', a.path ?? '', {
        class: 'field mono grow',
        spellcheck: 'false',
        onkeydown: (e) => {
            if (actionFor(['writing'], e) === 'choose' && e.target instanceof HTMLInputElement) {
                e.preventDefault();
                call('add_project', { gesture: { on: 'browse', path: e.target.value } });
            }
        },
    }));
}
function folders(a) {
    let body;
    if (a.browsing && a.entries.length === 0)
        body = ghosts([[0.5], [0.7], [0.4], [0.6]]);
    else if (a.listingError)
        body = el('div', { class: 'fg-danger ap-note' }, a.listingError);
    else if (a.entries.length === 0)
        body = el('div', { class: 'meta ap-note' }, 'No folders in here.');
    else {
        body = a.entries.map((entry) => el('button', {
            class: 'drow2',
            title: `Open ${entry.name}`,
            onclick: () => call('add_project', { gesture: { on: 'into', name: entry.name } }),
        }, el('span', { class: 'fg-faint' }, '▸'), el('span', { class: 'grow ellipsis' }, entry.name), el('span', { class: 'meta' }, entry.isRepo ? 'git' : 'folder')));
    }
    return el('div', { class: 'ap-folders', 'aria-label': a.path ? `Folders in ${a.path}` : 'Folders' }, body);
}
/** What the add would do, as the daemon has said it. */
function outcome(a) {
    if (a.kind === 'new') {
        return [
            el('label', { class: 'label', for: 'ap-folder' }, 'New repository’s folder'),
            field('ap-folder', a.folder, {
                class: 'field mono',
                placeholder: 'scratch',
                spellcheck: 'false',
                oninput: fieldsChanged,
            }),
            a.target && el('div', { class: 'meta mono ellipsis', title: a.target }, a.target),
            el('div', { class: 'meta' }, 'Made inside the folder on the left, and git init runs there. A folder that is already a repository is added as it is.'),
        ];
    }
    if (a.previewing || (a.path && a.depth === null && !a.listingError)) {
        return [
            el('div', { class: 'label' }, 'Looking for repositories…'),
            ghosts([[0.6], [0.8], [0.5]]),
        ];
    }
    if (a.kind === 'repository') {
        const one = a.repositories.length === 1 && a.repositories[0] === a.defaultName;
        return one
            ? [
                el('div', { class: 'label' }, 'Would add'),
                el('div', { class: 'meta mono' }, a.defaultName),
            ]
            : [
                el('div', { class: 'label' }, 'Not a repository'),
                el('div', { class: 'meta' }, 'Pick a folder marked git, or add this one as a folder of repositories.'),
            ];
    }
    const n = a.repositories.length;
    return [
        el('div', { class: 'label' }, `Would add ${n} ${n === 1 ? 'repository' : 'repositories'}`),
        el('div', { class: 'ap-found' }, a.repositories.map((repo) => el('div', { class: 'meta mono ellipsis' }, repo))),
        a.depth !== null &&
            el('div', { class: 'meta' }, `Searched up to ${a.depth} folders deep. A repository’s own submodules and worktrees stay part of it. New ones cloned here later join on their own.`),
    ];
}
function setRow(label, aside, input) {
    return el('div', { class: 'setrow' }, el('div', { class: 'grow stack-2' }, label, aside), input);
}
function addDialog(a) {
    const name = typed('ap-name', a.name).trim() || a.defaultName;
    return el('div', {
        class: 'scrim',
        onclick: (e) => {
            if (e.target === e.currentTarget)
                closeAdd();
        },
    }, el('section', {
        class: 'dialog add-dialog',
        role: 'dialog',
        'aria-modal': 'true',
        'aria-labelledby': 'ap-title',
        onkeydown: (e) => {
            // The dialog holds the keys: none reach the stage under it.
            e.stopPropagation();
            if (actionFor(['anywhere'], e) === 'closeLayer') {
                e.preventDefault();
                closeAdd();
            }
        },
    }, el('div', { class: 'dhead ap-head' }, el('div', { class: 'type-title grow', id: 'ap-title' }, 'Add a project'), el('button', { class: 'icon', 'aria-label': 'Close', onclick: closeAdd }, icon('close'))), el('div', { class: 'ap-body stack-12' }, kinds(a), pathRow(a), el('div', { class: 'ap-split' }, folders(a), el('div', { class: 'ap-outcome' }, outcome(a))), setRow(el('label', { for: 'ap-name' }, 'Project name'), null, field('ap-name', a.name, { placeholder: a.defaultName, oninput: fieldsChanged })), a.kind === 'folder' &&
        setRow(el('label', { for: 'ap-skip' }, 'Also skip'), a.alwaysSkipped.length > 0 &&
            el('span', { class: 'meta' }, `${a.alwaysSkipped.join(', ')} are always skipped`), field('ap-skip', a.skip, {
            class: 'field mono',
            placeholder: 'vendor, infra/legacy',
            spellcheck: 'false',
            oninput: fieldsChanged,
        })), a.refusal && el('div', { class: 'fg-danger', role: 'alert' }, a.refusal)), el('div', { class: 'dfoot' }, el('span', { class: 'meta' }, a.kind === 'new'
        ? 'Adds to the daemon’s config; only the new repository is made.'
        : 'Adds to the daemon’s config; nothing in the folders is changed.'), el('span', { class: 'grow' }), el('button', { class: 'btn', onclick: closeAdd }, 'Cancel'), el('button', {
        class: 'btn primary',
        id: 'ap-add',
        disabled: !!a.blocked || a.adding,
        title: a.blocked,
        onclick: add,
    }, a.adding ? 'Adding…' : `Add ${name || 'project'}`))));
}
export function drawAddProject() {
    const a = state.addProject;
    const host = $('addproject');
    if (!a?.open) {
        clearTimeout(fieldsTimer);
        host.hidden = true;
        host.replaceChildren();
        return;
    }
    const focused = document.activeElement instanceof HTMLInputElement ? document.activeElement : null;
    const caret = focused && {
        id: focused.id,
        from: focused.selectionStart,
        to: focused.selectionEnd,
    };
    // A folder newly listed shows its path; one being typed keeps the typing.
    const path = document.getElementById('ap-path');
    if (path instanceof HTMLInputElement && focused !== path)
        path.value = a.path ?? '';
    const opening = host.hidden;
    host.hidden = false;
    host.replaceChildren(addDialog(a));
    const back = caret && document.getElementById(caret.id);
    if (caret && back instanceof HTMLInputElement) {
        back.focus();
        back.setSelectionRange(caret.from, caret.to);
    }
    else if (opening) {
        document.getElementById('ap-path')?.focus();
    }
}
