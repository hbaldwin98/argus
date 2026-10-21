// Which browsers there are, as Rust last said: the one place the overview,
// the sidebar and the grid's browser cells read them from.
let page = { browsers: [] };
export function setBrowsers(event) {
    page = event;
}
export function browsers() {
    return page.browsers;
}
export function byPane(pane) {
    return page.browsers.find((b) => b.pane === pane) ?? null;
}
