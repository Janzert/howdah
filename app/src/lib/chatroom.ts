// The gameroom chat room's look: message colors, spoilers and times.
import type { Color } from './bindings/Color';
import type { PieceKind } from './bindings/PieceKind';

/** The room's palette (`chat.js`): black and 18 colors (one twice). */
export const CHAT_PALETTE = [
  '000000',
  '800000',
  'c00000',
  'ff8000',
  '808000',
  '80c000',
  '00c000',
  '008000',
  '00c080',
  '00c0c0',
  '0080ff',
  '0040c0',
  '0000c0',
  '000080',
  '400080',
  '8000ff',
  '800080',
  '800040',
];

/** A color typed as hex (`#` optional; `rgb` is `rrggbb`) as `rrggbb`, or
 * null if it isn't one. */
export function parseHex(s: string): string | null {
  let hex = s.trim().replace(/^#/, '').toLowerCase();
  if (/^[0-9a-f]{3}$/.test(hex)) hex = [...hex].map((c) => c + c).join('');
  return /^[0-9a-f]{6}$/.test(hex) ? hex : null;
}

/** The color that marks a spoiler: the room's log is white, so white
 * text shows only when selected, and people use it as a spoiler tag. */
export const SPOILER_COLOR = 'ffffff';

/** The panel's background in each appearance (`--panel` in `app.css`). */
const LIGHT_PANEL = 'fbfaf6';
const DARK_PANEL = '262522';

/** The least contrast a message's color keeps with the panel (the room's
 * white log shows pale colors barely, or not at all). */
const MIN_CONTRAST = { light: 2.5, dark: 3.5 };

function channels(hex: string): [number, number, number] {
  return [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16)) as [number, number, number];
}

/** WCAG relative luminance of an `rrggbb` color. */
export function luminance(hex: string): number {
  const [r, g, b] = channels(hex).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

function mix(hex: string, toward: number, t: number): string {
  return channels(hex)
    .map((c) =>
      Math.round(c + (toward - c) * t)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('');
}

/** How to show a message in `hex` (`rrggbb`, or null for the default):
 * `color` as a CSS color (null for the page's text color) and whether it's
 * a spoiler. White is the room's spoiler tag (see `SPOILER_COLOR`); here
 * those lines are hidden until revealed. Other colors are kept, darkened
 * or lightened only as far as needed to be readable on the panel. */
export function messageColor(hex: string | null, dark: boolean): { color: string | null; spoiler: boolean } {
  if (!hex || !/^[0-9a-f]{6}$/i.test(hex)) return { color: null, spoiler: false };
  hex = hex.toLowerCase();
  if (hex === SPOILER_COLOR) return { color: null, spoiler: true };
  if (hex === '000000') return { color: null, spoiler: false };
  const [panel, toward, min] = dark ? [DARK_PANEL, 255, MIN_CONTRAST.dark] : [LIGHT_PANEL, 0, MIN_CONTRAST.light];
  let shown = hex;
  for (let t = 0.1; contrast(shown, panel) < min && t <= 1; t += 0.1) shown = mix(hex, toward, t);
  return { color: `#${shown}`, spoiler: false };
}

/** A message's time: the clock time today, else the date too. */
export function chatTime(unix: number, now = new Date()): string {
  const d = new Date(unix * 1000);
  const time = d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  if (d.toDateString() === now.toDateString()) return time;
  return `${d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })} ${time}`;
}

/** The room's faces: the symbol typed, the room's image name and the
 * emoji shown for it (its own images aren't ours to ship). In the order
 * of `FACES` in howdah-gameroom's `chat.rs`, which does the replacing. */
export const FACES: { symbol: string; name: string; emoji: string }[] = [
  { symbol: ':)', name: 'smiley', emoji: '🙂' },
  { symbol: '=)', name: 'happy', emoji: '😊' },
  { symbol: '^^', name: 'nice', emoji: '😄' },
  { symbol: 'XD', name: 'laugh', emoji: '😆' },
  { symbol: ';)', name: 'wink', emoji: '😉' },
  { symbol: ':P', name: 'tongue', emoji: '😛' },
  { symbol: ':D', name: 'biggrin', emoji: '😁' },
  { symbol: ':|', name: 'blank', emoji: '😐' },
  { symbol: ':shocked:', name: 'shocked', emoji: '😲' },
  { symbol: ':(', name: 'sad', emoji: '🙁' },
  { symbol: ':frown:', name: 'frown', emoji: '☹️' },
  { symbol: ':angry:', name: 'angry', emoji: '😠' },
  { symbol: ':cry:', name: 'cry', emoji: '😢' },
];

/** Each face's emoji by its image name. */
export const FACE_EMOJI: Record<string, string> = Object.fromEntries(FACES.map((f) => [f.name, f.emoji]));

const KINDS: PieceKind[] = ['elephant', 'camel', 'horse', 'dog', 'cat', 'rabbit'];

/** The room's piece symbols (`:ge`, gold elephant; camels are `m`),
 * gold's then silver's, strongest first. */
export const PIECE_SYMBOLS: { symbol: string; color: Color; piece: PieceKind }[] = (
  ['gold', 'silver'] as const
).flatMap((color) =>
  KINDS.map((piece) => ({ symbol: `:${color[0]}${piece === 'camel' ? 'm' : piece[0]}`, color, piece })),
);
