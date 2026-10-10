// The keyboard map: one table for both the key handler (App.svelte) and the
// help overlay, so the two can't drift apart. Keys follow lichess where a
// key has a counterpart there.

export type ShortcutId =
  | 'back'
  | 'forward'
  | 'start'
  | 'end'
  | 'prevVariation'
  | 'nextVariation'
  | 'prevBranch'
  | 'nextBranch'
  | 'commit'
  | 'planTurn'
  | 'undoStep'
  | 'resetTurn'
  | 'moveNow'
  | 'analysis'
  | 'flip'
  | 'cycleHover'
  | 'mute'
  | 'nextGame'
  | 'editPosition'
  | 'help';

export interface Shortcut {
  id: ShortcutId;
  group: string;
  /** `KeyboardEvent.key` values, in the order the help lists them. */
  keys: string[];
  label: string;
}

export const SHORTCUTS: readonly Shortcut[] = [
  { id: 'back', group: 'Moves', keys: ['ArrowLeft', 'k'], label: 'Previous move' },
  { id: 'forward', group: 'Moves', keys: ['ArrowRight', 'j'], label: 'Next move; at the latest, replay it' },
  { id: 'start', group: 'Moves', keys: ['Home', '0'], label: 'Start of the game' },
  { id: 'end', group: 'Moves', keys: ['End', '$'], label: 'Latest move; in a game, back to the live position' },
  { id: 'prevVariation', group: 'Moves', keys: ['ArrowUp'], label: 'Previous alternative to the shown move' },
  { id: 'nextVariation', group: 'Moves', keys: ['ArrowDown'], label: 'Next alternative to the shown move' },
  { id: 'prevBranch', group: 'Moves', keys: ['Shift+ArrowLeft'], label: 'Previous move with alternatives' },
  { id: 'nextBranch', group: 'Moves', keys: ['Shift+ArrowRight'], label: 'Next move with alternatives' },
  { id: 'commit', group: 'Your turn', keys: ['Enter'], label: 'Play the move (end the turn in a plan), or confirm the setup' },
  { id: 'planTurn', group: 'Your turn', keys: ['Shift+Enter'], label: 'End the turn without playing it (plan ahead)' },
  { id: 'undoStep', group: 'Your turn', keys: ['Backspace'], label: 'Undo a step' },
  { id: 'resetTurn', group: 'Your turn', keys: ['Escape'], label: 'Undo the whole turn' },
  { id: 'analysis', group: 'Engines', keys: ['l'], label: 'Analysis on or off' },
  {
    id: 'moveNow',
    group: 'Engines',
    keys: [' '],
    label: "Make the thinking engine move now; otherwise add analysis's best turn",
  },
  { id: 'flip', group: 'Board', keys: ['f'], label: 'Flip the board' },
  { id: 'cycleHover', group: 'Board', keys: ['s'], label: 'Hover input: off, arrows, step mode' },
  { id: 'editPosition', group: 'Board', keys: ['e'], label: 'Edit the shown position (position editor, in a new window)' },
  { id: 'nextGame', group: 'Other', keys: ['n'], label: 'Next game waiting on your move' },
  { id: 'mute', group: 'Other', keys: ['m'], label: 'Sound on or off' },
  { id: 'help', group: 'Other', keys: ['?'], label: 'This help' },
];

const KEY_NAMES: Record<string, string> = {
  ArrowLeft: '←',
  ArrowRight: '→',
  ArrowUp: '↑',
  ArrowDown: '↓',
  Enter: 'Enter',
  Backspace: 'Backspace',
  Escape: 'Esc',
  ' ': 'Space',
};

/** How the help shows a key. */
export function keyName(key: string): string {
  if (key.startsWith('Shift+')) return `Shift+${keyName(key.slice(6))}`;
  return KEY_NAMES[key] ?? key;
}

/** The shortcut a key press means, if any. Presses with Ctrl, Alt or Meta
 * are left to the browser and the OS. Shift is part of a character (`?`,
 * `$`), and is written out for named keys (`Shift+ArrowLeft`). */
export function shortcutFor(
  e: Pick<KeyboardEvent, 'key' | 'shiftKey' | 'ctrlKey' | 'altKey' | 'metaKey'>,
): Shortcut | null {
  if (e.ctrlKey || e.altKey || e.metaKey) return null;
  if (e.key.length > 1) {
    const key = e.shiftKey ? `Shift+${e.key}` : e.key;
    return SHORTCUTS.find((s) => s.keys.includes(key)) ?? null;
  }
  const key = e.key.toLowerCase();
  return SHORTCUTS.find((s) => s.keys.includes(key) || s.keys.includes(e.key)) ?? null;
}

/** Groups in table order, for the help. */
export function shortcutGroups(): { group: string; shortcuts: Shortcut[] }[] {
  const groups: { group: string; shortcuts: Shortcut[] }[] = [];
  for (const s of SHORTCUTS) {
    const last = groups[groups.length - 1];
    if (last?.group === s.group) last.shortcuts.push(s);
    else groups.push({ group: s.group, shortcuts: [s] });
  }
  return groups;
}
