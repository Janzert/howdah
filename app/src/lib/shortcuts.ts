// The keyboard map: one table for both the key handler (App.svelte) and the
// help overlay, so the two can't drift apart. Keys follow lichess where a
// key has a counterpart there.

export type ShortcutId =
  | 'back'
  | 'forward'
  | 'start'
  | 'end'
  | 'commit'
  | 'undoStep'
  | 'resetTurn'
  | 'moveNow'
  | 'flip'
  | 'cycleHover'
  | 'mute'
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
  { id: 'end', group: 'Moves', keys: ['End', '$'], label: 'Latest move' },
  { id: 'commit', group: 'Your turn', keys: ['Enter'], label: 'Commit the move or setup' },
  { id: 'undoStep', group: 'Your turn', keys: ['Backspace'], label: 'Undo a step' },
  { id: 'resetTurn', group: 'Your turn', keys: ['Escape'], label: 'Undo the whole turn' },
  { id: 'moveNow', group: 'Your turn', keys: [' '], label: 'Make the thinking engine move now' },
  { id: 'flip', group: 'Board', keys: ['f'], label: 'Flip the board' },
  { id: 'cycleHover', group: 'Board', keys: ['s'], label: 'Hover input: off, arrows, step mode' },
  { id: 'mute', group: 'Other', keys: ['m'], label: 'Sound on or off' },
  { id: 'help', group: 'Other', keys: ['?'], label: 'This help' },
];

const KEY_NAMES: Record<string, string> = {
  ArrowLeft: '←',
  ArrowRight: '→',
  Enter: 'Enter',
  Backspace: 'Backspace',
  Escape: 'Esc',
  ' ': 'Space',
};

/** How the help shows a key. */
export function keyName(key: string): string {
  return KEY_NAMES[key] ?? key;
}

/** The shortcut a key press means, if any. Presses with Ctrl, Alt or Meta
 * are left to the browser and the OS; Shift is part of `?` and `$`. */
export function shortcutFor(e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'altKey' | 'metaKey'>): Shortcut | null {
  if (e.ctrlKey || e.altKey || e.metaKey) return null;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
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
