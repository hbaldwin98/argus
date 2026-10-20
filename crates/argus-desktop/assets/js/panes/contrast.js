// The text colour that stays readable on a cell's own background: a program
// that paints a coloured background (a diff's red and green) picks its text
// colour for its own palette, and the theme's default text can be as dark as
// that background in a light flavour. The same floor a terminal like xterm
// calls minimumContrastRatio.
/** WCAG's floor for body text. */
const MINIMUM = 4.5;
function parse(colour) {
    const hex = /^#([0-9a-f]{6})$/i.exec(colour);
    if (hex?.[1]) {
        const n = parseInt(hex[1], 16);
        return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
    }
    const rgb = /^rgb\((\d+),\s*(\d+),\s*(\d+)\)$/.exec(colour);
    if (rgb)
        return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3])];
    return null;
}
function linear(c) {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}
function luminance([r, g, b]) {
    return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}
function ratio(a, b) {
    const [x, y] = [luminance(a), luminance(b)];
    return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
const cache = new Map();
/** `fore` on `back`, moved toward black or white, whichever reads better on
 *  `back`, until it reaches the floor. A colour it cannot parse is left. */
export function readable(fore, back) {
    const key = `${fore}|${back}`;
    const known = cache.get(key);
    if (known !== undefined)
        return known;
    let out = fore;
    const f = parse(fore);
    const b = parse(back);
    if (f && b && ratio(f, b) < MINIMUM) {
        const target = ratio([0, 0, 0], b) >= ratio([255, 255, 255], b) ? [0, 0, 0] : [255, 255, 255];
        for (let t = 0.1; t <= 1.0001; t += 0.1) {
            const mixed = f.map((c, i) => Math.round(c + ((target[i] ?? 0) - c) * t));
            out = `rgb(${mixed[0]},${mixed[1]},${mixed[2]})`;
            if (ratio(mixed, b) >= MINIMUM)
                break;
        }
    }
    if (cache.size > 2000)
        cache.clear();
    cache.set(key, out);
    return out;
}
