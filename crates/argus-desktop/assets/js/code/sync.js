// The editor's buffer kept in step with the file on disk, which the daemon
// referees: what of the buffer is the person's and not yet written, what is
// in flight, and how a write by anything else — an agent — is folded in.
// The daemon writes a change only onto the version it was made on, so a
// change made while an agent wrote is made again on top of the agent's.
// Where both touched the same text nothing is guessed: the disk's change is
// held aside until the person says which one stands. Nothing here touches
// the page or the window; `editor` owns the view and the calls.
import { ChangeSet, Text } from '../vendor/codemirror.js';
/** A change set as the protocol carries it: positioned in the text before
 *  it, sorted, in UTF-16 units — what CodeMirror counts. */
export function toWire(changes) {
    const out = [];
    changes.iterChanges((from, to, _fromB, _toB, inserted) => {
        out.push({ from, to, insert: inserted.toString() });
    });
    return out;
}
export function fromWire(changes, length) {
    return ChangeSet.of(changes, length);
}
/** Each run a change set replaces in the text before it, as `[from, to]`. */
function spans(changes) {
    const out = [];
    changes.iterChangedRanges((from, to) => out.push([from, to]));
    return out;
}
/** Whether two runs of the same text touch the same characters. Runs that
 *  only meet end to end do not: one line typed, the next rewritten. Two
 *  insertions at one place do, since which goes first is a choice. */
function clash([af, at], [bf, bt]) {
    if (af < bt && bf < at)
        return true;
    if (af === at && bf === bt)
        return af === bf;
    if (af === at)
        return bf < af && af < bt;
    if (bf === bt)
        return af < bf && bf < at;
    return false;
}
/** `changes` split into the ones that clash with none of `against` and
 *  the ones that clash with one, each still a set on the same text. */
function split(changes, against, length) {
    const theirs = spans(against);
    const clear = [];
    const clashing = [];
    changes.iterChanges((from, to, _fromB, _toB, inserted) => {
        const change = { from, to, insert: inserted.toString() };
        (theirs.some((span) => clash([from, to], span)) ? clashing : clear).push(change);
    });
    return [ChangeSet.of(clear, length), ChangeSet.of(clashing, length)];
}
/** The single change that turns `a` into `b`: what they share at each end
 *  kept. Used only where no diff came from the daemon — a file opened again
 *  after the connection went. */
