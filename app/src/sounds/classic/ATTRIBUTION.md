# Classic arimaa.com sounds

Sound effects from the original arimaa.com game client (2002), released to
the public domain. Source archive: `AllSounds.zip`.

No attribution is required; this note records provenance.

`place.wav`, `trapped.wav` and `win.wav` are the archive's own `.wav`
files. `slide2.wav`, `dogStep.wav`, `Drop2.wav`, `elephantStep.wav` and
`Metal2_3.wav` were converted from the archive's Sun `.au` files (which
aren't playable in all webviews) to 16-bit PCM with
`ffmpeg -i <name>.au -c:a pcm_s16le <name>.wav`. The app no longer plays
any of them (its sounds are in `sounds/board/` and `sounds/app/`); they
stay as test data for the WAV decoder (`lib/wav.test.ts`), which they
cover well: 8-bit, odd sample rates and an extended format chunk.
