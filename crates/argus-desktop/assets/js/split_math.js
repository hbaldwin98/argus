// The arithmetic of the Workspace's grid of panes on the page: where each
// pane's rectangle falls, what dragging a gutter does to the two panes it
// parts, and which zone of a pane a dropped header is over. It reads
// nothing from the page, so it is tested under node; `split.ts` holds the
// layout's state and draws it.
/** Every pane's rectangle as fractions of the grid, row by row, from the
 *  rows' heights and each row's widths (both in thousandths, as Rust
 *  sends them). */
export function cellRects(heights, widths) {
    let y = 0;
    return heights.map((height, r) => {
        let x = 0;
        const h = height / 1000;
        const row = (widths[r] ?? []).map((width) => {
            const w = width / 1000;
            const rect = { x, y, w, h };
            x += w;
            return rect;
        });
        y += h;
        return row;
    });
}
/** The sizes of a row's panes, in pixels, once the gutter after pane `at`
 *  is dragged by `delta`. Only the two panes either side change, and
 *  neither goes below `min`. A pair with room for less than two minimums
 *  stays as it is. */
export function dragGutter(sizes, at, delta, min) {
    const before = sizes[at];
    const after = sizes[at + 1];
    const next = sizes.slice();
    if (before === undefined || after === undefined)
        return next;
    const total = before + after;
    if (total < 2 * min)
        return next;
    const first = Math.min(total - min, Math.max(min, before + delta));
    next[at] = first;
    next[at + 1] = total - first;
    return next;
}
/** Sizes in pixels as whole thousandths summing to exactly 1000, the
 *  rounding left over going to the largest remainders. */
export function toThousandths(sizes) {
    const total = sizes.reduce((sum, s) => sum + s, 0);
    if (total <= 0)
        return sizes.map(() => Math.floor(1000 / Math.max(1, sizes.length)));
    const exact = sizes.map((s) => (s / total) * 1000);
    const whole = exact.map((e) => Math.floor(e));
    let left = 1000 - whole.reduce((sum, w) => sum + w, 0);
    const order = exact
        .map((e, i) => ({ i, rest: e - Math.floor(e) }))
        .toSorted((a, b) => b.rest - a.rest || a.i - b.i);
    for (const { i } of order) {
        if (left <= 0)
            break;
        whole[i] = (whole[i] ?? 0) + 1;
        left -= 1;
    }
    return whole;
}
/** The zone of a pane a pointer at `[fx, fy]`, fractions of the pane, is
 *  over: the nearest edge when it is within a quarter of it, else the
 *  centre. */
export function dropZone(fx, fy) {
    const edges = [
        [fx, 'left'],
        [1 - fx, 'right'],
        [fy, 'top'],
        [1 - fy, 'bottom'],
    ];
    let [near, zone] = edges[0] ?? [1, 'centre'];
    for (const [distance, candidate] of edges) {
        if (distance < near)
            [near, zone] = [distance, candidate];
    }
    return near < 0.25 ? zone : 'centre';
}
/** What `zone` of a pane is lit while a header is over it, as fractions of
 *  the pane. */
export function zoneRect(zone) {
    switch (zone) {
        case 'left':
            return { x: 0, y: 0, w: 0.5, h: 1 };
        case 'right':
            return { x: 0.5, y: 0, w: 0.5, h: 1 };
        case 'top':
            return { x: 0, y: 0, w: 1, h: 0.5 };
        case 'bottom':
            return { x: 0, y: 0.5, w: 1, h: 0.5 };
        case 'centre':
            return { x: 0, y: 0, w: 1, h: 1 };
    }
}
