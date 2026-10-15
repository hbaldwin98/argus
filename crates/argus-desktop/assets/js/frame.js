// The window's own title strip, the system's being put away: a press on
// its empty space moves the window, a double press maximizes it, and its
// buttons minimize, maximize and close it. macOS draws its own buttons
// over the strip, so there the page's are hidden and room is left.
import { $ } from './dom.js';
import { call } from './tauri.js';
/** How long the pointer rests on maximize before Windows 11's snap
 *  layouts open, as long as the system's own button waits. */
const SNAP_DELAY_MS = 600;
const MAC = navigator.userAgent.includes('Mac');
const WINDOWS = navigator.userAgent.includes('Windows');
export function listen() {
    document.body.classList.toggle('mac', MAC);
    document.addEventListener('mousedown', (e) => {
        // The strip and its words move the window; a button on it keeps its press.
        if (e.button !== 0 || !(e.target instanceof Element))
            return;
        if (!e.target.closest('.drag') || e.target.closest('button, input, a, [role="menu"]'))
            return;
        e.preventDefault();
        if (e.detail === 2)
            call('toggle_maximize', {});
        else
            call('drag_window', {});
    });
    $('win-min').addEventListener('click', () => call('minimize', {}));
    $('win-max').addEventListener('click', () => call('toggle_maximize', {}));
    $('win-close').addEventListener('click', () => call('close_window', {}));
    let snap;
    if (WINDOWS) {
        $('win-max').addEventListener('mouseenter', () => {
            snap = setTimeout(() => call('snap_layouts', {}), SNAP_DELAY_MS);
        });
        $('win-max').addEventListener('mouseleave', () => clearTimeout(snap));
        $('win-max').addEventListener('mousedown', () => clearTimeout(snap));
    }
    window.addEventListener('resize', drawMaximized);
    drawMaximized();
}
/** Maximize turns to restore while the window fills the screen. Read from
 *  the window's size: the page asks nothing of Rust to draw a button. */
function drawMaximized() {
    const maximized = window.outerWidth >= screen.availWidth && window.outerHeight >= screen.availHeight;
    document.body.classList.toggle('maximized', maximized);
    $('win-max').setAttribute('aria-label', maximized ? 'Restore' : 'Maximize');
}
