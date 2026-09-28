import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AnimStep } from '../bindings/AnimStep';
import type { PieceView } from '../bindings/PieceView';
import { BoardModel, INSTANT_GAP_MS, MAX_BEHIND, STEP_MS } from './boardModel.svelte';

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

  it('shows each move instantly, a short gap apart, when more than MAX_BEHIND are waiting', async () => {
    const m = new BoardModel();
    m.snap(rabbit(0));
    // Move 1 starts animating at once; moves 2..9 queue up behind it (8 > 6).
    const moves = MAX_BEHIND + 3;
    for (let i = 0; i < moves; i++) m.apply(rabbit(i + 1), step(i, i + 1));
    await vi.advanceTimersByTimeAsync(STEP_MS + 5);
    expect(m.pieces[0].square).toBe(2); // move 2 shown instantly
    await vi.advanceTimersByTimeAsync(INSTANT_GAP_MS);
    expect(m.pieces[0].square).toBe(3); // then move 3, one gap later
    await vi.advanceTimersByTimeAsync(20 * STEP_MS);
    expect(m.pieces[0].square).toBe(moves);
    expect(m.animating).toBe(false);
  });

  it("fits a move's animation into its time budget", async () => {
    const m = new BoardModel();
    m.snap(rabbit(0));
    // Two steps at 220 ms would take 440 ms; the move only took 100 ms.
    const anim = [...step(0, 8), ...step(8, 16)];
    m.apply(rabbit(16), anim, {}, 100);
    expect(m.stepMs).toBeLessThanOrEqual(50);
    await vi.advanceTimersByTimeAsync(110);
    expect(m.animating).toBe(false);
    // A generous budget doesn't slow anything down.
    m.apply(rabbit(24), step(16, 24), {}, 10_000);
    expect(m.stepMs).toBe(STEP_MS);
  });

  it('base speed is settable', async () => {
    const m = new BoardModel();
    m.setBaseSpeed(100);
    expect(m.stepMs).toBe(100);
    m.snap(rabbit(0));
    m.apply(rabbit(8), step(0, 8));
    expect(m.stepMs).toBe(100);
    await vi.advanceTimersByTimeAsync(110);
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
