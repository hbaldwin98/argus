// Where a drawing sits in the larger view it opens in: how big it starts,
// and how a wheel or a drag moves it. Only numbers, so node can test it.

/** The drawing's scale, and where its top-left corner sits in the stage. */
export interface Zoom {
  scale: number;
  x: number;
  y: number;
}

export const MIN_SCALE = 0.1;
export const MAX_SCALE = 6;
const MARGIN = 24;
/** Below this a label is too small to read, so a drawing tall enough to
 *  need it starts at the stage's width instead and is dragged down. */
const READABLE = 0.75;
/** A small drawing is enlarged to fill the stage, but only so far. */
const MAX_START = 2;

function clamp(scale: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

/** The whole drawing in the stage, centred. */
export function fit(w: number, h: number, stageW: number, stageH: number): Zoom {
  const scale = clamp(Math.min((stageW - 2 * MARGIN) / w, (stageH - 2 * MARGIN) / h, MAX_START));
  return { scale, x: (stageW - w * scale) / 2, y: (stageH - h * scale) / 2 };
}

/** How a drawing opens: whole when it can be read whole, else across the
 *  stage from its top, since a long sequence is read downward. */
export function start(w: number, h: number, stageW: number, stageH: number): Zoom {
  const whole = fit(w, h, stageW, stageH);
  if (whole.scale >= READABLE) return whole;
  const scale = clamp(Math.min((stageW - 2 * MARGIN) / w, 1));
  if (scale <= whole.scale) return whole;
  return { scale, x: (stageW - w * scale) / 2, y: MARGIN };
}

/** Scaled by `factor` about the stage point (`px`, `py`), which stays put
 *  under the pointer. */
export function zoomAt(z: Zoom, factor: number, px: number, py: number): Zoom {
  const scale = clamp(z.scale * factor);
  const k = scale / z.scale;
  return { scale, x: px - (px - z.x) * k, y: py - (py - z.y) * k };
}
