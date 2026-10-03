// Sound effects (classic arimaa.com set). Whether sound is on and its
// volume are settings (settings.svelte.ts); the app passes them in with
// `setMuted` and `setVolume`.
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

export type SoundName = 'slide' | 'place' | 'trapped' | 'win' | 'tick';

const sources: Record<Exclude<SoundName, 'tick'>, string> = { slide, place, trapped, win };

let muted = false;
let gain = 1;
let context: AudioContext | null = null;
const buffers = new Map<SoundName, AudioBuffer>();
// Sources still playing. WebKit can garbage-collect a source node nothing
// refers to before it finishes, cutting the sound off part way.
const playing = new Set<AudioBufferSourceNode>();
// Every sound goes through one long-lived node, and a silent loop plays
// into it the whole time. Without the loop, WebKitGTK's GStreamer output
// goes idle between sounds and clips the next ones while it starts up
// again (tested: the shared node alone doesn't help).
let output: GainNode | null = null;
let keepAlive: AudioBufferSourceNode | null = null;

export function isMuted() {
  return muted;
}

export function setMuted(m: boolean) {
  muted = m;
}

/** Sets the volume from a 0-100 setting, squared so the slider feels even. */
export function setVolume(percent: number) {
  gain = (Math.min(100, Math.max(0, percent)) / 100) ** 2;
  if (output) output.gain.value = gain;
}

/** Creates the audio context and decodes every sound. Safe to call repeatedly. */
function init(): AudioContext | null {
  if (context) return context;
  if (typeof AudioContext === 'undefined') return null;
  context = new AudioContext();
  output = context.createGain();
  output.gain.value = gain;
  output.connect(context.destination);
  keepAlive = context.createBufferSource();
  keepAlive.buffer = context.createBuffer(1, context.sampleRate, context.sampleRate);
  keepAlive.loop = true;
  keepAlive.connect(output);
  keepAlive.start();
  buffers.set('tick', tickBuffer(context));
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

/** The low-time clock tick: a short, quickly decaying click. The classic
 * set has no tick, so it's synthesized. */
function tickBuffer(ctx: AudioContext): AudioBuffer {
  const rate = ctx.sampleRate;
  const buffer = ctx.createBuffer(1, Math.round(rate * 0.04), rate);
  const data = buffer.getChannelData(0);
  for (let i = 0; i < data.length; i++) {
    const t = i / rate;
    const tone = Math.sin(2 * Math.PI * 1900 * t) + 0.5 * Math.sin(2 * Math.PI * 3100 * t);
    data[i] = 0.35 * tone * Math.exp(-t / 0.006);
  }
  return buffer;
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
  if (!ctx || !buffer || !output) return;
  if (ctx.state === 'suspended') ctx.resume().catch(() => {});
  const source = ctx.createBufferSource();
  source.buffer = buffer;
  source.connect(output);
  playing.add(source);
  source.onended = () => {
    playing.delete(source);
    source.disconnect();
  };
  source.start();
}

/** Context state, for debugging (e.g. from the devtools console). */
export function audioState(): string {
  return context ? `${context.state}, ${buffers.size} sounds` : 'not started';
}
