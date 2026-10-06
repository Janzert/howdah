import { describe, expect, it } from 'vitest';
import dogStep from '../sounds/classic/dogStep.wav?inline';
import drop2 from '../sounds/classic/Drop2.wav?inline';
import elephantStep from '../sounds/classic/elephantStep.wav?inline';
import metal2 from '../sounds/classic/Metal2_3.wav?inline';
import place from '../sounds/classic/place.wav?inline';
import slide2 from '../sounds/classic/slide2.wav?inline';
import trapped from '../sounds/classic/trapped.wav?inline';
import win from '../sounds/classic/win.wav?inline';
import { dataUrlBytes, decodeWav } from './wav';

// Loaded the same way the app loads them.
const files: Record<string, string> = {
  'place.wav': place,
  'trapped.wav': trapped,
  'win.wav': win,
  'slide2.wav': slide2,
  'dogStep.wav': dogStep,
  'Drop2.wav': drop2,
  'elephantStep.wav': elephantStep,
  'Metal2_3.wav': metal2,
};
const sound = (name: string) => dataUrlBytes(files[name]);

describe('decodeWav', () => {
  it('decodes the classic sounds', () => {
    const expected: Record<string, [number, number]> = {
      'place.wav': [11025, 504],
      // 18-byte fmt chunk plus a fact chunk before the data.
      'trapped.wav': [22050, 24282],
      'win.wav': [11025, 38369],
      // 16-bit, converted from the archive's .au files.
      'slide2.wav': [8000, 1231],
      'dogStep.wav': [8000, 1375],
      'Drop2.wav': [8000, 8448],
      'elephantStep.wav': [8012, 1854],
      'Metal2_3.wav': [11000, 517],
    };
    for (const [name, [rate, frames]] of Object.entries(expected)) {
      const w = decodeWav(sound(name));
      expect(w.sampleRate, name).toBe(rate);
      expect(w.channels.length, name).toBe(1);
      expect(w.channels[0].length, name).toBe(frames);
      const peak = w.channels[0].reduce((m, x) => Math.max(m, Math.abs(x)), 0);
      expect(peak, `${name} isn't silent`).toBeGreaterThan(0.05);
      expect(peak, name).toBeLessThanOrEqual(1);
    }
  });

  it('decodes 16-bit stereo', () => {
    // 2 frames of 16-bit stereo: (0, 32767), (-32768, 0)
    const header = (dataSize: number) => {
      const b = new DataView(new ArrayBuffer(44));
      const s = (at: number, t: string) => [...t].forEach((c, i) => b.setUint8(at + i, c.charCodeAt(0)));
      s(0, 'RIFF'); b.setUint32(4, 36 + dataSize, true); s(8, 'WAVE');
      s(12, 'fmt '); b.setUint32(16, 16, true); b.setUint16(20, 1, true); b.setUint16(22, 2, true);
      b.setUint32(24, 8000, true); b.setUint32(28, 32000, true); b.setUint16(32, 4, true); b.setUint16(34, 16, true);
      s(36, 'data'); b.setUint32(40, dataSize, true);
      return new Uint8Array(b.buffer);
    };
    const samples = new Int16Array([0, 32767, -32768, 0]);
    const bytes = new Uint8Array([...header(8), ...new Uint8Array(samples.buffer)]);
    const w = decodeWav(bytes);
    expect(w.sampleRate).toBe(8000);
    expect(Array.from(w.channels[0])).toEqual([0, -1]);
    expect(w.channels[1][0]).toBeCloseTo(1, 3);
  });

  it('rejects non-PCM and junk', () => {
    expect(() => decodeWav(new Uint8Array(12))).toThrow();
  });

  it('reads data URLs', () => {
    expect(Array.from(dataUrlBytes('data:audio/wav;base64,AAEC'))).toEqual([0, 1, 2]);
  });
});
