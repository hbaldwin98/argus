// What a saved view's blocks are drawn with: the view it is, the column of
// the panel it is drawn in (so a link followed in it knows where to put its
// target), and the saved views drawn around it, which stops an embed that
// loops. A saved view has no pane: a reference in it is read against the
// checkout on screen, and a press is relayed to whichever agent last saved
// it.
import { state } from './state.js';
export function savedCtx(view, host, chain, depth, place) {
    return {
        pane: 0,
        id: view.id,
        // Per column and per embed, so a chosen tab or an opened row is kept
        // where the person made the choice.
        path: `saved/${place}/${chain.join('>')}`,
        drawn: (source) => view.diagrams.find((d) => d.source === source),
        pressed: view.pressed,
        canPress: state.saved?.canPress ?? false,
        ...(host ? { saved: host } : {}),
        chain,
        depth,
    };
}
