// The shortcuts sheet's sections: the keymap Rust sends, and the extensions'
// headings, dealt into one section per place with the pairs of bindings that
// are one thought written once. Pure, so the sheet stays generated from the
// keymap and node can hold it to listing every binding.
/** Bindings that are one row. The caps come from the chords themselves, so a
 *  rebound key reads true; a set whose chords disagree stays as it was. */
const MERGES = [
    {
        actions: ['otherSide', 'previousSide'],
        label: 'the keys to the next / previous pane on screen',
        arrows: false,
    },
    {
        actions: ['focusLeft', 'focusRight', 'focusUp', 'focusDown'],
        label: 'the keys to the pane that way',
        arrows: true,
    },
    {
        actions: ['movePaneLeft', 'movePaneRight', 'movePaneUp', 'movePaneDown'],
        label: 'move the pane that way, trading places',
        arrows: true,
    },
];
const ARROW = /\s?([←→↑↓])$/;
/** `← → ↑ ↓`, or `Shift + arrow` when the four hold a modifier; null when
 *  they do not share one. */
function arrowCap(chords, namer) {
    const parts = chords.map((chord) => {
        const written = namer.press({ ...chord, leader: false });
        const m = ARROW.exec(written);
        return m ? { prefix: written.slice(0, m.index), glyph: m[1] ?? '' } : null;
    });
    if (parts.some((p) => p === null))
        return null;
    const found = parts;
    const prefix = found[0]?.prefix ?? '';
    if (found.some((p) => p.prefix !== prefix))
        return null;
    return [prefix ? `${prefix} + arrow` : found.map((p) => p.glyph).join(' ')];
}
function keysOf(chords, namer, bare) {
    return chords.map((chord) => {
        const written = namer.press({ ...chord, leader: false });
        return chord.leader && !bare ? [namer.leader, written] : [written];
    });
}
/** The rows of one source: a merged set is one row where its first member
 *  stood. `bare` writes a chorded key without its leader. */
function rowsOf(source, namer, bare) {
    const rows = [];
    const done = new Set();
    source.bindings.forEach((binding, i) => {
        if (done.has(i))
            return;
        const merge = MERGES.find((m) => binding.action !== undefined && m.actions[0] === binding.action);
        if (merge) {
            const at = merge.actions.map((a) => source.bindings.findIndex((b) => b.action === a));
            if (at.every((j) => j >= 0)) {
                const chords = at.flatMap((j) => source.bindings[j]?.chords ?? []);
                const cap = merge.arrows ? arrowCap(chords, namer) : undefined;
                if (cap !== null) {
                    at.forEach((j) => done.add(j));
                    rows.push({ label: merge.label, keys: cap ? [cap] : keysOf(chords, namer, bare) });
                    return;
                }
            }
        }
        rows.push({ label: binding.label, keys: keysOf(binding.chords, namer, bare) });
    });
    return rows;
}
/** A pane key that follows the leader: all but the leader sent on. */
function keyed(b) {
    return b.action !== 'leader' && b.chords.some((c) => c.leader);
}
/** One section per source, in the order given. The pane's chorded keys lose
 *  their leader and the rest wait under "without"; the leader sent on keeps
 *  both its presses, it being the one chord that is the leader twice. */
export function sections(sources, namer) {
    return sources.map((source) => {
        const chorded = source.bindings.some((b) => b.chords.some((c) => c.leader));
        if (!chorded) {
            return {
                id: source.place,
                title: source.title,
                rows: rowsOf(source, namer, false),
                without: [],
                chorded,
            };
        }
        const rows = rowsOf({ ...source, bindings: source.bindings.filter(keyed) }, namer, true);
        const without = rowsOf({ ...source, bindings: source.bindings.filter((b) => !keyed(b)) }, namer, false);
        return { id: source.place, title: source.title, rows, without, chorded };
    });
}
function text(row) {
    return `${row.label} ${row.keys.map((k) => k.join(' ')).join(' ')}`.toLowerCase();
}
/** The sections with a row that has every word of `query` in its action or
 *  its key, narrowed to those rows. An empty query is every section. */
export function matching(all, query) {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (words.length === 0)
        return [...all];
    const keep = (rows) => rows.filter((row) => words.every((w) => text(row).includes(w)));
    return all
        .map((s) => ({ ...s, rows: keep(s.rows), without: keep(s.without) }))
        .filter((s) => s.rows.length + s.without.length > 0);
}
/** The section for where the person is: the layer with the keys, as the
 *  window asks them, then the stage. Anywhere when nothing is more exact
 *  or the keymap has no such section. */
export function sectionFor(where, ids) {
    const stage = {
        feature: 'feature',
        checkouts: 'checkouts',
        code: 'code',
    };
    const wanted = [
        where.dialog,
        where.pane ? 'pane' : null,
        where.rail ? 'rail' : null,
        where.diagram ? 'diagram' : null,
        where.outline ? 'outline' : null,
        stage[where.stage] ?? null,
    ];
    return wanted.find((id) => id !== null && ids.includes(id)) ?? 'anywhere';
}
