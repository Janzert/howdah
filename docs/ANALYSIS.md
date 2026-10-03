# Analysis mode: design

Status: the first version is built (2026-10-03): build steps 1 to 5. The
design was reviewed with Brian, and availability, setups and partial
turns were settled (see "Decided"). Where the build differs from the
sections below, "As built" says how.

Analysis mode runs an engine on the position the board shows, without
playing. It restarts as you move around the game, and its best line can
be added to the game as a variation. It's item 5 of the suggested order
in `UI-SURVEY.md` (section 5 there has the ideas from other clients), and
it builds on the variation tree (`VARIATIONS.md`).

## Goals

- `l` (or a button) turns analysis on and off. While it's on, an engine
  searches the shown position until you move away, then starts on the new
  one.
- The panel shows depth, eval and the principal variation (PV), with an
  eval bar beside the board and the PV's first turn drawn on the board.
- Hovering a PV turn previews the position after it; clicking it adds the
  PV up to that turn as a variation, and `space` plays the first turn.
- What the engine found at a node stays with the node, so coming back
  shows it at once while the search goes deeper.
- Sharp and OpFor work without the user knowing their options.
- The model allows several engines at once and whole-game analysis
  later, though the first version runs one engine on the shown position.

Not goals for now: threat mode, side-by-side engines, whole-game
analysis, and saving evals in records. Each gets a short section below so
the first version doesn't block it.

## What the engines give us

From `ENGINES.md`, as they bear on analysis:

- **Searching until `stop`:** OpFor does this when no time control and no
  depth are set (the defaults). Sharp has no `go infinite`;
  `setoption name ignoretc value true` makes a plain `go` search until
  `stop`. Both may still answer early: OpFor stops by itself on a found
  win or a single legal move. An early `bestmove` ends the search; it
  isn't an error.
- **Progress:** OpFor sends standard `info depth/time/nodes/score/pv` at
  each new depth and each new best move. Sharp sends only a summary at
  the end unless `verbose` is set (our build allows it), and then
  `log ID …`/`log FS …` lines with a PV, which `SearchLog` parses.
- **`stop` latency:** Sharp answers at once; OpFor in 0.2-0.3 s since
  its `SEARCH_SLICE` change.
- **Scores** are from the mover's side. OpFor's are centi-rabbits, with a
  win near ±32,650 (`WIN_SCORE` 64000 / 1.96). Sharp's are about 10 per
  centi-rabbit, with wins near ±1,000,000 (`SearchEval::Decided`).
- **Positions:** Sharp ignores the side in `setposition`, so analysis
  sends `newgame` plus a `makemove` per move, as matches do. That also
  gives the engine the history it needs for repetition.

### Engine profiles

An analysis run needs options that a match doesn't (`ignoretc`,
`verbose`) and must read scores in the engine's own scale. Rather than
make users find these, the controller picks a **profile** from the
engine's `id name` after the handshake:

| Profile | Analysis options | Score |
|---|---|---|
| Sharp | `ignoretc true`, `verbose true` | log eval / 10; `Decided` is a win or loss |
| OpFor | none | `info score`; ≥ 32,000 is a win |
| Generic | none | `info score` as centi-rabbits; no win detection |

`EngineSpec` also gets an `options` list (name, value) sent after the
handshake for both play and analysis, for `threads`, `hash` and anything
a profile doesn't cover. A user option overrides a profile option of the
same name. The engines dialog edits the list as plain `name = value`
rows.

The profile is the one place engine quirks live; the Sharp-specific
`log_score` in `controller.rs` moves into it.

## What gets analysed

