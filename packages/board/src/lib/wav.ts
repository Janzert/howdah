// Minimal WAV (RIFF PCM) decoder. The classic sounds are 8-bit mono PCM at
// 11–22 kHz, which platform decoders handle inconsistently, so we decode them
// ourselves and hand the samples to Web Audio.

export interface DecodedWav {
  sampleRate: number;
  /** One Float32Array per channel, samples in [-1, 1]. */
  channels: Float32Array<ArrayBuffer>[];
}

export function decodeWav(bytes: Uint8Array): DecodedWav {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const tag = (at: number) => String.fromCharCode(...bytes.subarray(at, at + 4));
  if (tag(0) !== 'RIFF' || tag(8) !== 'WAVE') throw new Error('not a WAV file');

  let format: { channels: number; sampleRate: number; bits: number } | null = null;
  let data: Uint8Array | null = null;
  let at = 12;
  while (at + 8 <= bytes.length) {
    const id = tag(at);
    const size = view.getUint32(at + 4, true);
    const body = at + 8;
    if (id === 'fmt ') {
      const audioFormat = view.getUint16(body, true);
      if (audioFormat !== 1) throw new Error(`unsupported WAV format ${audioFormat} (only PCM)`);
      format = {
        channels: view.getUint16(body + 2, true),
        sampleRate: view.getUint32(body + 4, true),
        bits: view.getUint16(body + 14, true),
      };
    } else if (id === 'data') {
      // Some writers get the size wrong; never read past the end.
      data = bytes.subarray(body, Math.min(body + size, bytes.length));
    }
    at = body + size + (size & 1);
  }
  if (!format || !data) throw new Error('WAV file is missing fmt or data');
  const { channels, sampleRate, bits } = format;
  if (bits !== 8 && bits !== 16) throw new Error(`unsupported WAV sample size ${bits}`);

  const bytesPerSample = bits / 8;
  const frames = Math.floor(data.length / (bytesPerSample * channels));
  const out = Array.from({ length: channels }, () => new Float32Array(frames));
  const dataView = new DataView(data.buffer, data.byteOffset, data.byteLength);
  for (let f = 0; f < frames; f++) {
    for (let c = 0; c < channels; c++) {
      const i = (f * channels + c) * bytesPerSample;
      // 8-bit PCM is unsigned (128 = silence); 16-bit is signed little-endian.
      out[c][f] = bits === 8 ? (data[i] - 128) / 128 : dataView.getInt16(i, true) / 32768;
    }
  }
  return { sampleRate, channels: out };
}

/** Bytes of a `data:...;base64,` URL (what Vite's `?inline` imports give). */
export function dataUrlBytes(url: string): Uint8Array {
  const b64 = url.slice(url.indexOf(',') + 1);
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}
