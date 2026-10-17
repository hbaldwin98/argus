// What the page asks of the window's Rust, and the channel its events come
// down. Every call is one `Call` of `src/connection/page.rs`, which
// `src/connection/commands.rs` dispatches, and the events are that file's
// too: both are typed by the bindings `cargo test` writes.
function invoke(command, args) {
    window.__TAURI__.core.invoke(command, args).catch((err) => {
        console.error(command, args, err);
    });
}
/** Sends a call. It answers nothing: what it changed comes back as events. */
export function call(name, args) {
    invoke('call', { call: { call: name, ...args } });
}
/** Opens the page's channel; Rust sends everything it needs at once. */
export function connect(receive) {
    const events = new window.__TAURI__.core.Channel();
    // One bad message is logged rather than ending the stream. A Channel
    // offers only this setter, not addEventListener.
    // oxlint-disable-next-line unicorn/prefer-add-event-listener
    events.onmessage = (event) => {
        try {
            receive(event);
        }
        catch (err) {
            console.error(err);
        }
    };
    invoke('connect', { events });
}