The **target** is the node the board shows (the session's cursor node),
given to the engine as `path(node)` in notation.

- **The in-progress turn is ignored for now.** AEI has no way to say
  "these steps are already taken": `setposition` with the mid-turn board
  would give the engine four fresh steps. So the engine keeps analysing
  the turn's start while you enter steps, and the panel says so ("12s,
  before your steps"). The PV arrows hide once you take a step, like the
  last-move trails. An AEI change to send a partial turn is planned (see
  "Later"); no current engine would handle it yet.
- **Setups are searched like any other move.** Silver's setup is a real
  search: the engine answers the setup gold chose. Gold's is less
  useful, but it isn't worth a special case either. The engine gets
  `newgame` (and gold's setup, at ply 1) and a plain `go`; its PV, if it
  sends one, is the setup's placements, and its `bestmove` usually ends
  the search at once (state `finished`). The board draws no arrows for a
  setup; the preview and "add as variation" work as for a turn.
- **A terminal node** (goal, elimination, immobilization) isn't searched;
  the panel shows the result instead. After a resignation or timeout the
  position is still analysed, which is what "analyse" in the game-end
  dialog wants.
- **Third repetitions** need nothing special: the engine has the history.

## When analysis is available

Analysis is available **whenever the user isn't a player in an online
game**:

- **Local games, matches included.** In human vs engine it's the user's
  own business. In bot vs bot the user may well want a different engine's
  view of the game being played. In a running match the analysis engine
  shares the CPU with the match engines, which weakens their play under a
  clock; the panel notes this while both run, and the `threads` option
  keeps it in check.
- **Spectated arimaa.com games** (use case 3). The gameroom already
  delays what spectators see, so that commentary on the live position
  can't help the players; analysis is no different.
- **Refused** while the user plays an arimaa.com game (use case 2), and
  turned off when such a game starts. After it ends, analysis is
  available again.

In a match the target is still the shown node, so with the board
following the live position, analysis follows the game move by move.
The game-end dialog's **Analyse** button closes the dialog and turns
analysis on at the final position.

Analysis never plays a move in a match. Adding a PV (below) at the live
node on the user's turn adds a plan, not the live move; to play it, the
user enters it on the board.

## Controller

Analysis runs in the existing coordinator task (`controller.rs`), next to
the match engines, with its own engine actor.

- **The actor is shared with matches.** `run_actor` already keeps an
  engine's move list in sync incrementally (`prepare`: `newgame` when the
  new list isn't an extension of the old one, then the missing
  `makemove`s). An analysis `Think` is the same command with no time
  control and no clock. Two changes:
  - the actor sends the profile's and the user's options once, after
    the handshake; and
  - a `Think` that arrives while the engine is searching first sends
    `stop` and waits for its `bestmove` (2 s, then the engine is treated
    as failed), so `makemove`/`newgame` never land mid-search. A match
    never does this, so play is unchanged.
- **The session says what to analyse:** `Session::analysis_target()`
  returns the generation, the node, its ply and `path(node)` in notation,
  or `None` (analysis off, or a terminal node). The coordinator compares
  it with the running request on every poke, as `sync` already does for
  `engine_turn`.
- **Analysis has its own actor**, separate from the match engines', even
  when it runs the same program: a second process with its own options
  and no clock.
- **Fast navigation coalesces.** Holding `→` pokes once per move. The
  coordinator only keeps the latest target: while a stop is in flight,
  newer targets replace the waiting one, and only the last is sent. No
  timer-based debounce is needed, since a search that's restarted at once
  costs little.
- **Staleness** uses the existing request ids: output from an old
  request is dropped, so a slow `stop` can't paint the previous
  position's PV on the new one.
- **Turning analysis off** quits the engine process. Keeping it alive
  between uses would save the start-up (Sharp's is quick, OpFor's under a
  second), but an idle engine holding its hash table isn't worth it.
  Revisit if starting feels slow.
- **Failure** (crash, garbage, a hang on `stop`) turns analysis off and
  shows why in the panel. Nothing in the game changes.

## Results

Engine output is turned into one **`AnalysisLine`** per update:

```rust
pub struct AnalysisLine {
    pub node: NodeId,              // the target it belongs to
    pub depth: Option<String>,     // as the engine writes it, e.g. "12+"
    pub score: Option<Score>,      // from gold's side
    pub pv: Vec<PvTurn>,           // validated, from `node`
    pub nodes: Option<u64>,
    pub time: Option<Duration>,
}

pub enum Score {
    CentiRabbits(i32),
    Win,  // gold wins
    Loss, // silver wins
}
```

- **Gold's side, everywhere.** Engines report from the mover's side; the
  controller flips silver-to-move scores. The eval bar, the panel and a
  future `%eval` (which `VARIATIONS.md` already defines from gold's side)
  then all agree. The match engine panel keeps showing each engine's own
  view, which reads naturally there.
- **The PV is validated** turn by turn from the target position with
  `TurnBuilder` (through `Game::play_notation` on a copy), so arrows,
  previews and "add as variation" only ever see legal moves. Validation
  stops at the first turn that doesn't parse or isn't legal, or after a
  turn ending the game; the rest is dropped and logged once. Each
  `PvTurn` carries its label (`12s`), its notation (with capture tokens
  filled in) and its steps for drawing.
- **Results stay with nodes.** The session keeps the deepest
  `AnalysisLine` per node (`evals: HashMap<NodeId, AnalysisLine>`, dropped
  with the node on delete and cleared with a new game). Coming back to a
  node shows its stored line immediately, marked as stored, until the new
  search passes its depth. This is also what whole-game analysis will
  fill. It's in memory only for now.

## Boundary

- **Commands:**
  - `set_analysis(on: bool)`: refused while the user plays an online
    game.
  - `add_line(from: NodeId, moves: Vec<String>)`: adds the moves as a
    line from `from` (reusing existing children, as `add_notation` does)
    and shows its last node. In a match it never plays the live move:
    lines added at the live node are plans. It isn't analysis-specific:
    pasting a line
    will use it too. Clicking a PV turn sends the PV's notation up to that
    turn, taken from the `AnalysisLine` the UI is showing, so a PV that
    changes between hover and click can't add something else.
  - `preview_line(from: NodeId, moves: Vec<String>) -> PositionView`: a
    query for the hover preview.
- **Events:** `analysis://update` carries the current `AnalysisView`
  (state, engine name, the target's label, the `AnalysisLine`, and any new
  log lines). Updates are a snapshot, not deltas, and the coordinator
  sends at most one every 100 ms per run, so a chatty engine can't flood
  the UI. `CLAUDE.md` suggests a Tauri `Channel` for streams like this;
  an event is used instead because the dev bridge already forwards
  events, and one analysis stream at 10 Hz is far from what events can
  carry. Revisit with several engines.
- **`SessionView`** gets `analysis: bool` (on or off) and, for the board,
  the stored line of the shown node if there is one, so the eval bar
  doesn't flash empty while navigating.
- **`AnalysisView.state`**: `starting`, `searching`, `finished` (the
  engine answered on its own, as it usually does for a setup), `idle` (a
  terminal node, with the result), `failed` (with the detail).

## UI

- **Panel.** The engine panel area shows an analysis section while
  analysis is on: engine name, depth, eval and nodes per second in the
  header; below it the PV as one clickable chip per turn (`12s ed7s
  ed6s …`); and the engine log folded under a disclosure. When it shows a
  stored line it says so ("stored, depth 14").
- **Eval bar** beside the board, on the side away from the move list:
  gold's share fills from gold's end, so it follows the board when it's
  flipped. Centi-rabbits map through a logistic, `1 / (1 + e^(-cr/k))`,
  with `k` to be tuned (300 puts a rabbit up at about 58%). A win fills it
  and shows `W`/`L` in place of the number.
- **PV arrows.** The first PV turn's steps are drawn the way last-move
  trails are (`lastMoveTrails`, dashed for pushes and pulls), in an
  engine color, over the last-move layer. They show while the board is at
  rest on the analysed node with no steps taken. lichess draws up to
  three moves; Arimaa turns are already up to four steps, so one turn is
  probably enough.
- **Interactive PV.**
  - Hovering a turn shows a small read-only board with the position after
    it (from `preview_line`), next to the panel. The main board doesn't
    change, so the mouse can sweep along the PV without animations.
  - Clicking a turn calls `add_line` with the PV up to that turn, and the
    board animates through the added moves as for a multi-ply jump.
  - `space` adds the first PV turn and shows it. While a match engine is
    thinking, `space` stays "move now" for that engine; at other times in
    a match it adds the turn as a plan, like a click.
- **Match panels.** In a match with engines, the analysis section sits
  below the match engines' sections and is labelled as analysis, with a
  note that it shares the CPU with them.
- **Keys.** `l` toggles analysis and `space` adds the best turn; both go
  into `lib/shortcuts.ts` (and so the help overlay). `x` (threat mode)
  waits.
- **Game-end dialog.** "Analyse" as described above.

## Later

These aren't in the first version, but the design above leaves room:

- **Partial turns over AEI.** A change to AEI will let a controller hand
  an engine the steps already taken in a turn, so analysis can follow
  them as they're entered. No current engine would handle it yet.
  - For context: the only "standard" Arimaa representation of a partial
    turn is the long position format, which can carry the move number,
    side and the steps played so far. It's specified that way, but
    probably no one implemented the partial-move part, and it isn't
    suited to AEI anyway.
  - The least disruptive change is a new AEI command that engines
    without support reject or ignore.
  - If the protocol version is bumped anyway, a flag on `makemove`
    marking the move as partial is cleaner. Engines would announce
    support through the version, and the controller would only send
    partial moves to engines that have it.
  - Until an engine supports it, analysis keeps searching the turn's
    start (as above). OpFor is the natural first engine to support it.
- **Threat mode (`x`):** analyse as if the side to move passed. That needs
  the position with the other side to move, which only `setposition` can
  give. It loses the history (so repetition isn't seen), and Sharp ignores
  the side, so it would need a patch in `tools/build-sharp.sh` first.
  OpFor honors the side.
- **Several engines:** the controller keys runs by a slot id instead of
  holding one, `AnalysisView` carries the slot, and each slot gets a panel
  section and an arrow color. AEI has no multi-PV, so this is how lines are
  compared.
- **Whole-game analysis:** a background run over the main line, one fixed
  depth or time per ply, filling `evals`. The move list can then show
  evals and swings, and a graph plots them. Caching by position hash
  (across transpositions and games) belongs with the game database.
- **`%eval` in records**, from `evals`, as an export option.
- **A CPU budget:** stop after N seconds or at a depth (a setting), for
  laptops on battery.

## Build order

1. **Engine profiles and options:** `EngineSpec.options`, profile
   detection from `id name`, options sent after the handshake, and the
   Sharp score scale moved into the profile. Tests with the test engine
   (options arrive; the generic profile) and parsing tests for each
   profile's scores.
2. **The actor:** `stop`-and-wait before a new `Think` while searching,
   with a test that a `Think` mid-search never sends `makemove` before the
   `bestmove`.
3. **Session and controller:** `analysis_target`, `set_analysis`, and
   the analysis run in the coordinator (alongside a match's engines),
   with coalescing and staleness, PV validation into `AnalysisLine`, and
   `evals`. Session tests for the target rules; a controller test with the
   test engine that navigates quickly and checks only the last target's
   output arrives.
4. **Boundary and panel:** the commands and event, ts-rs types, the
   analysis panel, the `l` key, and the game-end "Analyse". Checked in the
   browser pane against Sharp and OpFor.
5. **Board:** the eval bar, PV arrows, hover preview, click and `space`.

## As built

Differences from the design above, and details it left open:

- **Engine choice.** `set_analysis(engine_id)` takes the engine to use
  (`null` turns analysis off) instead of `on: bool`. The panel has an
  engine picker; `l` and the Analysis button use the engine picked last
  (`settings.analysisEngine`), or the first in the list.
- **Profiles** live in `arimaa-aei` (`Profile`, `Score`), not the app, so
  a CLI gets them too. Only Sharp's `log` lines are read as search
  progress. The actor sends the options, then `isready`, and passes on
  what the engine said about them, so a rejected option shows in the log.
- **Evals** are `Eval::CentiRabbits { value }` or `Eval::Decided { winner }`,
  from gold's side. The panel shows rabbits (`+0.35`); the eval bar uses
  `k` = 300.
- **A `bestmove` without a PV** becomes a one-turn line. Sharp sends no
  PV for a setup, so this is what shows (and what `Space` adds) there.
- **Analysis stays on** across new games and loaded records; only the
  target changes. Turning it off quits the engine, as proposed.
- **The hover preview** is a small board beside the panel, as proposed,
  kept inside the window.
- **`Space`** is "move now" while a match engine thinks; otherwise it
  adds the analysis's first turn (not while steps are being entered).
- **Coalescing** happens in the actor: when commands queue up behind a
  `stop`, only the newest `Think` is searched.
- **Checked** in the browser pane with Sharp and OpFor (2026-10-03):
  evals agree in scale (a horse up: Sharp +4.55, OpFor +3.71 a ply
  later), 13 quick arrow-key moves with OpFor ended on the right node
  with no failure, stored lines show at once on returning, the game-end
  Analyse button and a missing engine's failure message work.

## Decided

- Analysis is available whenever the user isn't a player in an online
  game: in free play, in every local match (human vs engine and bot vs
  bot), and when spectating, since arimaa.com already delays what
  spectators see.
- Setups are searched like any other move: silver's needs a real search
  to answer gold's setup, and gold's isn't worth a special case.
- Partial turns will come through an AEI change, later. Until then
  analysis searches the turn's start.
- Confirmed with Brian (2026-10-03), after the first version:
  - Evals are shown in rabbits (`+0.35`), everywhere: the match engine
    panel too (still from that engine's side, since it's labelled by
    engine).
  - The eval bar's `k` = 300 stays, to tune by use.
  - Analysis stays on across new games, unless the new game is one where
    it isn't allowed (an online game the user plays).
  - `Space` adds the analysis engine's best turn when no match engine is
    thinking.

## Open questions

- **Which AEI change for partial turns:** a new command, or a `makemove`
  flag with a protocol version bump (see "Later"). Not needed for the
  first version.
