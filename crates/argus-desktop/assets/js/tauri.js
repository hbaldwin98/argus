// What the page asks of the window's Rust, and the channel its events come
// down. Every command here is one in `src/commands.rs`; the events are
// `src/page.rs`'s, typed by the bindings `cargo test` writes.
/** Sends a call. It answers nothing: what it changed comes back as events. */
export function call(command, args) {
    window.__TAURI__.core.invoke(command, args).catch((err) => {
        console.error(command, err);
    });
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
    call('connect', { events });
}
