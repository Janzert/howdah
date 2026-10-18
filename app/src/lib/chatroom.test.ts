import { describe, expect, it } from 'vitest';
import { chatTime, FACE_EMOJI, luminance, messageColor, parseHex, PIECE_SYMBOLS } from './chatroom';

describe('chat room colors', () => {
  it('hides white, the spoiler tag, and only white', () => {
    expect(messageColor('ffffff', false)).toEqual({ color: null, spoiler: true });
    expect(messageColor('FFFFFF', true)).toEqual({ color: null, spoiler: true });
    // Near white is a color, darkened to be readable on the light panel.
    const pale = messageColor('fefefe', false);
    expect(pale.spoiler).toBe(false);
    expect(luminance(pale.color!.slice(1))).toBeLessThan(luminance('fefefe'));
    expect(messageColor('fefefe', true)).toEqual({ color: '#fefefe', spoiler: false });
  });

  it('shows black and missing colors in the text color', () => {
    expect(messageColor('000000', true)).toEqual({ color: null, spoiler: false });
    expect(messageColor(null, false)).toEqual({ color: null, spoiler: false });
    expect(messageColor('nope', false)).toEqual({ color: null, spoiler: false });
  });

  it('keeps colors in the light appearance and lightens dark ones in the dark', () => {
    expect(messageColor('000080', false).color).toBe('#000080');
    expect(messageColor('c00000', false).color).toBe('#c00000');
    const navy = messageColor('000080', true).color!;
    expect(luminance(navy.slice(1))).toBeGreaterThan(luminance('000080'));
    expect(messageColor('ff8000', true).color).toBe('#ff8000');
  });
});

describe('chat room times', () => {
  it('leaves out the date for today', () => {
    const now = new Date(2026, 9, 18, 12, 0);
    const today = chatTime(new Date(2026, 9, 18, 9, 5).getTime() / 1000, now);
    const earlier = chatTime(new Date(2026, 9, 12, 9, 5).getTime() / 1000, now);
    expect(earlier.endsWith(today)).toBe(true);
    expect(earlier.length).toBeGreaterThan(today.length);
  });
});

describe('chat room symbols', () => {
  it('names the piece symbols as the room types them', () => {
    expect(PIECE_SYMBOLS.map((p) => p.symbol).join(' ')).toBe(':ge :gm :gh :gd :gc :gr :se :sm :sh :sd :sc :sr');
    expect(PIECE_SYMBOLS[7]).toEqual({ symbol: ':sm', color: 'silver', piece: 'camel' });
  });

  it('keeps an emoji for each face', () => {
    expect(FACE_EMOJI.biggrin).toBe('😁');
    expect(Object.keys(FACE_EMOJI)).toHaveLength(13);
  });
});

describe('typed colors', () => {
  it('reads hex with or without #, short or long', () => {
    expect(parseHex('#F70077')).toBe('f70077');
    expect(parseHex(' 0a8 ')).toBe('00aa88');
    expect(parseHex('12345')).toBeNull();
    expect(parseHex('red')).toBeNull();
  });
});
