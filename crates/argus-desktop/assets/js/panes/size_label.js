// What the Workspace header says of the grid: where a parked view is, and the
// size only for a moment after it changed, since it is not news at rest.
export function sizeLabel({ cols, rows, parked, shownUntil, now }) {
    if (!cols)
        return '';
    // A parked view leads with how far back it is: otherwise it looks like
    // a pane gone quiet.
    const back = parked ? `↑ ${parked.offset}/${parked.depth}` : '';
    if (now >= shownUntil)
        return back;
    return back ? `${back} · ${cols} × ${rows}` : `${cols} × ${rows}`;
}
