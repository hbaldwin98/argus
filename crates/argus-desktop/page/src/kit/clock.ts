// Times the daemon sends as a harness wrote them (RFC 3339), said in the
// person's own clock: `13:52 today`, `13:52 yesterday`, `Oct 12 13:52`.
// Here rather than in Rust because only the page knows its time zone.

function parsed(at: string | null): Date | null {
  if (!at) return null;
  const date = new Date(at);
  return Number.isNaN(date.getTime()) ? null : date;
}

function time(date: Date): string {
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
}

/** The day `date` falls on, from where the person is looking. */
function day(date: Date): string {
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (date.toDateString() === today.toDateString()) return 'today';
  if (date.toDateString() === yesterday.toDateString()) return 'yesterday';
  return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

/** `at` as a time and its day, or null when it says none. */
export function clock(at: string | null): string | null {
  const date = parsed(at);
  return date && `${time(date)} ${day(date)}`;
}

/** A time alone, for a line of something said: its day is the session's. */
export function hhmm(at: string | null): string | null {
  const date = parsed(at);
  return date && time(date);
}

/** From `start` to `end`, the day said once when both fall on it. */
export function span(start: string | null, end: string | null): string | null {
  const [from, to] = [parsed(start), parsed(end)];
  if (!from || !to) return clock(start ?? end);
  if (from.toDateString() === to.toDateString()) return `${time(from)}–${time(to)} ${day(to)}`;
  return `${clock(start)} – ${clock(end)}`;
}
