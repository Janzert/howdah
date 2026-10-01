// Sound effects (classic arimaa.com set). Whether sound is on is a setting
// (settings.svelte.ts); the app passes it in with `setMuted`.
//
// Sounds are played through Web Audio from buffers decoded once at startup
// (by our own WAV decoder, so every platform behaves the same). Each play is
// a cheap buffer source, so overlapping sounds (a step and a capture) mix
// instead of cutting each other off. HTML audio elements, which we used
// before, were unreliable in WebKitGTK: very short clips (place.wav is 45 ms)
// often didn't play, and overlapping ones were dropped.
import place from '../sounds/classic/place.wav?inline';
import slide from '../sounds/classic/slide.wav?inline';
import trapped from '../sounds/classic/trapped.wav?inline';
import win from '../sounds/classic/win.wav?inline';
import { dataUrlBytes, decodeWav } from './wav';

export type SoundName = 'slide' | 'place' | 'trapped' | 'win';

const sources: Record<SoundName, string> = { slide, place, trapped, win };

let muted = false;
let context: AudioContext | null = null;
const buffers = new Map<SoundName, AudioBuffer>();

export function isMuted() {
  return muted;
}

export function setMuted(m: boolean) {
  muted = m;
}

/** Creates the audio context and decodes every sound. Safe to call repeatedly. */
function init(): AudioContext | null {
  if (context) return context;
  if (typeof AudioContext === 'undefined') return null;
  context = new AudioContext();
  for (const [name, url] of Object.entries(sources) as [SoundName, string][]) {
    try {
      const wav = decodeWav(dataUrlBytes(url));
      const buffer = context.createBuffer(wav.channels.length, wav.channels[0].length, wav.sampleRate);
      wav.channels.forEach((samples, i) => buffer.copyToChannel(samples, i));
      buffers.set(name, buffer);
    } catch (e) {
      console.warn(`couldn't decode sound ${name}:`, e);
    }
  }
  return context;
}

/**
 * A context may start suspended until the user interacts (browser autoplay
 * rules). Resume it on the first click or key press.
 */
export function unlockOnInteraction() {
  const unlock = () => {
    init()?.resume().catch(() => {});
    window.removeEventListener('pointerdown', unlock, true);
    window.removeEventListener('keydown', unlock, true);
  };
  window.addEventListener('pointerdown', unlock, true);
  window.addEventListener('keydown', unlock, true);
}

export function play(name: SoundName) {
  if (muted) return;
  const ctx = init();
  const buffer = buffers.get(name);
  if (!ctx || !buffer) return;
  if (ctx.state === 'suspended') ctx.resume().catch(() => {});
  const source = ctx.createBufferSource();
  source.buffer = buffer;
  source.connect(ctx.destination);
  source.start();
}

/** Context state, for debugging (e.g. from the devtools console). */
export function audioState(): string {
  return context ? `${context.state}, ${buffers.size} sounds` : 'not started';
}
