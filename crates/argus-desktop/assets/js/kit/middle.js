// A middle click closes what it lands on, as a browser's tab does: a
// dock's tab, a view's tab or strip, a console tab, a pane's row on the
// rail. Each says what its close is; this only hears the button.
/** Makes a middle click on `node` run `close`, and returns `node`. */
export function closesOnMiddle(node, close) {
    // Held down, the middle button starts the webview's autoscroll, which
    // would swallow the click that follows.
    node.addEventListener('mousedown', (e) => {
        if (e.button === 1)
            e.preventDefault();
    });
    node.addEventListener('auxclick', (e) => {
        if (e.button !== 1)
            return;
        e.preventDefault();
        e.stopPropagation();
        close();
    });
    return node;
}
