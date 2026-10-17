// Dragging an edge — the rail's, the views panel's, an open feature's
// outline's and brief's — to make what it bounds bigger or smaller. The column follows the pointer at most once a
// frame, keeps the point it was grabbed at under the pointer, and holds
// the width it was let go at until Rust sends back the one it keeps; the
// pane beside it reports its size once, when the drag ends (`screen`).

/** Whether an edge is held now, so the panes wait to report their size. */
export function dragging(): boolean {
  return document.body.classList.contains('resizing');
}

interface Edge {
  /** The size with the pointer at `at`, along the axis the edge moves on. */
  sizeAt: (at: number) => number;
  /** Moves up and down, sizing a height, rather than across. */
  vertical?: boolean;
  /** Draws the column at `width`, in CSS pixels. */
  draw: (width: number) => void;
  /** The width let go at, for Rust; `null` to go back to the usual one. */
  report: (width: number | null) => void;
}

export function dragEdge(sash: HTMLElement, column: HTMLElement, edge: Edge): void {
  sash.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const at = (m: MouseEvent): number => (edge.vertical ? m.clientY : m.clientX);
    const box = column.getBoundingClientRect();
    // Where in the edge it was grabbed, so the column does not jump to put
    // its border under the pointer.
    const offset = (edge.vertical ? box.height : box.width) - edge.sizeAt(at(e));
    let width = Math.round(edge.sizeAt(at(e)) + offset);
    let frame = 0;
    document.body.classList.add('resizing');
    sash.classList.add('held');
    const move = (m: MouseEvent): void => {
      width = Math.round(edge.sizeAt(at(m)) + offset);
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        edge.draw(width);
      });
    };
    const up = (): void => {
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
