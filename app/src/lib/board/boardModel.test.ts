import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AnimStep } from '../bindings/AnimStep';
import type { PieceView } from '../bindings/PieceView';
import { BoardModel, MAX_BEHIND, STEP_MS } from './boardModel.svelte';

// One gold rabbit, id 1, walking up the a-file one square per "move".
const rabbit = (square: number): PieceView[] => [
  { id: 1, piece: { color: 'gold', kind: 'rabbit' }, square, frozen: false },
];
const step = (from: number, to: number): AnimStep[] => [{ id: 1, from, to, captured: null, restored: null }];

describe('BoardModel', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => setTimeout(() => cb(0), 16));
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('animates a move and settles on the final position', async () => {
    const m = new BoardModel();
    m.snap(rabbit(0));
    const slides = vi.fn();
    m.apply(rabbit(8), step(0, 8), { onSlide: slides });
    expect(m.animating).toBe(true);
    expect(m.pieces[0].square).toBe(8);
    await vi.advanceTimersByTimeAsync(STEP_MS + 50);
    expect(m.animating).toBe(false);
    expect(slides).toHaveBeenCalledTimes(1);
  });

  it("isn't interrupted by an update that doesn't change the board", async () => {
    const m = new BoardModel();
    m.snap(rabbit(0));
    m.apply(rabbit(8), step(0, 8));
    // e.g. the next engine starts thinking right after the move arrives
    m.apply(rabbit(8), []);
    expect(m.animating).toBe(true);
    await vi.advanceTimersByTimeAsync(STEP_MS + 50);
    expect(m.animating).toBe(false);
    expect(m.pieces[0].square).toBe(8);
  });

  it('plays queued moves in order, faster when behind', async () => {
    const m = new BoardModel();
    m.snap(rabbit(0));
    const seen: number[] = [];
    for (let i = 0; i < 3; i++) m.apply(rabbit(8 * (i + 1)), step(8 * i, 8 * (i + 1)), { onSlide: () => seen.push(i) });
    // The first move started before the others arrived; the next one plays
    // with one more waiting behind it, so faster.
    await vi.advanceTimersByTimeAsync(STEP_MS + 5);
    expect(seen).toEqual([0, 1]);
    expect(m.stepMs).toBeLessThan(STEP_MS);
    await vi.advanceTimersByTimeAsync(3 * STEP_MS);
    expect(seen).toEqual([0, 1, 2]);
    expect(m.pieces[0].square).toBe(24);
    expect(m.animating).toBe(false);
    expect(m.stepMs).toBe(STEP_MS);
  });

  it('jumps to the latest position when more than MAX_BEHIND moves are waiting', async () => {
    const m = new BoardModel();
    m.snap(rabbit(0));
    // The first move starts playing at once; the rest queue up behind it.
    for (let i = 0; i <= MAX_BEHIND + 1; i++) m.apply(rabbit(i + 1), step(i, i + 1));
    await vi.advanceTimersByTimeAsync(STEP_MS + 50);
    expect(m.pieces[0].square).toBe(MAX_BEHIND + 2);
    expect(m.animating).toBe(false);
  });

  it('a real jump cancels queued moves', async () => {
    const m = new BoardModel();
    m.snap(rabbit(0));
    m.apply(rabbit(8), step(0, 8));
    m.apply(rabbit(16), step(8, 16));
    m.apply(rabbit(40), []);
    expect(m.animating).toBe(false);
    expect(m.pieces[0].square).toBe(40);
    await vi.advanceTimersByTimeAsync(3 * STEP_MS);
    expect(m.pieces[0].square).toBe(40);
  });
});
