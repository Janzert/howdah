# Playing and watching on arimaa.com

How Howdah plays and watches games in the arimaa.com gameroom, and why.
The code is `crates/howdah-gameroom` (the HTTP client) and
`app/src-tauri/src/gameroom.rs` (the app's side); `CLAUDE.md` has the
architecture. Decided and checked against the live server in October 2026.

## Principles

- **The server is the authority.** In a game on arimaa.com the session
  plays only the server's moves, shows the server's clocks and takes the
  server's result. The local rules only keep the user from sending moves
  the server would refuse.
- **Least load on the server.** Where there's a choice of requests, use
  the one the server is sized for, as the browser client does: its lobby
  request, its seats, its long poll. Requests are spaced at least a second
  apart (long polls aside), and failures back off.
- **The browser client is the reference** for what the server expects;
  4steps (the Qt client) for ASIP and its workarounds. Where they differ,
  follow the browser.

## Logging in

- Logging in uses the browser's login form; its session works for ASIP
  too. The login sends the computer's time zone, so the gameroom's pages
  show local times.
- "Remember password" saves the login in a file in the app's config dir,
  the password obfuscated rather than encrypted (decided 2026-10-06, with
  no OS keychain): the gameroom's own login is plain HTTP, the keychain
  would be missing on some Linux desktops, and on macOS it would prompt
  after every update without a signed app. The password never goes back
  to the frontend, and network logs redact passwords and session ids.
- Logging out ends every login of the account, so logging in again as the
  same user drops the old session without logging it out.
- An expired login is renewed with the saved one, or else the user is
  logged out and the dialog shows the login form.

## The lobby

- One ASIP 2.0 `state` request gives the live games, the last few
  finished, the user's games, the open games and the invitations. The
  dialog asks every 20 s while it's open (as the browser lobby does), and
  a lobby watcher asks every minute while the user is logged in: it counts
  invitations to the user and keeps the login alive.
- Postal games aren't in the live list; "Show postal games" fetches the
  gameroom's postal page, only when asked.
- A player's finished games come from the gameroom's search and past-games
  pages, read leniently (a row that doesn't parse is skipped).

## Seats

- Viewers and players take their seats the browser's way (its game-window
  page), and follow the game on the browser's game server with its long
  poll (`maxwait` 300 s), which answers as soon as something changes. An
  ASIP viewer seat is the fallback; the server answers those only every
  ~10 s.
- If the game server drops a seat, the watch takes a new one (up to three
  in a row), or gets the final state if the game ended meanwhile.

## Watching

- A live game's moves arrive with animation; the board follows the live
  position unless the user is elsewhere (then the missed-move alert), as in
  a local match. Analysis is allowed: the gameroom already delays what
  spectators see.
- A finished game opens by its permanent id, whole, at the start of play,
  with each move's time kept as `%emt`. A watched game's record gets its
  permanent id as `GameId` once it ends.
- Spectators don't get a game's chat, and players aren't told about
  spectators: the server has no viewer count or list.

## Playing

- **Creating and joining.** The New game form creates an open game (or
  invites a player, below); the dialog lists the user's games (Play to sit
  again, "Your move" first) and others' open games (Play as the free
  side). Until an opponent sits, the panel says it's waiting, the user's
  first move is held, and the game can be cancelled. Games made by
  accepting an invitation can't be cancelled, only played or resigned.
- **Moves.** A human's move against the server is sent, not played: it
  shows as sent until the server's move list has it. The server's `ok`
  doesn't mean a move was played (after a refusal it drops legal moves for
  a few seconds, and it ignores moves after the game ends), so a move not
  back after a few seconds is checked against the full state and sent
  again. A refusal turns the move back into a plan and shows the server's
  message. Moves carry their capture tokens, which the server requires.
- **Takebacks.** "Ask for takeback" and Accept/Decline follow the server's
  takeback field. Rated games ignore takebacks on the server, so Howdah
  doesn't ask in them. Undo at the live position doesn't ask: a request
  goes to another person, so only the button sends one.
