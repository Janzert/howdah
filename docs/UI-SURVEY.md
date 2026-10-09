# UI survey: ideas from chess and Arimaa clients

A survey of established clients for UI and features worth adopting. It's
input for planning, not a commitment.
Written 2026-10-01, when local play (HvB, BvB) first worked; the labels
were brought up to date on 2026-10-09, after the 0.1.0 release.

Sources:
- **lichess** (`lichess-org/lila`, `ui/` modules: `round`, `analyse`,
  `puzzle`, `editor`, `keyboardMove`, `chart`, `study`, `tournament`, and
  others). This is the main reference for play and analysis UX.
- **4steps** ([TFiFiE/4steps](https://github.com/TFiFiE/4steps), read from
  source). It's the reference for
  Arimaa-specific input and for live-game handling.
- **arimaa.com gameroom client** (`/arimaa/jsClient/pro/arimaa.js`,
  v0.7.25b), read in a logged-in browser alongside its help page
  (`jscHelp.html`). Section 10 summarizes it.
- Desktop chess GUIs that match our roadmap:
  - **En Croissant**: Tauri + React, GPL-3.0. The same stack as ours, and
    an analysis-and-database toolkit.
  - **Nibbler**: real-time engine analysis display.
  - **Cute Chess**: engine matches and tournaments, with GUI and CLI.

Legend: **[have]** already in the client, **[P1]** do soon (fits the
current milestone), **[P2]** fits a planned milestone, **[P3]** later or
speculative.

## 1. Move input (the most Arimaa-specific part)

Entering a 4-step turn is the main source of friction, and chess UIs don't
cover it.

- **[have]** Drag with push/pull, legal-target hints, undo step, reset turn,
  and commit.
- **[have] Drag-to-route.** Drag a piece to any square it can reach with
  the steps left, and the steps are filled in.
  - The route is always a shortest one. When there are several, it takes
    the one that enters the most squares of the dragged path, then the one
    that loses the fewest friendly pieces on traps.
  - The path is recorded as the pointer crosses squares. Re-entering a
    square cuts the path back to it, and fast moves or cut corners are
    subdivided to the squares actually crossed.
  - While dragging, the route the drop would take is drawn on the board,
    with its step count beside the piece. Squares more than one step away
    are shown as smaller target dots.
  - Routes move only the dragged piece. A drop on an adjacent square is a
    single step of any kind, so pushes and pulls work as before.
  - After the drop, the route's steps are replayed quickly so the player
    sees the steps actually taken. Undo takes them back one at a time.
  - For comparison, 4steps (`GameState::preferredRoute`) also takes a
    shortest route, but breaks ties toward its previous route rather than
    toward the drag path.
- **[P2] Click-to-route.** Click a piece, then a destination. It's the same
  `try_route` call with an empty path.
- **[have] Hover arrows (arimaa.com's main input)**, as a setting (off by
  default). Hovering over any
  piece, yours or the opponent's, draws arrows on the squares it can step
  to legally right now. For an enemy piece, that means only the steps that
  start or finish a legal push or pull. Clicking an arrow takes the step.
  It explains the rules as you play. It fits alongside drag: show the same
  arrows on hover and accept a click on an arrow.
- **[P2] arimaa.com click shortcuts** (Expert mode only there; a setting for
  us):
  - Click one of your pieces: undo a step.
  - Click an empty square: redo.
  - Click a trap: undo the whole move.
  - Click an opponent's piece: pass the remaining steps.

  They're fast once learned, but too surprising to be on by default.
- **[have] Step mode (4steps, Ctrl+S)**, as the third choice of the Hover
  setting (off, arrows, step mode). As the cursor moves, the piece under
  it is highlighted, along with an arrow toward the closest adjacent square.
  A single click makes that step. It's very fast for experienced players.
- **[P2] Mouse wheel scrubs steps (4steps).** Wheel down undoes a step;
  wheel up redoes it. Redo needs us to keep the undone steps until a
  different step replaces them. In exploration mode, scrolling past the
  start of the turn continues back into the previous move.
- **[have] Move confirmation and planning.** A move is always played by an
  explicit Play (Enter), never by its fourth step. Steps past a full
  turn continue into the other side's turn, as a plan when it's your move
  in a game (like 4steps' explore mode), with a setting to turn that off
  (`VARIATIONS.md`). 4steps and lichess offer confirmation as an option
  because their moves are sent as they're completed.
- **[P2] Typed move entry (lichess `keyboardMove`).** A text box that
  accepts notation (`Ed2n Ed3n ee7s`) and previews the steps as you type.
  It's also good for accessibility, and "play from clipboard" (4steps:
  middle-click pastes a move or sequence) can use the same path.
- **[P2] Setup UX** (already a planned to-do). Add presets, "copy last
  setup", mirroring, and click-to-swap. arimaa.com starts with a default
  setup already placed (`rrrrrrrrdhcemchd`), and you click two pieces to
  swap them. 4steps also has a placement mode, where you click squares in
  strength order to place pieces.
- **[P3] Auto-complete from the tree (4steps `autoFinalize` in explore
  mode).** When the steps you've entered so far match the start of a known
  variation, propose the rest of it as ghost steps.

## 2. Board display

- **[have]** Themes, animation, frozen marker, annotation arrows, and
  sounds.
- **[have] Last-move highlight.** Done as a trail per moved piece (dashed
  for pushed/pulled pieces) with a ghost of captured pieces. lichess tints the from and to squares. For
  Arimaa, show each step of the last turn as a faint arrow (pushes and
  pulls drawn distinctly), and show a ghost of any piece captured on a
  trap. "Show last move" (4steps) replays its animation.
- **[have] Captured pieces tray**, in the player bars: each bar shows the
  opponent's pieces off the board, rabbits as one icon with ×n. arimaa.com shows the captured pieces below
  the board, and 4steps has "pieces off board" docks. Add a material
  summary. A FAME/HarLog-style material score is a possible stretch.
- **[have] Coordinates: none, traps only, or all (4steps)**, in Settings,
  with traps as the default. arimaa.com
  labels the four traps (C3/F3/C6/F6) on the board.
- **[have] Auto-rotate (4steps):** in a game, the human's side goes at
  the bottom (a setting, on by default). Flip stays available (lichess
  key `f`).
- **[P2] Three views (arimaa.com "Vw"):** gold at the bottom, silver at the
  bottom, and a sideways view with gold on the left and silver on the
  right.
- **[P2] Exploration-mode tint (4steps).** In a live game, the board uses an
  alternative square palette while you're browsing variations. You can't
  confuse analysis with the real game. (The "Back to live game" button,
  End, and the planning note in the turn bar are done; the tint isn't.)
- **[P2] Square color customization (4steps):** goal rows and traps per
  side, plus light and dark.
- **[P3] Teaching overlays.** Mark pieces that can be captured next turn,
  rabbits that can reach goal within N steps, and trap control. They're
  Arimaa-only and cheap to compute in `howdah-arimaa`. They should be off by
  default and probably disallowed in rated online play.
- **[P3] Zen mode (lichess `z`)** hides everything except the board and
  clocks. **Full screen** comes from 4steps.

## 3. Clocks and game flow

- **[have]** Move time and reserve, per-side time controls, and presets.
- **[have] Low-time cues.** As arimaa.com does, the clocks are colored
  by the time left for the turn (move time plus reserve): green, then
  yellow under 30 s, then red under 10 s (the idle one by its next turn's
  time, and only once it's under 30 s). A tick sound plays in the last
  seconds (4steps' schedule), a game-start sound plays, and the window
  asks for attention when it's your move while it's unfocused (4steps
  `QApplication::alert`).
- **[P2] More clock detail (arimaa.com):** the total game time, and a
  click on a clock explaining the time control. (The turn bar already
  shows the step count.)
- **[have] Game-end dialog** with the reason in words, after the final
  move's animation. Actions: rematch, swap sides, analyse; after an
  arimaa.com game only Analyse and Review, since players stay at the
  table.
- **[have] Move times in the move list**: each move's time or the time
  into the game, toggled for the list.
- **[have] Clocks at each move (arimaa.com's viewer):** in a finished
  game, the player bars show the clocks after the shown move, the mover's
  with the time the move took.
- **[P2] Move-time summaries:** arimaa.com shows average move times per
  player, and lichess has a move-time chart (`ui/chart`) under analysis.
- **[have] "Show" replays the last move**: Forward (→) at the latest move
  replays it, instead of a separate button. arimaa.com has four
  speeds (S1–S4), and it's the documented way to see the opponent's move.
- **[P2] Real-time replay (arimaa.com "auto" and "real-time").** With
  "auto", Show chains from move to move, each one waiting that ply's
  recorded `timeused`. Autoplay through the game either at a fixed speed or
  at the pace the moves were actually played. lichess has `autoplay.ts` with similar modes. It fits our
  animation queue, which already paces by `animationBudgetMs`.
- **[have] Takeback request and accept** for arimaa.com games (the
  server ignores them in rated games, so the button is disabled there and
  says why), and Resign. Local games have takebacks as an option when
  starting (`VARIATIONS.md`). The arimaa.com client also has buttons
  (some hidden) for adjourn, draw request and pass; we have none of them.
- **[have] Repetition message that names the earlier plies.** As
  arimaa.com says "Position repeats 3 times. Same as: 12g 14g", the turn
  bar says "the move repeats a position for the third time (as after 12g
  and 14g)".

## 4. Move list and variations

- **[have]** The move list shows the whole tree, with navigation and
  follow-live scrolling.
- **[have] Keyboard map (lichess)**, from one table in
  `lib/shortcuts.ts`, with the `?` help overlay. Only `x` waits, on
  threat mode:
  - `←`/`→` and `j`/`k` to step, `Home`/`End` (also `0`/`$`)
  - `↑`/`↓` to pick among sibling variations
  - `Shift+←`/`Shift+→` for the previous or next branch point
  - `f` to flip, `space` to play the engine's best move
  - `l` to toggle the engine, `x` for threat mode, `?` for the help overlay
- **[have] Variation tree** (design in `VARIATIONS.md`):
  - Entering a move mid-game creates a branch instead of truncating.
  - The move list's menu, after 4steps's and lichess's: copy the move or
    the moves to it, move a line up or down, make it the main line, and
    delete from a move (refused for a running match's line). Variations
    fold.
  - lichess offers two layouts: column (two moves per row) and inline
    (variations in parentheses); ours is one move per row.
- **[have] Comments and glyphs (lichess):** `!`, `?`, `!!`, `??`, `!?`,
  `?!` plus free-text comments per move, in the PGN-style record format.
  Still open: positional glyphs (`$10`-`$19`) and board shapes saved with
  the move (`%cal`/`%csl`).
- **[have] Plan ahead in a live game** (arimaa.com Expert mode and the "P"
  plan window), as below:
  - Play both sides forward from the live position. Planned moves are
    marked in the move list (`planmove`).
  - "Send" submits only the first move of the planned line.
  - The plan window is separate, so an incoming opponent move doesn't
    wipe the plan, and you can't send by accident.

  For us it's the variation tree: a move away from the live position is
  a plan, never sent, and Play sends the plan's first move. The
  exploration tint isn't done.
- **[P2] Copy and paste everywhere (4steps context menu):**
  - Copy the position (short format)
  - Copy the move sequence up to the current ply (done, in the move
    list's menu)
  - Paste a move or record anywhere (a record can be pasted in the Record
    dialog)
  - Drag and drop a record file onto the window (Nibbler, En Croissant)
  - arimaa.com's "M" opens the move list as plain text, using g/s notation,
    with the current position in setup notation above it.

## 5. Engine analysis

The current engine panel shows depth, eval, PV and log per side.

- **[have] Analysis mode** (`ANALYSIS.md`): an engine on the displayed
  position, restarting as you navigate, with lichess's toggle (`l`), an
  eval bar beside the board, and the first PV turn drawn as arrows.
  - Not done: the in-progress partial turn (4steps's "Run analysis"
    passes it to the engine). AEI has no way to send one; see
    `ANALYSIS.md`.
- **[have] Interactive PV:** hovering a PV turn previews that position
  (lichess's PV board preview), and clicking one adds the PV as a
  variation up to that turn. The chips hold still while the pointer is
  over them.
- **[P2] Threat mode (lichess `x`):** analyse as if you passed, which shows
  the opponent's threats. In Arimaa this answers the most common question,
  "what is threatening goal or captures?", and maps onto a null move: hand
  the engine the position with the other side to move.
- **[P2] Multiple engines side by side** (En Croissant). Each engine gets
  its own panel and arrow color. AEI has no standard multi-PV, so
  side-by-side engines are the practical way to compare lines.
- **[P2] Whole-game analysis** (lichess server analysis, Nibbler's
  "automatic full-game analysis"):
  - Run an engine over every ply in the background.
  - Plot an eval graph (lichess `ui/chart`); clicking the graph jumps to
    that ply.
  - Mark inaccuracies, mistakes and blunders from eval swings (lichess
    `nodeFinder.evalSwings`).
  - Cache results per position hash (lichess `evalCache`).
- **[P3] "Learn from your mistakes"** (lichess `retrospect`): step through
  your large eval drops and try to find the better move. It builds on
  whole-game analysis.
- **[P3] Restrict search to chosen moves** (Nibbler's `searchmoves`). AEI
  has no equivalent, so this would need an extension.

## 6. Position editor and puzzles

- **[P2] Board editor (lichess `ui/editor`, 4steps "Set up custom
  position"):**
  - A palette of pieces; drag onto or off the board.
  - Side to move, clear, and the starting setup.
  - Validation that 4steps does in its puzzle loader: no unsupported piece
    on a trap, no rabbit on its own goal row, piece counts within limits.
  - Import and export of the short position format.
  - "Start from this position" for play or analysis.
- **[P3] Puzzles (4steps `puzzles.cpp`, lichess `ui/puzzle`):**
  - 4steps loads puzzle files and has reshuffle, hint (highlights a square
    the solution changes), answer, auto-advance, auto-undo and
    auto-explore after a miss.
  - lichess adds themes, streaks, ratings and a timed "storm" mode.
  - Goal-in-N and capture puzzles generated by an engine would be a
    natural source.

## 7. Online play and spectating (use cases 2 and 3)

- **[have] Lobby** (the lobby window's arimaa.com section, `GAMEROOM.md`): my games,
  invitations, open, live and recently finished games (the ASIP 2.0
  `state` lists), postal games, a player search with their past games,
  and creating or inviting to a game (side, time control, rated).
  Still open:
  - The 4steps bot launcher: create a game against a server bot, with
    "join on creation".
  - A "Last updated" time, and "last server response" age in the corner as
    a connection-health cue (4steps).
- **[have] Game header like arimaa.com:** the panel shows the game id
  (and the permanent id once it ends), Rated or Unrated with the time
  control, and the player bars show names and ratings. Still open: the
  result as Won/Lost beside each name, and for finished games the dates
  played and average move times.
- **[P2] Game lists with board thumbnails** (the arimaa.com lobby). Recent
  games show a small board of the final position, the move count and
  result (`g 71`), and the time control. Clicking the thumbnail opens the
  game as a viewer, from that side.
- **[P2] Game comments** (arimaa.com "G" and `comments.cgi`, plus the
  "Commented Games" list): a discussion thread for each finished game.
  Showing it for gameroom games is read-only and easy.
- **[have] Sound events** (arimaa.com): step, capture, win, lose,
  clock, start, setup, chat, a refused move, and the opponent joining and
  leaving (the server doesn't tell players about spectators).
- **[have] Chat** under the board (arimaa.com and 4steps), at a player's
  seat; spectators don't get it from the server.
- **[P2] Several games at once:** a window per game and a lobby window,
  planned in `WINDOWS.md`. lichess's "move on" (`moveOn.ts`) jumps to
  the next game where it's your turn. This is essential for postal play.
- **[P3] Postal conveniences:**
  - Conditional moves (lichess "forecast" for correspondence games).
  - A per-game notes pad.
  - Notifications when the opponent moves. (While the app runs and is
    logged in, the arimaa.com button counts postal games waiting on your
    move and announces new ones; nothing tells you otherwise.)
- **[P3] Opening and setup explorer** (lichess `explorer`, En Croissant's
  database). Show statistics per setup and early position from a local
  copy of the arimaa.com game archive. Setups have a much smaller space
  than chess openings, so a "setup explorer" alone would be valuable.
  Position search ("absolute or partial", En Croissant) is the database
  version.

## 8. Engine matches and tournaments (bot tooling)

From Cute Chess. Our `play_match` plus the app's BvB are the seed.

- **[P2] Match runner:**
  - N games with sides alternating.
  - Concurrency.
  - A shared time control.
  - A setup book, the analogue of an opening book: start games from fixed
    setups or early positions so results aren't all the same game.
  - Output: a running score, Elo difference with error bars, and SPRT for
    regression testing.
- **[P2] Adjudication:** resign or score after K plies past an eval
  threshold, to save time.
- **[P2] Output** is a file of game records. Engine logs per game are
  optional.
- **[P3] Round-robin and gauntlet tournaments** with a crosstable (lichess
  `tournament` and `swiss` for presentation).
- These should live in a crate or CLI first, with the app as a frontend,
  per the architecture in `CLAUDE.md`.

## 9. Settings, layout and accessibility

- **[have] Settings dialog**: light or dark appearance (or the
  system's), theme, coordinates, auto-rotate (human at the bottom),
  animation speed (ms per step, as 4steps has), hover input (arrows or
  step mode), continuous step entry, sound and volume.
- **[P2] Dockable or resizable panels** (4steps docks for the move list,
  player bars and off-board pieces, with the layout saved). A lighter
  version for us: collapsible side panels with remembered sizes.
- **[P3] Screen-reader mode** (lichess `nvui`, a text interface for blind
  users). Typed move entry plus a text description of the board gets most
  of the way there.
- **[P3] GIF or image export of a game or position** (lichess
  `gifDialog`), useful for forum posts.

## 10. arimaa.com client in brief

How the arimaa.com client works overall, for reference:
- **Input:** hover arrows, then click. There's no dragging. Undo/redo step
  and undo/redo move buttons, Pass, and Send.
- **Setup:** a default setup is placed, and you click two pieces to swap
  them.
- **Expert mode:** move both sides, using the click shortcuts in section
  1. The Plan window is a separate copy, so you can't send by accident.
- **Replay:** Show at speeds S1–S4, with "auto" and "real-time" chaining.
  The move list is a `<select>`, with |< << >> >|.
- **Views:** three (Vw), plus mute and Help.
- **Clocks:** colored by the time left (green, yellow under 30 s, red
  under 10 s), with the steps left and the game time. Clicking a clock
  explains the time control.
- **Exports:** the move list as text (M), and game comments (G).
- **Checks** are done on the client: no net change, and the third
  repetition, which names the plies it repeats.

Not adopted:
- Alert boxes for errors and help (`uiHelpMessage` is `alert`).
- The Send button needed even in casual play. We can keep it optional.

## Suggested next steps

Every **[P1]** item, which made HvB against Sharp pleasant, is done, and
so is the variation tree that analysis, comments and threat mode depend
on (`VARIATIONS.md`). Only `x` waits, on threat mode. The next
milestone will come from the **[P2]** items.
