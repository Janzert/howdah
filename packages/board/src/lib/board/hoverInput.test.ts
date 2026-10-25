import { describe, expect, it } from 'vitest';
import { squareXY } from '../geometry';
import { nearestNeighbour, stepChoice, type StepArrow } from './hoverInput';

const sq = (name: string) => (Number(name[1]) - 1) * 8 + 'abcdefgh'.indexOf(name[0]);
/** A point inside `name`, offset from its center by (dx, dy) board units. */
function at(name: string, dx: number, dy: number, flipped = false) {
  const c = squareXY(sq(name), flipped);
  return { x: c.x + 50 + dx, y: c.y + 50 + dy };
}

describe('nearestNeighbour', () => {
  it('picks the square across the nearest edge', () => {
    expect(nearestNeighbour(at('d4', 0, -30), false)).toBe(sq('d5')); // up the screen is north
    expect(nearestNeighbour(at('d4', 30, 5), false)).toBe(sq('e4'));
    expect(nearestNeighbour(at('d4', 0, -30, true), true)).toBe(sq('d3')); // flipped: up is south
    expect(nearestNeighbour(at('d4', 0, 0), false)).toBeNull();
    expect(nearestNeighbour(at('a4', -30, 0), false)).toBeNull(); // off the board
  });
});

describe('stepChoice', () => {
  const legal: Record<number, StepArrow[]> = {
    [sq('d4')]: [
      { to: sq('d5'), enemy: false },
      { to: sq('e4'), enemy: false },
    ],
  };
  const steps = (s: number) => legal[s] ?? [];

  it('offers the piece under the pointer stepping toward the nearest edge', () => {
    expect(stepChoice(at('d4', 0, -30), false, steps)).toEqual({ from: sq('d4'), to: sq('d5'), enemy: false });
    // Toward c4, which isn't legal: the piece is shown without a step.
    expect(stepChoice(at('d4', -30, 0), false, steps)).toEqual({ from: sq('d4'), to: null, enemy: false });
  });

  it('offers a neighbour stepping into an empty square', () => {
    // Near the bottom edge of d5: the d4 piece can step up into it.
    expect(stepChoice(at('d5', 0, 30), false, steps)).toEqual({ from: sq('d4'), to: sq('d5'), enemy: false });
    expect(stepChoice(at('d5', 0, -30), false, steps)).toBeNull();
  });

  it('waits for steps still being fetched', () => {
    expect(stepChoice(at('d4', 0, -30), false, () => undefined)).toBeNull();
  });
});
