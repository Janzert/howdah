# Arimaa Desktop: notes for Claude

Cross-platform Arimaa client. It's a Tauri 2 spike, but it's structured the
way the real project will be. Long term it covers gameroom play, AEI bots,
analysis, tournaments, postal games and bot tooling. This repo is a submodule
of a private parent repo (`../`) that also holds the AEI reference repo and
the original arimaa.com asset archives.

## Architecture

- `crates/arimaa-core`: pure Rust, no UI or Tauri deps (`thiserror` only).
  It will also back a headless CLI and PyO3 bindings, so keep the API clean.
  - `types`: Color, PieceKind (Ord = strength), Piece, Square (0 = a1,
    63 = h8), Dir, TRAPS.
  - `position`: bitboard `Position` with a zobrist hash. `apply_step` and
    `undo_step` are raw mechanics: they move a piece and resolve traps, with
    no legality checks.
  - `turn`: `TurnBuilder` is the only place step legality lives: freezing,
    push/pull, rabbit direction, 4 steps, no unfinished push, no net-null turn.
    An ambiguous enemy step is a pull, and push-finish/pull-finish steps can't
    start a pull. Pusher eligibility is fixed when the push starts.
  - `setup`, `outcome` (goal/elimination; `TODO(rules)` markers for
    repetition and immobilization), `notation` (syntax only), and
    `game::Game` (moves plus cached positions per ply; `parse` validates
    capture tokens, `to_record` round-trips).
- `app/src-tauri`: thin shell.
  - `session.rs` has the pure state logic: cursor ply, in-progress turn,
    setup draft, and stable piece ids for animation. Unit-tested, no Tauri types.
  - `dto.rs` holds the view types sent to the UI, with ts-rs derives.
  - `commands.rs`: commands are **intents**. Mutating commands return
    `Result<(), ApiError>`, and the new state arrives as a `game://changed`
    event (`SessionUpdate {view, animation}`). Queries (`get_state`,
    `legal_targets`, `export_game`) return data.
- `app/src`: Svelte 5 (runes) + Vite, no SvelteKit.
  - `lib/api.ts`: typed invoke wrappers.
  - `lib/events.ts`: typed `on()`.
  - `lib/board/`: SVG board. `BoardModel` plays `AnimStep`s: slide, then
    fade out on capture, with fade-in for restored pieces going backward.
  - `lib/theme.ts` plus `themes/<dir>/*.theme.json`: data-driven themes
    (image or procedural board/pieces).

## Conventions

- **The frontend never decides legality.** It renders state and sends intents.
  Drag hints come from `legal_targets`, but `try_step` is authoritative.
- Event names are `domain://event`. Planned: `engine://info`,
  `gameroom://update`, `tournament://progress`. Use Tauri `Channel<T>` for
  per-request high-rate streams (e.g. one analysis run).
- TypeScript types come from Rust via ts-rs into `app/src/lib/bindings/`
  (committed; don't hand-edit). Regenerate after changing DTOs or core serde
  types: `cd app && npm run bindings`. `.cargo/config.toml` sets
  `TS_RS_EXPORT_DIR`.
- Core serde/ts derives are behind the `serde`/`ts` features. Only the app
  enables `ts`.
- Square serializes as its index (0..63). Enums serialize lowercase/camelCase.
- rustfmt: `max_width = 110`. Conventional commits.
- The reference implementation for rules questions is `../AEI/pyrimaa/board.py`.
  `crates/arimaa-core/tests/movegen.rs` checks move-generation counts
  against it. To add cases, compute counts with pyrimaa's `get_moves()`.

## Build / test / run

```bash
cargo test --workspace            # all Rust tests (test profile uses opt-level 1)
cargo clippy --workspace --all-targets
cd app && npm install
npm run check                     # svelte-check
npm run bindings                  # regenerate TS types
npm run tauri dev                 # run the app
```

- Linux system packages: see README.md (WebKitGTK 4.1 is the essential one).
- TypeScript is pinned to 6.x because svelte-check doesn't support TS 7 yet.
- Tauri 3 is in alpha; stay on 2.x.

## Adding a theme

Create `app/src/themes/<dir>/<name>.theme.json` (see `lib/theme.ts` for the
schema) next to its images. For image boards, `grid` is the 8×8 playing area
in image pixels. It's picked up automatically.