export function between(a, b) {
    if (a === b)
        return [];
    let start = 0;
    const most = Math.min(a.length, b.length);
    while (start < most && a.charCodeAt(start) === b.charCodeAt(start))
        start++;
    let end = 0;
    while (end < most - start &&
        a.charCodeAt(a.length - 1 - end) === b.charCodeAt(b.length - 1 - end))
        end++;
    // Never between the halves of a surrogate pair.
    if (start > 0 && isHigh(a.charCodeAt(start - 1)))
        start--;
    if (end > 0 && isHigh(a.charCodeAt(a.length - end - 1)))
        end--;
    return [{ from: start, to: a.length - end, insert: b.slice(start, b.length - end) }];
}
function isHigh(unit) {
    return unit >= 0xd800 && unit < 0xdc00;
}
export class Synced {
    /** The disk's text at `version`, or while a clash is held, the text the
     *  held changes are made on. */
    base;
    /** The daemon's latest version of the file. */
    version;
    /** Written, and waiting on the daemon: `base` to what was sent. */
    sent = null;
    /** Typed since: what was sent, or `base`, to the buffer. */
    local;
    /** The disk's changes that clash with the person's, made on `base`. */
    held = null;
    constructor(text, version) {
        // Split as CodeMirror splits a document, so both count a line break
        // as one position.
        this.base = Text.of(text.split(/\r\n?|\n/));
        this.version = version;
        this.local = ChangeSet.empty(this.base.length);
    }
    /** Everything not yet on disk: `base` to the buffer. */
    mine() {
        return this.sent ? this.sent.compose(this.local) : this.local;
    }
    /** The buffer as it should stand, for a test to compare with the disk. */
    doc() {
        return this.mine().apply(this.base);
    }
    /** The person typed. */
    typed(changes) {
        this.local = this.local.compose(changes);
    }
    /** Whether anything typed is still not written. */
    pending() {
        return this.sent !== null || !this.local.empty;
    }
    /** Whether a clash waits on the person. */
    clashing() {
        return this.held !== null;
    }
    /** What to write now, made on `version`: nothing while a write is in
     *  flight, a clash is held, or nothing was typed. */
    take() {
        if (this.sent || this.held || this.local.empty)
            return null;
        this.sent = this.local;
        this.local = ChangeSet.empty(this.sent.newLength);
        return toWire(this.sent);
    }
    /** The write made on `base` is on disk as `version`. */
    saved(base, version) {
        if (!this.sent || base !== this.version)
            return;
        this.base = this.sent.apply(this.base);
        this.version = version;
        this.sent = null;
    }
    /** The write made on `base` failed for `reason`: it is typed again, to
     *  go with the next. A refusal with no reason followed a change the
     *  buffer has already taken, which took the write back with it. */
    refused(base, reason) {
        if (reason === null || !this.sent || base !== this.version)
            return;
        this.local = this.sent.compose(this.local);
        this.sent = null;
    }
    /** The connection went, and the write in flight with it. */
    lost() {
        this.local = this.mine();
        this.sent = null;
    }
    /** The disk changed by `changes` from `from` to `version`: the change
     *  for the buffer, or `null` when it was not made on the version held
     *  and the file has to be opened again. */
    changed(from, version, changes) {
        if (from !== this.version)
            return null;
        const disk = this.held ? this.held.apply(this.base) : this.base;
        const step = fromWire(changes, disk.length);
        this.version = version;
        return this.fold(this.held ? this.held.compose(step) : step);
    }
    /** The file opened again, as `text` at `version`: whatever it became
     *  while the connection was down is folded in as one change. */
    reopened(text, version) {
        this.lost();
        const disk = (this.held ? this.held.apply(this.base) : this.base).toString();
        const step = fromWire(between(disk, text), disk.length);
        this.version = version;
        return this.fold(this.held ? this.held.compose(step) : step);
    }
    /** Takes `theirs`, the disk's changes on `base`, into the buffer: the
     *  ones clear of the person's at once, the rest held. */
    fold(theirs) {
        const mine = this.mine();
        this.sent = null;
        const [clear, clashing] = split(theirs, mine, this.base.length);
        const shown = clear.map(mine, true);
        const old = this.base;
        this.base = clear.apply(old);
        this.local = mine.map(clear);
        this.held = clashing.empty ? null : clear.invert(old).compose(theirs);
        return shown;
    }
    /** Where the held changes fall in the buffer, and what each would put
     *  there. */
    clashes() {
        if (!this.held)
            return [];
        const mine = this.local;
        const out = [];
        this.held.iterChanges((from, to, _fromB, _toB, inserted) => {
            out.push({
                from: mine.mapPos(from, -1),
                to: mine.mapPos(to, 1),
                theirs: inserted.toString(),
            });
        });
        return out;
    }
    /** The person's word on a held clash. Keeping theirs gives up the
     *  person's changes that clash and takes the disk's whole; keeping mine
     *  writes the buffer as it stands over the disk's. The change for the
     *  buffer, or `null` when it stays as it is. */
    resolve(keep) {
        const held = this.held;
        if (!held)
            return null;
        const mine = this.local;
        const disk = held.apply(this.base);
        this.held = null;
        if (keep === 'mine') {
            this.local = held.invert(this.base).compose(mine);
            this.base = disk;
            return null;
        }
        const [kept] = split(mine, held, this.base.length);
        const rebased = kept.map(held);
        const shown = mine.invert(this.base).compose(held).compose(rebased);
        this.local = rebased;
        this.base = disk;
        return shown;
    }
}
