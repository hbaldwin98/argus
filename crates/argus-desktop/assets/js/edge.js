// Dragging a side column's edge — the rail's, the views panel's — to make
// it wider or narrower. The column follows the pointer at most once a
// frame, keeps the point it was grabbed at under the pointer, and holds
// the width it was let go at until Rust sends back the one it keeps; the
// pane beside it reports its size once, when the drag ends (`screen`).
/** Whether an edge is held now, so the panes wait to report their size. */
export function dragging() {
    return document.body.classList.contains('resizing');
}
export function dragEdge(sash, column, edge) {
    sash.addEventListener('mousedown', (e) => {
        if (e.button !== 0)
            return;
        e.preventDefault();
        // Where in the edge it was grabbed, so the column does not jump to put
        // its border under the pointer.
        const offset = column.getBoundingClientRect().width - edge.widthAt(e.clientX);
        let width = Math.round(edge.widthAt(e.clientX) + offset);
        let frame = 0;
        document.body.classList.add('resizing');
        sash.classList.add('held');
        const move = (m) => {
            width = Math.round(edge.widthAt(m.clientX) + offset);
            if (frame)
                return;
            frame = requestAnimationFrame(() => {
                frame = 0;
                edge.draw(width);
            });
        };
        const up = () => {
            window.removeEventListener('mousemove', move);
            window.removeEventListener('mouseup', up);
            cancelAnimationFrame(frame);
            edge.draw(width);
            document.body.classList.remove('resizing');
            sash.classList.remove('held');
            edge.report(Math.max(0, width));
        };
        window.addEventListener('mousemove', move);
        window.addEventListener('mouseup', up);
    });
    sash.addEventListener('dblclick', () => edge.report(null));
}
