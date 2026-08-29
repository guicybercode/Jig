export const CANVAS_STAGE_WIDTH = 12_000;
export const CANVAS_STAGE_HEIGHT = 12_000;
export const CANVAS_ORIGIN_X = 3_000;
export const CANVAS_ORIGIN_Y = 3_000;
export const INITIAL_VIEW_CENTER = { x: 520, y: 320 } as const;

/** Converts persisted world coordinates into the positive scroll-stage space. */
export function toStagePoint(point: {
  readonly x: number;
  readonly y: number;
}): { readonly x: number; readonly y: number } {
  return {
    x: CANVAS_ORIGIN_X + point.x,
    y: CANVAS_ORIGIN_Y + point.y,
  };
}
