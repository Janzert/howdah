// Sound effects (classic arimaa.com set). Mute state is a per-viewer preference.
import slide from '../sounds/classic/slide.wav?url';
import place from '../sounds/classic/place.wav?url';
import trapped from '../sounds/classic/trapped.wav?url';
import win from '../sounds/classic/win.wav?url';

export type SoundName = 'slide' | 'place' | 'trapped' | 'win';

const urls: Record<SoundName, string> = { slide, place, trapped, win };
const cache = new Map<SoundName, HTMLAudioElement>();

let muted = readPref('muted') === '1';

function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function isMuted() {
  return muted;
}

export function setMuted(m: boolean) {
  muted = m;
  try {
    localStorage.setItem('muted', m ? '1' : '0');
  } catch {
    /* preference just won't persist */
  }
}

export function play(name: SoundName) {
  if (muted) return;
  let a = cache.get(name);
  if (!a) {
    a = new Audio(urls[name]);
    cache.set(name, a);
  }
  // Clone so overlapping plays don't cut each other off.
  const node = a.cloneNode() as HTMLAudioElement;
  node.play().catch(() => {
    /* autoplay policies or missing codecs: sounds are optional */
  });
}
