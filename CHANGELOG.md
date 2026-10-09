# Changelog

Notable changes to Howdah, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [semantic versioning](https://semver.org/) (0.x while Howdah is
young).

## [Unreleased]

### Changed

- A refused third repetition names the moves after which the position
  stood before ("as after 12g and 14g"), as arimaa.com does.
- Installing an engine update adds the new version as its own engine,
  with the earlier one's options, and offers to remove the earlier one;
  until then both can be used. A new engine whose name another engine
  already has gets its version added to the name (which can be changed in
  the Engines dialog).
- Deleting an engine installed from a manifest deletes its downloaded
  files too, after a second click.
- The arimaa.com game panel says whether the game is rated, with its
  time control, instead of the server's event name ("Casual game" for
  nearly every game), and the disabled "Ask for takeback" button says
  why it's disabled.

### Fixed

- Games ended by the game or turn limit are scored by arimaa.com's rule:
  with equal piece counts, the side ahead after the most recent turn
  where the counts differed wins (silver only if they never differed),
  not silver on every tie.
- "Move now" right after Sharp started thinking no longer makes it lose
  on time, and analysis no longer stalls when Sharp is stopped that
  early. (Sharp's next release also answers such a stop with a move.)
- On Linux, drop-down lists and tooltips match the light or dark setting
  instead of the desktop's theme.
- The analysis line holds still while the pointer is over it, so a click
  adds the line that was shown.
- Long player names shorten instead of wrapping in a narrow window, and
  the setup header no longer wraps.

## [0.1.0] - 2026-10-08

The first release, with installers for Linux (AppImage, deb, rpm),
Windows (MSI and NSIS setup) and macOS (dmg, signed ad hoc). None are
code-signed; the release notes say how to get past SmartScreen and
Gatekeeper.

### Playing and reviewing

- The complete rules, including the repetition rule and the one-minute
  setup allowance in timed games.
- A board with drag, hover-arrow and step-mode input, animated moves,
  frozen pieces marked, and themes.
- A move tree with variations, comments and move glyphs, and a move list
  that can show each move's time or the time into the game.
- Game records in the PGN-style Arimaa format, read and written,
  including variations, results inside variations and move times.
  Records from the arimaa.com archive read as is, `takeback` lines
  included.
- Clocks colored by the time left for the turn, as on arimaa.com.
- Sounds for game events, including per-theme board sounds.
- Light and dark colors, following the system or chosen in Settings.

### Engines

- Play against an AEI engine or watch two engines play, with clocks and
  time controls.
- Download and update engines from their manifests: Sharp and OpFor are
  offered from the first run, and other engines can be added by manifest
  URL, manifest file, or program path.
- Engine options as fields in the engine's settings, for one game, or
  for analysis.

### Analysis

- An engine searches the shown position, with its evaluation, principal
  variation (added to the tree with a click) and search output.

### arimaa.com

- Log in to the gameroom to watch live games and open finished ones.
- Play: create and join games (rated or unrated, live or postal),
  invitations, chat, takebacks, timeouts, and opponent presence.
- Keeps playing through network drops and sleep, and resumes games in
  progress.

[Unreleased]: https://github.com/Janzert/howdah/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Janzert/howdah/releases/tag/v0.1.0
