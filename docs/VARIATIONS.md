# Variation tree: design

Status: reviewed (2026-10-02); open questions remain. Nothing here is
implemented yet.

Today a game is one line of moves. Entering a move at an earlier ply
truncates everything after it (`Game::truncate` in
`Session::commit_turn`). Analysis mode, the interactive PV, comments,
threat mode and planning ahead in a live game all need the game to be a
tree instead. This note covers the model, how the session and UI use it,
and a record format that can save it.

## Goals

- Entering a move away from the end of a line adds a branch. Nothing is
  lost without an explicit delete.
- Lines can be promoted, demoted, made the main line and deleted.
- Moves carry comments and glyphs (`!`, `?`, ...), and later board shapes.
- A live match keeps its own line, which the user can branch from but not
  change.
- Records with variations round-trip, and a plain record (main line only)
  stays exactly what `Game::to_record` writes today.
- `arimaa-core` keeps the model, so the CLI and Python bindings can read
  and write annotated games.

Not goals for now: merging transpositions (the same position reached by
different lines stays separate nodes), and a database of games.

The database will come, and it will be the primary store for games
played and analysed, variations included. The record format is mainly
for sharing with others and with other tools. So the tree is the model
both use; the record format doesn't have to carry everything the app
stores, only what's worth sharing.

## Model (`arimaa-core`)

A new `GameTree` type sits next to `Game`. `Game` stays as it is: it's
the right type for a single line, and `arimaa-aei`'s `play_match` and the
engine controller only ever need one line.

```rust
pub struct NodeId(u32);

pub struct GameTree {
    nodes: Vec<Option<Node>>, // arena; deleted nodes leave a None
    root: NodeId,             // ply 0, the empty board
}

struct Node {
    parent: Option<NodeId>,
    mv: Option<Move>,          // None only for the root
    position: Position,        // after `mv`
    children: Vec<NodeId>,     // children[0] continues the main line
    result: Option<GameResult>,// from the rules, or recorded at the end of a line
    end_marker: Option<String>,
    annotation: Annotation,    // comment, glyphs; shapes later
}
```

- **Ids are stable** for the life of the tree: an arena index, never
  reused. The UI and the session refer to moves by `NodeId` rather than
  by ply, since a ply number no longer names a single move.
- **Child order is meaningful.** `children[0]` is the main continuation;
  the rest are variations in the order shown. Promote and demote reorder
  siblings. "Make main line" promotes every node on the path to the root
  to `children[0]`.
- **Adding a move** (`add_setup`, `add_turn`, `add_notation` taking a
  parent `NodeId`) returns the existing child if one already plays the
  same move, so replaying a known line doesn't create duplicates.
  Setups count as the same when they produce the same position, whatever
  order the pieces are listed in. Otherwise the new child goes last.
- **Repetition** counts positions on the path from the root to the
  parent, not the whole tree. `is_third_repetition(node, end)` replaces
  the ply form, and walks `parent` links (bounded by the game length, so
  no cache is needed at first).
- **Results** live on nodes. A rules result (goal, elimination,
  immobilization) is set when the node is created. An external result
  (timeout, resignation) is attached to the node that ends the game with
  `end_line(node, result)`.
  - A rules result is terminal (`Node::is_terminal`): no move can follow.
  - After an outside result the position can still be played on, for
    analysis such as the likely finish after a resignation, shown by the
    player or a commentator. Those moves are always variations: the main
    line (`line_end`, `main_line`) stops at a node with a result.
- **Line helpers:** `path(node)` (root to node), `main_line()`,
  `line_through(node)` (the path to `node`, then `children[0]` down to a
  leaf), and `to_game(node) -> Game` for anything that needs a plain
  line.
- **Editing:** `delete(node)` removes the node and its subtree;
  `promote`/`demote`/`make_main_line`. `delete` on the root is an error.
