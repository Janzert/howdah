// User drawings on the board: square highlights and arrows. Purely visual;
// never sent to the backend.
import type { Square } from '../bindings/Square';

export type AnnotationColor = 'primary' | 'red' | 'blue';

export interface Arrow {
  from: Square;
  to: Square;
  color: AnnotationColor;
}

export interface Highlight {
  square: Square;
  color: AnnotationColor;
}

export function colorFor(e: { shiftKey: boolean; ctrlKey: boolean; altKey: boolean }): AnnotationColor {
  if (e.shiftKey) return 'red';
  if (e.ctrlKey || e.altKey) return 'blue';
  return 'primary';
}

export class Annotations {
  highlights = $state<Highlight[]>([]);
  arrows = $state<Arrow[]>([]);
  /** Arrow being drawn with a right-drag. */
  preview = $state<Arrow | null>(null);

  toggleHighlight(square: Square, color: AnnotationColor) {
    const i = this.highlights.findIndex((h) => h.square === square);
    if (i >= 0 && this.highlights[i].color === color) this.highlights.splice(i, 1);
    else if (i >= 0) this.highlights[i].color = color;
    else this.highlights.push({ square, color });
  }

  toggleArrow(from: Square, to: Square, color: AnnotationColor) {
    const i = this.arrows.findIndex((a) => a.from === from && a.to === to);
    if (i >= 0 && this.arrows[i].color === color) this.arrows.splice(i, 1);
    else if (i >= 0) this.arrows[i].color = color;
    else this.arrows.push({ from, to, color });
  }

  clear() {
    this.highlights = [];
    this.arrows = [];
    this.preview = null;
  }
}
