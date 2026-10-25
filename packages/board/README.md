# @howdah/board

Howdah's board and the pieces around it, as Svelte 5 components that take
only props, for the Howdah app (`app/`) and other sites (Maqha's web UI).

- `board/Board.svelte`: the SVG board, with dragging (routes of several
  steps), hover arrows or step mode, annotations and the last move's
  trail. It never decides legality: the page passes `legalTargets`,
  `planRoute`, `onDrop` and `onStep`. `board/boardModel.svelte.ts`
  (`BoardModel`) holds the pieces shown and plays animations
  (`AnimStep`s); `board/registry.ts` lets an app's scripted hooks find
  the mounted board.
- `board/MiniBoard.svelte`, `board/PieceGlyph.svelte`: a small board and
  one piece.
- `PlayerBar.svelte`: a player's name, rating, captures and clock.
- `clock.ts`: clock arithmetic and formatting; `geometry.ts`: squares
  and board units; `theme.ts` with `themes/`: board and piece themes;
  `sound.ts` with `wav.ts` and `sounds/`: the sounds, played with Web
  Audio.
- `types.ts`: the data the components take. They match the app's
  bindings generated from Rust, which `app/src/lib/boardTypes.check.ts`
  keeps in step.

It's used from source: the consumer's Vite config aliases
`@howdah/board` to `src/lib`, dedupes `svelte`, and allows the
package's directory (see `app/vite.config.ts`). Its tests run with the
app's (`npm test` in `app/`).

Licensed MIT, like the rest of Howdah. The images and sounds have their
own notes: `src/themes/classic/ATTRIBUTION.md`,
`src/sounds/*/ATTRIBUTION.md`.