- **Conversions:** `GameTree::from_game(&Game)` (returning the tree and
  the game's last node) and `main_game()` are cheap, so existing code
  paths keep working.

The `Annotation` type:

```rust
pub struct Annotation {
    pub comment: Option<String>,
    pub glyphs: Vec<Glyph>,       // move glyphs (!, ?, !!, ??, !?, ?!), at most one
                                  // per class, as in PGN NAGs; positional ones later
    // later: shapes (arrows and highlighted squares), as on the board today
}
```

The root's annotation is the game comment.

## Session

`Session` holds a `GameTree` instead of a `Game`.

- **Cursor:** `cursor: NodeId` replaces `cursor: usize`. The ply shown in
  the UI is the cursor's depth.
- **Current line:** the session also remembers `line_end: NodeId`, a
  leaf at or below the cursor. `→` follows the path toward `line_end`,
  not blindly `children[0]`, so stepping back into the main line and
  forward again stays in the variation you were looking at (lichess
  works this way). Clicking a move sets the cursor and `line_end =` the
  end of `line_through(node)`, unless the node is already on the current
  line.
- **Committing a move** at the cursor calls `add_turn(cursor, ..)` and
  moves the cursor to the new (or existing) child. No truncation. The
  existing repetition check uses the cursor's path.
- **Piece ids** for animation are computed along the cursor's path,
  incrementally: `ids` becomes a map from `NodeId` to `IdMap`, filled on
  demand from the nearest ancestor that has one. Navigation still
  animates one move forward or back; a jump between lines snaps (as a
  multi-ply jump does today).
- **Captures and last move** (`captured_view`, `last_move_view`) walk the
  cursor's path rather than slicing `moves()`.

### Matches

A match owns one line: `live: NodeId`, the node at the end of the game
being played.

- The live line is always the main line. Engine and human moves extend
  `live`, and adding a child of `live` makes it `children[0]` even if a
  variation was already there (an earlier plan). It can't be deleted,
  demoted or have a sibling promoted over it while the match runs.
- **Exploring during a match is allowed**: input away from `live`, or at
  `live` when it isn't your turn, adds a variation instead of being
  refused. This gives arimaa.com's "plan ahead" for free. A planned move
  at `live` is kept as a variation and is never sent. Sending the first
  move of a plan is a later, explicit action.

  This changes the current rule (`require_input` refuses input unless it's
  your turn at the latest move). Input at `live` on your own turn still
  plays the move.
- The board follows a new live move only while the cursor is at `live`,
  as now.
- `EngineTurn.moves` is `path(live)` in notation, so the controller's
  `newgame` + `makemove` sync is unchanged. Analysis mode will hand an
  engine `path(cursor)` the same way.
- When the match ends, the result goes on `live`, and the tree becomes an
  ordinary free-play tree. Plans made after the live node stay, as
  analysis after the end, and more can be added.

## UI

### Move list

Arimaa moves are long (up to four steps, and 16 placements for a setup),
so the list keeps one move per row instead of a chess-style two-column
layout.

```
 1g  Ra1 Rb1 …
 1s  ra8 rb8 …
 2g  Ed2n Ed3n Ed4n Ed5n  ?!
   ┆ 2g  Ee2n Ee3n Ee4n Dd2n  {Safer.}
   ┆ 2s  ed7s ed6s …
   ┆   ┆ 2s  hb7s …
 2s  ed7s ed6s ed5w …
 3g  …
```

- A variation is drawn after the move it replaces, indented one level
  per nesting depth, with a thin guide line. Each variation starts with
  its own move label, so it reads on its own.
- Long variations collapse to their first move with a "+n" count; the
  collapsed state is per node and kept in the session view.
- The current line is highlighted and the cursor's move is marked, as
  now. In a match, moves on the live line look like they do today, and
  moves in variations are tinted (the "exploration tint").
- Glyphs show after the notation; a comment shows below its move in a
  smaller, muted style. The comment is edited in a box under the move
  list for the cursor's move.
- A context menu on a move (and keys for the common items): copy move,
  copy line up to here, promote, demote, make main line, delete from
  here. Items that would touch the live line are disabled during a
  match.

### Keys

These are the keys `lib/shortcuts.ts` left out until variations exist:

- `↑`/`↓`: switch to the previous or next sibling of the cursor's move
  (the alternatives for the same ply), keeping the cursor at that ply.
- `Shift+←`/`Shift+→`: jump to the previous or next branch point on the
  current line.
- `←`/`→`, `Home`/`End` keep their meanings, along the current line.

### Boundary

- `SessionView.moves` becomes the tree, flattened in display order:

  ```ts
  type MoveNodeView = {
    id: number; parent: number | null; depth: number; // nesting depth
    label: string; notation: string;
    glyphs: string[]; comment: string | null;
    isMain: boolean; isLive: boolean; collapsed: boolean;
    childCount: number;
  };
  ```

  plus `cursor: number`, `line: number[]` (the current line's ids) and
  `live: number | null`. `ply` stays as the cursor's depth for the code
  that only needs a number (the setup check, labels). Sending the whole
  tree on every update is fine for game-sized trees; switch to deltas
  only if long analysis sessions show it's slow.
- `goto_ply(ply)` becomes `goto(node)`. Step keys get their own commands
  (`step_forward`, `step_back`, `next_sibling`, ...) so the session owns
  the current-line rule. `moves_after_cursor` becomes "moves after the
  cursor on the current line".
- New commands: `promote`, `demote`, `make_main_line`, `delete_from`,
  `set_comment`, `set_glyph`, `toggle_collapsed`.

## Record format

No Arimaa client has stored variations before, so the format borrows from
chess's PGN, keeping the parts that have worked well there and dropping
the parts chess tools only keep for compatibility. It starts from the two
Arimaa formats already in use:

- **The arimaa.com record format** ([notation page][notation]): optional
  `Tag: value` lines (Event, Site, Date, Round, Gold, Silver, Result), a
  blank line, then one move per line (`2g Ed2n Ed3n`). A multi-line tag
  value goes between `-=+=-` markers (used for chat logs). The move list
  can contain `takeback` lines, and `resigns` and `lost` as moves.
- **AEI's round-robin output** (`pyrimaa/roundrobin.py`): PGN tag pairs
  (`[White "bot"]`, `[Black "bot"]`, `[TimeControl ...]`, `[PlyCount ...]`,
  `[ResultCode "g"]`, `[Result "1-0"]`), a blank line, the move lines,
  and a `1-0`/`0-1` line. It's PGN-shaped so that bayeselo can rate
  tournaments from it.

The arimaa.com game archive (`allgamesYYYYMM.txt`, and an older
`ratedgames.txt` with results only) is a tab-separated database dump,
one game per row. It doesn't shape the format, but it shows what an
arimaa.com game carries, so records should have room for it:
- Players: username, id, title, country, rating and rating K, and
  whether each side is a human or a bot (`wtype`/`btype`).
- Game: `event`, `site`, `timecontrol` (the `M/R/P/L/G/T` format),
  `postal`, `rated`, `mode`, `plycount`, and start and end times as Unix
  timestamps.
- Result: `result` (`w`/`b`, and in old games `d` for a draw or `u` for
  unknown) and `termination`, using the `WinReason` letters plus `a`, `p`
  and `n`. All of them, with arimaa.com's definitions, are in
  `RESULT-CODES.md`.
- `movelist`: the plain record, with `1w`/`1b` labels (which the parser
  already reads) and newlines escaped as `\n`.
- `events`: a timestamped log. Per move it has the time used and both
  reserves (`after 5w, move used:N wresv:N bresv:N game:N`), which map
  straight onto `%emt` and `%clk`, plus joins and the result.

[notation]: http://arimaa.com/arimaa/learn/notation.html

### What to take from PGN, and what not

Taken, because it works well:
- **Variations as recursive parenthesized lines (RAV)**, placed after the
  move they replace, with the first child as the main line. Every chess
  tool understands this shape, and it nests cleanly.
- **Brace comments** that attach to the move before them.
- **Commands inside comments** (`[%cmd args]`, from the PGN spec
  supplement), used by ChessBase and lichess for clocks (`%clk`), time
  spent (`%emt`), engine evals (`%eval`), arrows (`%cal`) and square
  highlights (`%csl`). A reader that doesn't know a command still reads
  the comment, so new data can be added without breaking old readers.
  `%cal Gd2d4` and `%csl Re4` work unchanged with Arimaa square names.
- **The six move glyphs** `!`, `?`, `!!`, `??`, `!?`, `?!`, and PGN's
  numeric annotation glyphs (`$n`) for the rest, so the numbering
  (`$14` = gold slightly better, and so on) means what chess tools
  expect.
- **Several games per file**, each starting with its tags. It's how game
  collections, tournaments and puzzle sets travel.
- **Lenient import, strict export** (PGN's "import format" and "export
  format"): read what people and old tools write, always write one
  canonical form.
- **A starting-position tag** (PGN's `FEN` with `SetUp`) for games and
  puzzles that don't start from the empty board.

Not taken, because chess is stuck with them:
- **Free-flowing movetext and `1...` move numbers.** PGN lets moves wrap
  anywhere and needs `1...` to say whose move starts a variation. Arimaa
  records already put one move per line with its number and color
  (`2s`), which is clearer and is what every Arimaa tool reads. Keep
  that. The reader may accept several moves on one line, since the
  labels mark where each starts, but the writer never produces it.
- **Mandatory tags and placeholders** (the Seven Tag Roster with `"?"`
  and `????.??.??`). Arimaa's tags are optional; a missing tag is simply
  absent.
- **Latin-1.** Records are UTF-8.
- **Rest-of-line `;` comments and `%` escape lines.** Round-tripping can't
  keep them in place. The reader skips `#` lines as now, which covers the
  same need; the writer never produces them.
- **A result without a reason.** `1-0`/`0-1` alone don't say how a game
  ended, so the reason goes alongside it, as AEI already does (below).
  Draws can't happen under the current rules, but old arimaa.com games
  include a few, so `1/2-1/2` and `*` (unknown or abandoned) stay readable
  and writable for those. The end markers (`resigns`, `lost`, ...) stay
  accepted in the move list.
- **Ambiguity about comments at the start of a variation.** PGN doesn't
  say whether `( {text} 2g ...` belongs to the variation or its first
  move, and tools disagree (python-chess added a separate
  `starting_comment` for it). Here it's defined: a comment right after
  `(` is the variation's introduction, and every other comment belongs
  to the move before it.
- **Null moves.** Chess tools write them as `--`, `Z0`, `null` or `@@@@`,
  and many readers reject all of them. Threat mode needs a pass in
  analysis, but it doesn't need to be stored; leave it out of records
  until a use appears.

### The format

```
[Event "Example annotations"]
[Gold "alice"]
[Silver "bob"]
[Date "2026.10.02"]
[TimeControl "2/2/100/2/0"]
[PlyCount "84"]
[ResultCode "g"]
[Result "1-0"]

{Game comment. [%cal Ge2e5]}
1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Ha2 Db2 Cc2 Md2 Ee2 Cf2 Dg2 Hh2
1s ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8 ha7 db7 cc7 ed7 me7 cf7 dg7 hh7
2g Ed2n Ed3n Ed4n Ed5n ?! {The elephant leaves early. [%emt 0:00:12]}
(
{A quieter start.}
2g Ee2n Ee3n Ee4n Dd2n
2s ed7s ed6s ed5s ed4w
(
2s hb7s hb6s hb5s hb4s !?
)
)
2s ed7s ed6s ed5w ed5e
...
1-0
```

Where AEI's round-robin output and the arimaa.com format differ, this
follows AEI (it's what our own tools already write), except for the side
names: records say Gold and Silver, as the game does.

- **Tags** are PGN tag pairs, `[Name "value"]`, as AEI writes them (plus
  PGN's `\"` and `\\` escapes, which AEI doesn't need for bot names, and
  `\n` for a line break, our extension, since PGN can't put one in a
  tag), then a blank line. The reader also
  accepts the arimaa.com form (`Name: value`, with `-=+=-` around
  multi-line values). Unknown tags are kept and written back.
- **Side names** are Gold and Silver everywhere: `Gold`/`Silver` for the
  players and a `Gold`/`Silver` prefix on per-side tags. The reader maps
  `White` to Gold and `Black` to Silver, in tag names (`White`,
  `WhiteElo`, ...) and values, so AEI round-robin files and chess-style
  tags load. Writing `White`/`Black` (for bayeselo, which expects them)
  can be an export option later.
- **Tag names:** `Gold`, `Silver`, `TimeControl` (the Arimaa
  `M/R/P/L/G/T` format, the same string AEI writes), `PlyCount`,
  `ResultCode` and `Result` from AEI; the rest of arimaa.com's set
  (`Event`, `Site`, `Date`, `Round`); and, where PGN has the same idea
  and the archive has the data, `GoldRating`/`SilverRating` (read from
  `WhiteElo`/`BlackElo` too), `GoldTitle`/`SilverTitle`,
  `GoldType`/`SilverType` (PGN's `human`/`program`), `Mode`,
  `Annotator`, and `Position`. The
  arimaa.com game id needs a tag of its own (`Site` already holds the
  archive's `site` value, "Over the Net"); lichess puts the game URL in
  `Site`, and chess.com uses a separate `Link` tag, which fits better.
- **`Position`** holds the starting position in the short format
  (`Position: g [rrrrrrrr...]`, which `Position::to_short_string` already
  writes; the side letter is the side to move). Without it a game starts
  from the empty board with setups. Not supported yet: the reader
  refuses a record with a `Position` tag, since `GameTree` always starts
  from the empty board.
- **Result**, as AEI writes it: `Result` is `1-0` (gold won) or `0-1`,
  `ResultCode` is the reason letter `WinReason` already uses (`g` goal,
  `e` elimination, `m` immobilization, `t` timeout, `r` resignation, ...,
  plus `a`, `p` and `n` from old games; see `RESULT-CODES.md`), and the same `1-0`/`0-1` ends the
  move list. Imported historical games may also have `1/2-1/2` or `*`. An unfinished game has
  neither tags nor the closing token. The closing token also makes the
  end of each game unambiguous in a file of several games.
- **Moves** are one per line, as now, followed by optional glyphs and a
  comment. Move labels are read with either side letters, `g`/`s` or
  arimaa.com's `w`/`b` (`1w`, `1b`), and always written with `g`/`s`. Every move line inside a variation
  still has its label, and the reader checks it, so a hand-edited file
  gets errors with line numbers. A move ends at the end of its line.
  After a line has ended, an empty label (`19b`, as arimaa.com records
  end after a goal) is ignored.
- **Variations:** `(` and `)` each sit on their own line. A block holds
  an alternative to the move just before it, so its first move has the
  same label as that move. Blocks nest.
- **Continuations after a line's last move** (analysis after a
  resignation) are blocks whose first move has the *next* ply's label:
  `2g Ee2n` then `(`, `2s ee7s`, `)` continues 2g instead of replacing it.
  Since every move carries its ply, the two readings can't be confused;
  PGN can't express this, and chess annotators work around it by
  repeating the last move inside the variation, which the reader also
  accepts. The writer only uses a continuation block after the last move
  of a line; elsewhere the move's own line continues it. A hand-written
  continuation block before the line's next move is read, and the line's
  own next move still stays the main continuation. The writer indents nothing (old
  tools trim lines anyway); the move list does the indenting.
- **Comments** are in braces and may span lines; braces don't nest.
  Writers escape a literal `}` in a comment as `\}` and a backslash as
  `\\` (PGN has no escape, so a brace in a comment truncates it there).
  An introduction (a comment right after `(`) is only kept on a
  variation's first move; if that move later becomes the main line, the
  writer drops it.
- **Commands** used from the start: `%clk` (the mover's reserve after the
  move) and `%emt` (time used for the move), so match records keep the
  clocks, as arimaa.com's `timeused` does; `%cal`/`%csl` for the board
  annotations; and later `%eval` for analysis results, in centi-rabbits
  from gold's point of view.
- **`takeback`** lines (arimaa.com) are read by stepping back one move,
  and the move that was taken back is kept as a variation instead of
  being thrown away.
- **Several games** in one file follow one another, each with its tag
  block; a finished game ends at its result token, and a tag line after
  a blank line starts the next game.

A record with no variations, comments or glyphs is a plain record, and
writing the main line without tags is exactly today's `to_record`
output. Export offers "main line only" (for arimaa.com, pyrimaa and
anything else that reads plain records) and "full record".

Results: only the main line's result is written (as tags and the closing
token). The end words also give a result when there are no result tags
(`2s resigns`: gold wins by resignation), so analysis after them stays
off the main line. Rules results in variations are found again when the record is
read; an outside result (a resignation) at the end of a variation is not
written.

Parsing: `GameRecord::parse` (one game) and `GameRecord::parse_all` read
both forms into tags plus a `GameTree`; `Game::parse` calls it and takes
the main line, so loading an annotated file anywhere works.
`GameRecord::to_record` writes the full record, and
`tree.main_game().to_record()` the plain main line. Checked against the
350 games of arimaa.com's archive for August to October 2026: all
parse, and the rules results match the archive's.

## Build order

1. `GameTree` in `arimaa-core`, with tests: adding and reusing children,
   repetition along a path, results per line, promote/demote/main line,
   delete, `from(Game)`/`to_game`.
2. The record format: `GameTree::parse` and `to_record`, with tags,
   comments, commands, `takeback` and several games per file. Round-trip
   tests, the error cases (unbalanced parentheses, a variation before
   any move, a wrong label inside a block), and loading an AEI
   round-robin file.
3. `Session` on the tree, with the cursor, the current line and the match
   live line. The existing session tests should pass with the ply-based
   helpers rewritten; add tests for branching, following `→` through a
   variation, and exploring during a match.
4. The boundary (`SessionView`, commands, ts-rs types) and the move list
   with nesting, the current line and the context menu.
5. Keys, comments and glyphs in the UI, then export with variations.

Then analysis mode builds on it: the engine analyses `path(cursor)`, and
clicking a PV turn adds the PV as a variation (`add_notation` per turn).

## Decided

- Entering a move always branches; there's no overwrite setting.
- Exploring off the live line during a match is allowed.
- No earlier Arimaa client stores variations, so the record format
  follows PGN where that works well (see above), on top of the existing
  Arimaa formats. Where AEI's round-robin output and arimaa.com differ,
  follow AEI, except that sides are Gold and Silver everywhere
  (`White`/`Black` accepted when reading).
- Move labels: read `g`/`s` and `w`/`b`, write `g`/`s`.
- Variations display after the main-line move they're an alternative
  to, as PGN orders them and lichess, ChessBase and SCID display them.
- A game database, later, is the primary store; records are for
  sharing.

## Open questions

- **Old results in the core.** `WinReason` needs `p`, `a` and `n` (see
  `RESULT-CODES.md`). `p` and `n` only occur in games under the rules
  before 2008.07.01; a current game can't end that way. `GameResult` always has a winner, so draws and
  unknown results from old games also need a place: a variant of
  `GameResult`, or the record's result tags kept as they are without a
  `GameResult`.
- **`Date` format.** PGN and arimaa.com both use `YYYY.MM.DD`; keep it,
  or write ISO `YYYY-MM-DD` and read both?
- **The game archive.** Import arimaa.com's archive directly (each row
  becomes a record with tags and `%emt`/`%clk` from `events`), or convert
  it with a script?
- **Setups in variations.** The tree allows alternative setups (a
  variation at ply 0 or 1). That's cheap to support and useful for setup
  study, but the move list should show them compactly.
