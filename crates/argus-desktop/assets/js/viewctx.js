// What every block of an agent's view is drawn with: which view it is in,
// so an action or a reference inside it knows whom to ask, and the path of
// the block, which keys what the page alone remembers about it (a chosen
// tab, an opened row) across a redraw.
/** The context of the block at `index` under `ctx`. */
export function at(ctx, index) {
    return { ...ctx, path: `${ctx.path}.${index}` };
}
