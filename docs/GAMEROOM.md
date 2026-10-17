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
- The login form takes the username or the account's email address. The
  login asks for the lobby's lists at once (the lobby wants them next
  anyway); their `me` gives the user's gameroom username, which the app
  shows ("Logged in as") and finds the user's games by. The saved login
  and logging in again after an expiry keep what was typed.
- Logging out ends every login of the account, so logging in again as the
  same user (by gameroom username, however they logged in) drops the old
  session without logging it out.
- An expired login is renewed with the saved one, or else the user is
  logged out and the lobby shows the login form.

## The lobby

- One ASIP 2.0 `state` request gives the live games, the last few
  finished, the user's games, the open games and the invitations. The
  lobby window asks every 20 s while it's shown (as the browser lobby
  does), and a lobby watcher asks every 50 s while the user is logged
  in, also while the lobby is hidden: it counts invitations to the user
  and keeps the login alive. The backend keeps the last lists, so a game
  window opened between polls shows the counts at once.
- Postal games aren't in the live list; "Show postal games" fetches the
  gameroom's postal page, only when asked.
- A player's finished games come from the gameroom's search and past-games
  pages, read leniently (a row that doesn't parse is skipped).

## Seats

- Viewers and players take their seats the browser's way (its game-window
  page), and follow the game on the browser's game server with its long
  poll (`maxwait` 300 s), which answers as soon as something changes. An
  ASIP viewer seat is the fallback; the server answers those only every
  ~10 s. A player's seat can be taken in the gameroom's unrated mode
  (see Server bots under Playing).
- If the game server drops a seat, the watch takes a new one (up to three
  in a row), or gets the final state if the game ended meanwhile.

## Watching

- A live game's moves arrive with animation; the board follows the live
  position unless the user is elsewhere (then the missed-move alert), as in
  a local match. Analysis is allowed: the gameroom already delays what
  spectators see.
