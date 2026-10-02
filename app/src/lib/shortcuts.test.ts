import { describe, expect, it } from 'vitest';
import { SHORTCUTS, shortcutFor, shortcutGroups } from './shortcuts';

const press = (key: string, mods: { ctrlKey?: boolean; altKey?: boolean; metaKey?: boolean } = {}) =>
  shortcutFor({ key, ctrlKey: false, altKey: false, metaKey: false, ...mods })?.id ?? null;

describe('shortcutFor', () => {
  it('maps each key to its shortcut', () => {
    expect(press('ArrowLeft')).toBe('back');
    expect(press('k')).toBe('back');
    expect(press('$')).toBe('end');
    expect(press('?')).toBe('help');
    expect(press(' ')).toBe('moveNow');
    expect(press('x')).toBeNull();
  });

  it('ignores case, so Caps Lock and Shift still work', () => {
    expect(press('F')).toBe('flip');
  });

  it('leaves Ctrl, Alt and Meta combinations alone', () => {
    expect(press('f', { ctrlKey: true })).toBeNull();
    expect(press('ArrowLeft', { altKey: true })).toBeNull();
    expect(press('k', { metaKey: true })).toBeNull();
  });

  it('gives each key one meaning', () => {
    const keys = SHORTCUTS.flatMap((s) => s.keys);
    expect(new Set(keys).size).toBe(keys.length);
  });
});

describe('shortcutGroups', () => {
  it('keeps every shortcut, grouped in table order', () => {
    const groups = shortcutGroups();
    expect(groups.map((g) => g.group)).toEqual(['Moves', 'Your turn', 'Board', 'Other']);
    expect(groups.flatMap((g) => g.shortcuts)).toEqual(SHORTCUTS);
  });
});
