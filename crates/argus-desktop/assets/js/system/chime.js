// The sound an agent coming to need you makes. Whether it plays is Rust's
// call (`session`); this only plays it.
//
// Synthesized rather than a file, so there is no asset to ship or load: two
// soft notes, rising, short enough to hear without being asked to stop.
let context = null;
/** A webview may hold audio until the page has been used, so the context
 *  is made, or woken, on the person's first gesture. */
function wake() {
    context ??= new AudioContext();
    if (context.state === 'suspended')
        context.resume().catch(() => { });
}
export function listen() {
    for (const type of ['pointerdown', 'keydown']) {
        document.addEventListener(type, wake, { capture: true, passive: true });
    }
}
export function chime() {
    wake();
    const audio = context;
    if (!audio)
        return;
    const start = audio.currentTime;
    for (const [at, frequency] of [
        [0, 660],
        [0.12, 880],
    ]) {
        const tone = audio.createOscillator();
        const gain = audio.createGain();
        tone.type = 'sine';
        tone.frequency.value = frequency;
        // A quick rise and a long fall: a click at either end otherwise.
        gain.gain.setValueAtTime(0, start + at);
        gain.gain.linearRampToValueAtTime(0.18, start + at + 0.01);
        gain.gain.exponentialRampToValueAtTime(0.001, start + at + 0.35);
        tone.connect(gain).connect(audio.destination);
        tone.start(start + at);
        tone.stop(start + at + 0.4);
    }
}
