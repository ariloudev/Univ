// Names drawn over the view without overlap (see-the-structures): nearer names first, each kept
// only where it clears every name already kept.

/** A name's box on screen, centred on its place, and its place's depth. */
export interface LabelBox {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  depth: number;
}

const overlaps = (a: LabelBox, b: LabelBox): boolean =>
  Math.abs(a.x - b.x) * 2 < a.width + b.width && Math.abs(a.y - b.y) * 2 < a.height + b.height;

/** The ids of the names to draw: the nearest first, each clear of those before it. */
export function clearLabels(boxes: readonly LabelBox[]): Set<string> {
  const kept: LabelBox[] = [];
  for (const box of [...boxes].sort((p, q) => p.depth - q.depth)) {
    if (!kept.some((other) => overlaps(box, other))) kept.push(box);
  }
  return new Set(kept.map((box) => box.id));
}
