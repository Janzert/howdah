// What a session's view gives the board package's PlayerBar for one side.
import type { Color } from './bindings/Color';
import type { SessionView } from './bindings/SessionView';

export function barProps(view: SessionView, side: Color) {
  const i = side === 'gold' ? 0 : 1;
  const player = view.players?.[side];
  return {
    side,
    name: player?.name ?? view.tagNames[i] ?? (side === 'gold' ? 'Gold' : 'Silver'),
    rating: view.tagRatings[i],
    engine: player?.kind === 'engine',
    // The live clock, or the clocks after the shown move once the game is over.
    clock: view.shownClock,
    toMove: view.result == null && view.position.sideToMove === side,
    thinking: view.thinking === side,
    captured: view.captured[side === 'gold' ? 'silver' : 'gold'],
  };
}
