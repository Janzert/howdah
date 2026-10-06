# Classic arimaa.com sounds

Sound effects from the original arimaa.com game client (2002), released to
the public domain. Source archive: `AllSounds.zip`.

No attribution is required; this note records provenance.

`place.wav`, `trapped.wav` and `win.wav` are the archive's own `.wav`
files. `slide2.wav`, `dogStep.wav`, `Drop2.wav`, `elephantStep.wav` and
`Metal2_3.wav` were converted from the archive's Sun `.au` files (which
aren't playable in all webviews) to 16-bit PCM with
`ffmpeg -i <name>.au -c:a pcm_s16le <name>.wav`. Which sound goes with
which event follows 4steps (see `lib/sound.ts`).