- **Resigning and leaving.** Resign asks for a second click. "Leave game"
  stops playing here; the game goes on at arimaa.com with the user's clock
  running, and Play under the user's games takes the seat again.
- **Results** are the server's. A player left with only third
  repetitions loses in Howdah's own games, but arimaa.com probably
  doesn't check that, so in its games Howdah doesn't apply it either.
  The server does refuse a third repetition itself ("Bad Move: position
  repeats for the 3rd time"), counting from the position after the
  setups as Howdah does. So a player left with only repetitions can't
  move, and the panel says so (`SessionView.stuck`): at the user's seat,
  that they can only resign or let the clock run out.
- **Time.** The server flags a player out of time itself (a second or so
  past the allowance) and tells both seats a few seconds later; no client
  claims a win on time. Until it does, the panel says the clock has run
  out and the result is coming.
- **Presence.** A player is away once they leave or their seat's poll
  connection closes, and back with their next request; a player assigned to
  a game who never sat counts as away too. The player bars mark them, and
  the panel notes when the user's opponent isn't at the table.
- **Chat.** Players' chat shows below the board, as in the web client,
  with a message box at the user's seat. An opponent's line while the
  window isn't focused asks for attention, as their move does. Lines are
  shown as plain text
  (the server stores them as sent), and empty messages aren't sent (the
  server would add an empty line).
- **Invitations.** ASIP has none, so they use the browser lobby's pages:
  the New game form's Opponent field invites a player, a background wait
  on the inviter's waiting page brings the answer (accepted, the game
  opens; declined, with the reason), and the dialog lists invitations
  both ways with Accept, Decline and Cancel.
- **Postal games** are postal because of their time control (days per
  move, or the gameroom's "No time limit", `0/0/0/0/0`, which is played
  untimed). The New game form offers the gameroom's postal time controls,
  clocks of a day or more read `Nd h:mm`, and the user's games show which
  wait on the user's move. While logged in, the lobby check every minute
  counts those on the arimaa.com button and announces new ones (the
  window's own game aside). The lobby's list has no turn before the first
  move, which is taken as gold's setup.
- **After a game** players often stay at the table to chat, so the
  game-end dialog offers only analysis and review, and the seat keeps
  following the table's chat (the server keeps it open after the end and
  wakes the long poll for each line). When the server stops answering
  for the table (it clears finished tables away, perhaps sooner for
  event games), the chat closes with a note instead of the message box; the lines stay
  (chat only grows, so a state with fewer lines, like a finished game's
  page, never replaces them).
  The game's panel offers a new open game with the same time control and
  rating, as either side, which leaves the table.
- **No analysis while playing** (see `ANALYSIS.md`), postal games included,
  and not after leaving a game mid-play until another game is loaded.

## Clocks

- The server stamps its replies and turn starts in whole seconds. Howdah
  estimates the server's clock from every stamped reply (`ClockSync`):
  each request's send and arrival times bound the offset, the bounds are
  intersected (Cristian's algorithm with Marzullo-style intersection, on
  the monotonic clock), and the estimate allows for the expected rounding
  and half the quickest round trip. Measured against an NTP-synced
  computer it stays within about a tenth of a second.
- A turn's clock is anchored at the server's turn start; the user's own
  clock runs ahead by the one-way delay, so it shows the time left for a
  move sent now. Before the game starts the clock stands still.

## Network trouble

- Failed polls retry: within 5 s after a network error, backing off to
  30 s after the server's own errors.
- TCP keepalive makes a long poll on a silently dead connection fail in
  about half a minute.
- After the computer sleeps (the wall clock runs ahead of the monotonic
  one), the old poll is dropped for a full state.
- A move that failed to send is checked against the state before it's sent
  again, and not resent while the state can't be checked.

## Not built

- A window per game: everything plays and watches in the main window's
  session.
- Inviting from a list of players online, scheduled games, and an
  opponent rating range in the New game form.
- An engine playing on arimaa.com, and a postal controller for bots (on
  hold). An engine would use the gameroom's bot API, as `gameroom.py`
  does, or ASIP as 4steps's bot launcher does.
- Sounds for chat and for players coming and going (the web client's
  aren't public domain).
