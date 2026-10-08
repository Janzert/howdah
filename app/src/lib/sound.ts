// Sound effects, one per game event. The board sounds (a step, a move's
// last step, captures and a capture undone) are made in the sound lab from
// CC0 samples (`sounds/board/ATTRIBUTION.md`), and a theme can replace any
// of them (`setThemeSounds`); each play varies their pitch a little so
// repeated steps don't sound mechanical. The rest are app-wide and still
// from the classic arimaa.com set until the sound rework reaches them.
// Whether sound is on and its volume are settings (settings.svelte.ts); the
// app passes them in with `setMuted` and `setVolume`.
//
// Sounds are played through Web Audio from buffers decoded once each
// (by our own WAV decoder, so every platform behaves the same). Each play is
// a cheap buffer source, so overlapping sounds (a step and a capture) mix
// instead of cutting each other off. HTML audio elements, which we used
// before, were unreliable in WebKitGTK: very short clips (place.wav is 45 ms)
// often didn't play, and overlapping ones were dropped.
import capture from '../sounds/board/capture.wav?inline';
import lastStep from '../sounds/board/lastStep.wav?inline';
import ownLoss from '../sounds/board/ownLoss.wav?inline';
import restore from '../sounds/board/restore.wav?inline';
import step from '../sounds/board/step.wav?inline';
import drop2 from '../sounds/classic/Drop2.wav?inline';
import elephantStep from '../sounds/classic/elephantStep.wav?inline';
import metal2 from '../sounds/classic/Metal2_3.wav?inline';
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

/** The sounds a theme can replace. */
export const BOARD_SOUNDS = ['step', 'lastStep', 'capture', 'ownLoss', 'restore'] as const;
export type BoardSoundName = (typeof BOARD_SOUNDS)[number];
const isBoardSound = (n: SoundName): n is BoardSoundName => (BOARD_SOUNDS as readonly string[]).includes(n);

/** How far a board sound's pitch varies from play to play, either way. */
const BOARD_VARIATION = 0.08;

const boardDefaults: Record<BoardSoundName, string> = { step, lastStep, capture, ownLoss, restore };

// TODO(sounds): the sound rework gives every event its own sound; until
// then the new events are silent or borrow another one.
const appSounds: Partial<Record<SoundName, string>> = {
  setupDone: lastStep,
  yourTurn: lastStep,
  gameStart: win,
  win: drop2,
  loss: elephantStep,
  timeout: elephantStep,
  tick: metal2,
};

/** Each event's WAV, as a data URL: the app's, with the theme's board sounds. */
let sources: Partial<Record<SoundName, string>> = { ...appSounds, ...boardDefaults };

let muted = false;
let gain = 1;
let context: AudioContext | null = null;
/** Decoded sounds, by data URL. */
const buffers = new Map<string, AudioBuffer | null>();
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
  decodeAll();
  return context;
}

/** Decodes every current sound not decoded yet. */
function decodeAll() {
  for (const [name, url] of Object.entries(sources) as [SoundName, string][]) bufferFor(name, url);
}

function bufferFor(name: SoundName, url: string): AudioBuffer | null {
  if (!context) return null;
  if (buffers.has(url)) return buffers.get(url)!;
  let buffer: AudioBuffer | null = null;
  try {
    const wav = decodeWav(dataUrlBytes(url));
    buffer = context.createBuffer(wav.channels.length, wav.channels[0].length, wav.sampleRate);
    wav.channels.forEach((samples, i) => buffer!.copyToChannel(samples, i));
  } catch (e) {
    console.warn(`couldn't decode sound ${name}:`, e);
  }
  buffers.set(url, buffer);
  return buffer;
}

/** Uses a theme's board sounds (data URLs) in place of the defaults; any
 * it leaves out keep the default. */
export function setThemeSounds(theme: Partial<Record<BoardSoundName, string>>) {
  sources = { ...appSounds, ...boardDefaults, ...theme };
  decodeAll();
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
  const url = sources[name];
  const buffer = url ? bufferFor(name, url) : null;
  if (!ctx || !buffer || !output) return;
  if (ctx.state === 'suspended') ctx.resume().catch(() => {});
  const source = ctx.createBufferSource();
  source.buffer = buffer;
  if (isBoardSound(name)) source.playbackRate.value = 1 + (Math.random() * 2 - 1) * BOARD_VARIATION;
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
