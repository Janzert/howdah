# Game results and termination codes

arimaa.com records a finished game's result as two letters: the winner
and the way the game ended (the termination or reason code). The same
reason letters appear in the gameroom protocols, in AEI tools
(pyrimaa's `game.py`, the `ResultCode` tag in round-robin output), and
in 4steps. `WinReason::letter` and `WinReason::from_letter` in
`arimaa-core` (`outcome.rs`) use them.

## Winner

The game archive's `result` field, and the first letter of the gameroom
`result`:

| Code | Meaning |
|---|---|
| `w` | Gold won |
| `b` | Silver won |
| `d` | Draw (only in old games; see `n` and `s` below) |
| `u` | Unknown (seen with `a`, abandoned) |

arimaa.com's data calls the sides white and black, hence `w` and `b`.

## Termination codes

From arimaa.com's [reason codes][reasons] page, with the dates it gives:

| Code | Meaning | `WinReason` |
|---|---|---|
| `g` | A rabbit reached the goal | `Goal` |
| `m` | The player had no move (immobilization) | `Immobilization` |
| `e` | No rabbits left (elimination) | `Elimination` |
| `t` | The player ran out of time | `Timeout` |
| `r` | The player resigned | `Resignation` |
| `i` | A bot submitted an illegal move (from 2009.02.20) | `IllegalMove` |
| `f` | Forfeit: the player didn't show up for a scheduled game | `Forfeit` |
| `a` | Abandoned: both players left | none yet |
| `s` | Score decided the result after the game time ran out | `Score` |
| `p` | A position repeated three times (before 2008.07.01) | none yet |
| `n` | No rabbits left for either player; a draw (before 2008.07.01) | none yet |

Notes:
- `p` and `n` come from the rules before 2008.07.01. Under the current
  rules a third repetition is an illegal move, which `Game` refuses, and
  a turn that removes both sides' last rabbits is decided by the
  current elimination rule. They appear only in old games.
- `a` and `u` have no winner, and `n` (and the odd `s`) can be draws, so
  `GameResult`, which always has a winner, can't represent them yet.
  Importing old games needs that (see `VARIATIONS.md`, open questions).
- `s` is decided by score. For games from 2008.07.01 on, the rule on
  arimaa.com's match rules page
  (<http://arimaa.com/arimaa/learn/matchRules.html>) is the one in use:
  the player who has more pieces after the last completed turn wins; if
  they're equal, whoever had more pieces after the most recent turn where
  the counts differed wins; if they never differed, silver wins.
  `arimaa-core` (like pyrimaa) still only compares the final counts, with
  silver winning ties; implementing the full rule is a to-do.
- The app also uses `Forfeit` when a local engine fails (crashes or
  stops answering), which is broader than arimaa.com's meaning.

## Counts in the archive

For a sense of how rare the old codes are, `ratedgames.txt` (74,604
rated games, 2002 to November 2008) has: `g` 51,635, `t` 10,944, `r`
8,592, `m` 1,895, `e` 812, `p` 697, `n` 20, `s` 6, `a` 3. The winner
field has 21 draws (`d`, with `n` or `s`) and 3 unknown results (`u`,
all `a`).

[reasons]: http://arimaa.com/arimaa/gameroom/reasonCodes.html