- Move times: a move that arrives on its own gets the server's
  `lastmoveused` as `%emt` (in played games too). Moves made before we
  sat, or several arriving in one reply, have no time; the state only
  carries the last move's. When the game ends with any move untimed,
  the finished game is fetched once and its `timeused` fills them in.
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
  doesn't ask in them: the button is disabled, and its tooltip says why
  (`SessionView.takebackBlocker`, as for every other reason it can't ask).
  The panel says whether the game is rated, with its time control; the
  server's event name is left out, since it's "Casual game" for nearly
  every game, rated or not. Undo at the live position doesn't ask: a request
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
- **The chat room.** The gameroom's room-wide chat (`/arimaa/chat/`,
  separate from a game's) is the lobby's Chat room panel. Entering is
  the user's choice and is public: the room shows who comes and goes,
  and everything is archived, so the panel says so before Enter. The
  room has no push: its server answers polls, enters the room on the
  first one, and times out a user who hasn't asked for about a minute
  (the archive is full of `timeout`/`in` pairs from clients polling
  slower). Howdah asks every 5 s while the panel is shown and something
  happened there in the last two minutes, every 10 s while it's shown,
  and every 30 s otherwise, slower than the room's own page (every
  second, slowing to ten). It says `out` when the user leaves, closes
  the panel (nobody should sit in the room unseen), logs out of
  arimaa.com or quits. Messages get the room's formatting: links, its
  faces (shown as emoji: its images aren't ours to ship), piece
  symbols (`:ge`, drawn with the board's pieces), `/me` emotes, and
  the poster's color. White (`ffffff`) is the room's spoiler tag (its
  log is white, so the text shows only when selected), so a message in
  white has its text (not the name) hidden until clicked; a revealed one
  keeps a dashed outline and hides again with another click. The
  color popup's "Spoiler (next message)" sends the next message in
  white, then goes back to the user's color. Only exact white counts: other pale colors
  are darkened, and in the dark appearance dark colors lightened, as far
  as needed to be readable.
  Unlike the room's script, symbols aren't replaced inside links, and
  every occurrence is replaced, not just the first.
  - **The archive.** The room's archive (`archsrv.php`) needs no login,
    only the Referer every request carries, and gives the 24 hours of
    events from any moment. Howdah keeps the room's log locally: the
    Chat archive panel (open logged out too) shows a month calendar with
    each day's messages and people where kept, all of the day or part
    (a partial day: today, the start of a visit's backlog, or a request
    cut short), and opening a day asks the archive only for the
    stretches not yet kept, from the start of each, so a day costs one
    request (two or three at most, for a 25-hour day or gaps at both
    ends) and never again. What the user sees in the room is kept too:
    the room's first reply is the unbroken tail of its log, and each
    poll brings everything since, so a visit covers from just after the
    backlog's first line until its last poll. Where a visit and an
    archive request cover the same time, the archive (the server's own
    record) is read. Days are the computer's local days, as on the
    archive's own page. Searching the kept log is planned.
  - **Leaving for games.** The custom in the gameroom is to leave the
    chat room while playing a rated game against a person that isn't
    postal, and any event game. With the setting on (the default),
    Howdah leaves the room as such a game is seen at the user's seat
    (once the opponent has sat, to tell a person from a bot: a `bot_`
    name, or the lobby's bot flag) and says why in the panel. An event
    game is one whose `event` isn't "Casual game". It does so once per
    game, so entering again mid-game is the user's call.
- **Announcements.** `state`'s `announce` list gives each
  announcement's HTML and `modified`, the time of its last edit. The
  gameroom keeps them for years (in 2026 the oldest was from 2018), so
  Howdah shows only those edited in the last 90 days by the server's
  clock, each with Hide, and keeps the rest under "Older
  announcements". An edit makes one new again, hidden or not. The HTML
  is read in the backend into plain styled text (paragraphs, list
  items, bold, italic, web links), never shown as HTML, and links open
  in the system's browser. The `welcome` message (help for new members
  of the browser lobby) isn't shown.
- **Invitations.** ASIP has none, so they use the browser lobby's pages:
  the New game form's Opponent field invites a player, a background wait
  on the inviter's waiting page brings the answer (accepted, the game
  opens in a new window; declined, with the reason), and the lobby lists invitations
  both ways with Accept, Decline and Cancel. The Online list (who's
  logged in, from `state`'s `online`, leaving out anyone whose last
  lobby request, `idle`, is over 60 s old, as the browser lobby does)
  has Invite, which fills in the Opponent field, for players who take
  invitations (`inviteme`, the browser lobby's invite icon) and aren't
  `busy` (a bot may be: an invitation controller can play several
  games at once and turns its flag off when full), once their `idle` has moved since Howdah first saw them. The
  flag is only the player's wish (the server takes an invitation to
  anyone), and it isn't their setting until a lobby request of theirs
  applies it: a new login shows it on. A bot on the bot API
  (`gameroom.py`) never asks the lobby for anything, so its `idle`
  never moves and it gets no button; the browser lobby leaves bots out
  altogether. A bot whose controller polls the lobby, as one that
  answers invitations must, shows with Invite.
- **Server bots** (4steps's bot launcher): the lobby's "Show server
  bots" fetches, only when asked, the official bot ladder (about 30
  bots, ranked around the user: those above are the ones they have yet
  to beat) and the page of every bot arimaa.com runs, with the user's
  record against each. Both are asked for by the user's player id. The
  ladder is shown first, with the user's place in it; "All bots" shows
  every one. Choosing one reads
  its page (its time control, whether it's rated and running, and
  whether players may start it). Start posts the page's own form, which
  has the bot open a game with itself at the side the user didn't pick;
  the backend notes the bot's open games first (some bots keep one open)
  and checks the lobby every 4 s, for up to 40 s, for the new one. With
  "Join on creation" (on by default, as in 4steps) the user sits at it
  at once in a new window; without, it waits in the open games. "Keep
  rated" off (4steps's "Keep joined bot games rated") sits in the
  gameroom's unrated mode, which the browser lobby keeps as a cookie
  per player: a rated game against a bot becomes unrated. The server's
  answer to the start ("Bot started.") is shown under the form.
- **Postal games** are postal because of their time control (days per
  move, or the gameroom's "No time limit", `0/0/0/0/0`, which is played
  untimed). The New game form offers the gameroom's postal time controls,
  clocks of a day or more read `Nd h:mm`, and the user's games show which
  wait on the user's move. While logged in, the lobby check every 50 s
  counts those in the lobby and on the game windows' Lobby button, and the
  lobby announces new ones (games open in a window aside). The lobby's list has no turn before the first
  move, which is taken as gold's setup.
- **After a game** players often stay at the table to chat, so the
  game-end dialog offers only analysis and review, and the seat keeps
  following the table's chat (the server keeps it open after the end and
  wakes the long poll for each line). When the server stops answering
  for the table, the chat closes with a note instead of the message box; the lines stay
  (chat only grows, so a state with fewer lines, like a finished game's
  page, never replaces them). The server seems to clear a finished
  table a minute or two after both players are away ("Invalid Session
  Id"), and to keep it while one stays (5+ minutes seen). So the chat's
  poll also gets a full state after a lost connection, to stay present.
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
- The server closes idle connections after 5 s, so the client lets go of
  them after 4 s rather than send a request on one as it closes. It also
  closes a long poll's connection right after some replies without saying
  so; the next poll, sent at once, can go out on it as it closes. So after
  a good reply, a poll that fails with a network error is sent again at
  once on a new connection, and only a second failure shows the
  connection as lost.
- Network errors keep their causes ("error sending request: … connection
  reset"), without the URL, which may carry a session id.
- After the computer sleeps (the wall clock runs ahead of the monotonic
  one), the old poll is dropped for a full state. A lost connection also
  ends with a full state: the server marks a seat away when its polls
  stop, and only a `gamestate` marks it present again (long polls and
  moves don't), so without it the opponent would see the user as away
  for the rest of the game.
- A move that failed to send is checked against the state before it's sent
  again, and not resent while the state can't be checked.

## Not built

- One window per arimaa.com game: each Play, Watch or Open opens a new
  game window, even for a game that already has one (`WINDOWS.md`,
  phase 3).
- Scheduled games, and an
  opponent rating range in the New game form.
- An engine playing on arimaa.com, and a postal controller for bots (on
  hold). An engine would use the gameroom's bot API, as `gameroom.py`
  does, or ASIP. (4steps's bot launcher, now built, starts arimaa.com's
  own bots; it doesn't run engines.)
