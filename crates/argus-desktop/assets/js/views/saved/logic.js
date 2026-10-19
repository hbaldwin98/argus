// The rules for saved views that need no document: whether an embed is
// drawn, how the sidebar groups, nests and filters the list, and which
// views a feature's stage offers. Kept apart so node can test them.
/** How many views deep an embed is followed. This is
 *  `argus_protocol::MAX_EMBED_DEPTH` written a second time, since the page
 *  draws without asking Rust; a drift costs a line saying "too deep" at the
 *  wrong depth, never a loop, because the cycle check does not depend on it.
 *  Change the two together. */
export const MAX_EMBED_DEPTH = 3;
/** Whether the saved view `target` is drawn inline. `chain` holds the saved
 *  views being drawn, outermost first, and `depth` how many embeds were
 *  followed to get here. A view already being drawn is a cycle and not
 *  drawn again; past the depth is not drawn; one the window does not hold
 *  is missing, since an embed may name a view saved later. */
export function embedVerdict(target, chain, depth, held) {
    if (chain.includes(target))
        return 'cycle';
    if (depth >= MAX_EMBED_DEPTH)
        return 'deep';
    return held(target) ? 'draw' : 'missing';
}
/** The line drawn in place of an embed that is not drawn. */
export const EMBED_WORDS = {
    cycle: 'view embeds itself',
    deep: 'embedded too deep',
    missing: 'view not found',
};
/** The views in groups by scope, in the order Rust sent them, which is
 *  already global first, then repositories, then features. With `hereOnly`,
 *  the ones about where the person is. */
export function grouped(views, hereOnly) {
    const groups = [];
    for (const view of views) {
        if (hereOnly && !view.here)
            continue;
        const last = groups[groups.length - 1];
        if (last && last.label === view.scope.label)
            last.views.push(view);
        else
            groups.push({ label: view.scope.label, views: [view] });
    }
    return groups;
}
/** A title as the sidebar lists it under its group's label: an agent often
 *  names a view after its repository too ("reliab: the change flow"), and
 *  under "reliab" that is said twice. A title that is nothing but the label
 *  is kept whole. */
export function underGroup(title, group) {
    const lead = title.slice(0, group.length);
    if (!group || lead.toLowerCase() !== group.toLowerCase())
        return title;
    const rest = /^\s*[:\-–—·|/]\s*(.+)$/u.exec(title.slice(group.length));
    return rest?.[1] ?? title;
}
/** A group's views as the sidebar lists them: each under the view Rust
 *  named in `under`, in order. One whose `under` the group does not show,
 *  filtered out or of another scope, stands at the top instead. */
export function nested(views) {
    const nodes = new Map(views.map((view) => [view.id, { view, inner: [] }]));
    const top = [];
    for (const node of nodes.values()) {
        const parent = node.view.under === null ? undefined : nodes.get(node.view.under);
        (parent ? parent.inner : top).push(node);
    }
    return top;
}
/** How many views a node holds under it, at every depth. */
export function count(node) {
    return node.inner.reduce((n, inner) => n + 1 + count(inner), 0);
}
/** The views scoped to or attached to feature `number` of `repository`. */
export function forFeature(views, repository, number) {
    if (repository === null)
        return [];
    return views.filter((v) => v.features.some((f) => f.repository === repository && f.number === number));
}
