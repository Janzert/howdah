# Classic arimaa.com sounds

Sound effects from the original arimaa.com game client (2002), released to
the public domain. Source archive: `AllSounds.zip`.

No attribution is required; this note records provenance.

`place.wav`, `trapped.wav` and `win.wav` are the archive's own `.wav`
files. `slide2.wav`, `dogStep.wav`, `Drop2.wav`, `elephantStep.wav` and
`Metal2_3.wav` were converted from the archive's Sun `.au` files (which
aren't playable in all webviews) to 16-bit PCM with
`ffmpeg -i <name>.au -c:a pcm_s16le <name>.wav`. The app plays `win`,
`Drop2`, `elephantStep` and `Metal2_3` (see `lib/sound.ts`); the board
sounds that `place`, `slide2`, `trapped` and `dogStep` were used for now
come from `sounds/board/`, and they stay as test data for the WAV decoder.
