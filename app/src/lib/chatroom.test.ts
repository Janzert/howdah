import { describe, expect, it } from 'vitest';
import type { ChatEventKind } from './bindings/ChatEventKind';
import type { ChatRoomView } from './bindings/ChatRoomView';
import {
  browserCanOpen,
  chatTime,
  FACE_EMOJI,
  luminance,
  messageColor,
  newRoomMessages,
  parseHex,
  PIECE_SYMBOLS,
} from './chatroom';

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
    const today = chatTime(new Date(2026, 9, 18, 9, 5).getTime() / 1000, {}, now);
    const earlier = chatTime(new Date(2026, 9, 12, 9, 5).getTime() / 1000, {}, now);
    expect(earlier.endsWith(today)).toBe(true);
    expect(earlier.length).toBeGreaterThan(today.length);
  });

  it('shows the seconds', () => {
    const now = new Date(2026, 9, 18, 12, 0);
    expect(chatTime(new Date(2026, 9, 18, 9, 5, 37).getTime() / 1000, {}, now)).toContain('05:37');
  });

  it('uses the clock asked for', () => {
    const now = new Date(2026, 9, 18, 23, 0);
    const at = new Date(2026, 9, 18, 21, 5, 37).getTime() / 1000;
    expect(chatTime(at, { hourCycle: 'h23' }, now)).toContain('21:05:37');
    expect(chatTime(at, { hourCycle: 'h12' }, now)).toContain('09:05:37');
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

describe('browserCanOpen', () => {
  it('leaves out pages behind the gameroom', () => {
    expect(browserCanOpen('http://arimaa.com/arimaa/gameroom/comments.cgi?gid=1')).toBe(false);
    expect(browserCanOpen('http://www.arimaa.com/arimaa/gameroom/playerpage.cgi?id=1')).toBe(false);
    expect(browserCanOpen('http://arimaa.com/arimaa/games/jsShowGame.cgi?gid=1')).toBe(true);
    expect(browserCanOpen('https://example.com/arimaa/gameroom/')).toBe(true);
  });
});

describe('chat room sound', () => {
  const ev = (id: number, user: string, kind: ChatEventKind = 'message') => ({
    id,
    time: 1000 + id,
    user,
    kind,
    color: null,
    emote: false,
    parts: [],
  });
  const room = (events: ReturnType<typeof ev>[], state: ChatRoomView['state'] = 'in'): ChatRoomView => ({
    state,
    user: 'me',
    users: ['me', 'ann'],
    count: 2,
    events,
    note: null,
    autoLeave: true,
  });

  it("counts others' new messages", () => {
    const before = room([ev(1, 'ann')]);
    expect(newRoomMessages(before, room([ev(1, 'ann'), ev(2, 'ann'), ev(3, 'bob')]))).toBe(2);
  });

  it('leaves out the user, comings and goings, and nothing new', () => {
    const before = room([ev(1, 'ann')]);
    expect(newRoomMessages(before, room([ev(1, 'ann'), ev(2, 'me')]))).toBe(0);
    expect(newRoomMessages(before, room([ev(1, 'ann'), ev(2, 'bob', 'in'), ev(3, 'ann', 'out')]))).toBe(0);
    expect(newRoomMessages(before, before)).toBe(0);
  });

  it('is quiet on entering and outside the room', () => {
    const backlog = room([ev(1, 'ann'), ev(2, 'bob')]);
    expect(newRoomMessages(null, backlog)).toBe(0);
    expect(newRoomMessages(room([], 'entering'), backlog)).toBe(0);
    expect(newRoomMessages(room([], 'out'), backlog)).toBe(0);
    expect(newRoomMessages(backlog, room([ev(1, 'ann'), ev(2, 'bob'), ev(3, 'ann')], 'out'))).toBe(0);
  });

  it('counts from an empty log', () => {
    expect(newRoomMessages(room([]), room([ev(0, 'ann')]))).toBe(1);
  });
});
