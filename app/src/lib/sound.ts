// Sound effects, one per game event, from the classic arimaa.com set and
// assigned to events as 4steps does: a soft click per step and a louder one
// for a move's last, separate sounds for capturing and for losing one's own
// piece on a trap, and for winning and losing. Whether sound is on and its
// volume are settings (settings.svelte.ts); the app passes them in with
// `setMuted` and `setVolume`.
//
// Sounds are played through Web Audio from buffers decoded once at startup
// (by our own WAV decoder, so every platform behaves the same). Each play is
// a cheap buffer source, so overlapping sounds (a step and a capture) mix
// instead of cutting each other off. HTML audio elements, which we used
// before, were unreliable in WebKitGTK: very short clips (place.wav is 45 ms)
// often didn't play, and overlapping ones were dropped.
import dogStep from '../sounds/classic/dogStep.wav?inline';
import drop2 from '../sounds/classic/Drop2.wav?inline';
import elephantStep from '../sounds/classic/elephantStep.wav?inline';
import metal2 from '../sounds/classic/Metal2_3.wav?inline';
import place from '../sounds/classic/place.wav?inline';
import slide2 from '../sounds/classic/slide2.wav?inline';
import trapped from '../sounds/classic/trapped.wav?inline';
import win from '../sounds/classic/win.wav?inline';
import { dataUrlBytes, decodeWav } from './wav';

/** The events that make a sound. */
export type SoundName =
  | 'step' // a step within a move
  | 'lastStep' // a move's last step, a committed move, a missed move watched
  | 'capture' // an opponent's piece trapped
  | 'ownLoss' // the mover's own piece trapped
  | 'restore' // a trapped piece back, as a step is undone
  | 'setupDone' // a setup committed
  | 'illegal' // a drop the rules refuse, or a move the server refused
  | 'yourTurn' // the opponent moved and the human player is to move
  | 'gameStart'
  | 'win' // a game ending, unless the lone human player lost
  | 'loss'
  | 'timeout' // a game ending on time, in place of `win` or `loss`
  | 'tick' // the low-time clock
  | 'join' // the opponent sits down at the table, or comes back
  | 'leave' // the opponent leaves the table
  | 'chat' // an opponent's chat line
  | 'notification'; // a takeback request, an invitation, a postal move due

// TODO(themes): let a theme override the sound for each event.
// TODO(sounds): the sound rework gives every event its own sound; until
// then the new events are silent or borrow an old one.
const sources: Partial<Record<SoundName, string>> = {
  step: slide2,
  lastStep: place,
  capture: trapped,
  ownLoss: dogStep,
  restore: place,
  setupDone: place,
  yourTurn: place,
  gameStart: win,
  win: drop2,
  loss: elephantStep,
  timeout: elephantStep,
  tick: metal2,
};

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
  // Shows which event played, also for those without a sound yet.
  if (import.meta.env.DEV) console.debug(`sound: ${name}`);
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
