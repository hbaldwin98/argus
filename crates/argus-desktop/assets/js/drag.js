// Dragging a panel to where it goes: a dock's tab, a panel's header or an
// extension's view in the sidebar picked up, the place it would land shown
// under the pointer — the left, right or bottom dock, or the stage for a
// panel that has a page — and the move reported when it is let go, the
// same move the dock menus send. Rust decides what comes of it
// (`src/docks.rs`); nothing here moves a panel itself.
//
// Mouse events rather than the browser's drag and drop, so the preview
// follows the pointer over the panes' canvases as well as the page, and a
// press that never travels stays the click it was.
import { $, el } from './dom.js';
import { call } from './tauri.js';
/** How far the pointer goes before a press is a drag. */
const THRESHOLD = 5;
/** Where a drop at (`x`, `y`) lands: an edge of the work area is its
 *  dock, the bottom of the stage the bottom dock, and the rest the stage
 *  for a panel that has a page. Pure, so the regions are one rule. */
export function targetAt(x, y, work, center, stage) {
    if (x < work.left || x > work.left + work.width || y < work.top || y > work.top + work.height)
        return null;
    const edge = Math.min(160, work.width * 0.2);
    if (x < work.left + edge)
        return 'left';
    if (x > work.left + work.width - edge)
        return 'right';
    if (y > center.top + center.height - Math.min(160, center.height * 0.3))
        return 'bottom';
    return stage ? 'stage' : null;
}
/** The area a drop on `target` would fill, for the preview. */
function landing(target, work, center) {
    const side = Math.min(560, work.width * 0.35);
    switch (target) {
        case 'left':
            return { ...work, width: side };
        case 'right':
            return { ...work, left: work.left + work.width - side, width: side };
        case 'bottom': {
            const height = Math.min(320, center.height * 0.4);
            return { ...center, top: center.top + center.height - height, height };
        }
        case 'stage':
            return center;
    }
}
const WORDS = {
    left: 'Dock on the left',
    right: 'Dock on the right',
    bottom: 'Dock at the bottom',
    stage: 'Open on the stage',
};
function drop(panel, target) {
    if (target !== 'stage')
        call('docks', { gesture: { on: 'move', panel, dock: target } });
    else if (panel === 'console')
        call('console', { gesture: { on: 'tab' } });
    else
        call('extension_mode', { panel, page: true });
}
/** The console has the Console stage and an extension's view its page;
 *  the agents' views and the saved views are only ever docked. */
function hasStage(panel) {
    return panel !== 'views' && !panel.startsWith('saved:');
}
function box(id) {
    return $(id).getBoundingClientRect();
}
/** A drag swallows the click its release would otherwise be. */
let swallowClick = false;
window.addEventListener('click', (e) => {
    if (!swallowClick)
        return;
    swallowClick = false;
    e.preventDefault();
    e.stopPropagation();
}, true);
/** Makes a press on `handle` able to drag `panel`, naming it `title` under
 *  the pointer, and gives the handle back. A press on a button inside it
 *  stays the button's, unless the handle is that button. */
export function draggable(handle, panel, title) {
    handle.addEventListener('mousedown', (down) => {
        if (down.button !== 0)
            return;
        const pressed = down.target instanceof Element ? down.target.closest('button, input, textarea, a') : null;
        if (pressed && pressed !== handle)
            return;
        let dragging = false;
        let target = null;
        const ghost = el('div', { class: 'drag-ghost' }, title);
        const preview = el('div', { class: 'drop-preview' }, el('span', { class: 'drop-words' }));
        const move = (m) => {
            if (!dragging) {
                if (Math.hypot(m.clientX - down.clientX, m.clientY - down.clientY) < THRESHOLD)
                    return;
                dragging = true;
                document.body.classList.add('panel-dragging');
                document.body.append(preview, ghost);
            }
            m.preventDefault();
            ghost.style.left = `${m.clientX + 12}px`;
            ghost.style.top = `${m.clientY + 12}px`;
            const work = box('work');
            const center = box('center');
            target = targetAt(m.clientX, m.clientY, work, center, hasStage(panel));
            preview.hidden = target === null;
            if (target) {
                const at = landing(target, work, center);
                Object.assign(preview.style, {
                    left: `${at.left}px`,
                    top: `${at.top}px`,
                    width: `${at.width}px`,
                    height: `${at.height}px`,
                });
                preview.firstElementChild.textContent = WORDS[target];
            }
        };
        const end = (dropped) => {
            window.removeEventListener('mousemove', move, true);
            window.removeEventListener('mouseup', up, true);
            window.removeEventListener('keydown', key, true);
            if (!dragging)
                return;
            document.body.classList.remove('panel-dragging');
            ghost.remove();
            preview.remove();
            swallowClick = true;
            // The click a release makes comes at once, or not at all when the
            // pointer left the element it was pressed on.
            setTimeout(() => {
                swallowClick = false;
            }, 0);
            if (dropped && target)
                drop(panel, target);
        };
        const up = () => end(true);
        const key = (k) => {
            if (k.key !== 'Escape' || !dragging)
                return;
            k.preventDefault();
            k.stopPropagation();
            end(false);
        };
        window.addEventListener('mousemove', move, true);
        window.addEventListener('mouseup', up, true);
        window.addEventListener('keydown', key, true);
    });
    return handle;
}
