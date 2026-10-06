# Howdah

[![CI](https://github.com/Janzert/howdah/actions/workflows/ci.yml/badge.svg)](https://github.com/Janzert/howdah/actions/workflows/ci.yml)

Howdah is a cross-platform desktop client for [Arimaa](https://en.wikipedia.org/wiki/Arimaa),
built with Rust and [Tauri 2](https://tauri.app/) (Svelte + TypeScript frontend).
It runs on Linux and Windows; macOS builds in CI but hasn't been tried by hand.

**Status: early, in active development.** No releases yet; build it from
source as below.

## What it does

- **Play and review locally:** set up and play games on a board with drag,
  hover-arrow and step-mode input, with a move tree of variations,
  comments and move glyphs, and records read and written in the
  PGN-style Arimaa format.
- **Engines over [AEI](https://github.com/Janzert/AEI):** play against an
  engine or watch two play, with clocks and time controls. Tested with
  [Sharp](https://github.com/Janzert/arimaasharp) and
  [OpFor](https://github.com/Janzert/OpFor); add engines from the Engines
  dialog.
- **Analysis:** an engine searches the shown position, with its evaluation,
  principal variation (add it to the tree with a click) and search output.
- **arimaa.com:** log in to the gameroom to watch live games, open finished
  ones, and play: create and join games, invitations, chat, takebacks,
  postal games and timeouts. Analysis is off while you play there.

## Layout

- `crates/howdah-arimaa`: rules, positions, notation, the game tree and
  game records. Pure Rust with no UI dependencies.
- `crates/howdah-aei`: an async AEI controller and engine-vs-engine matches.
- `crates/howdah-gameroom`: the arimaa.com gameroom client.
- `app/`: the Tauri application. `src-tauri/` is the Rust shell and `src/`
  is the Svelte frontend.
- `docs/`: design notes (analysis, variations, engines, the gameroom,
  result codes).

## Prerequisites

- Rust (stable, 1.90+) and Node.js 22.12+ (or 24+) with npm.
- Platform packages for Tauri:

**Linux (Debian/Ubuntu)**

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Fedora: `sudo dnf install webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel`
plus `sudo dnf group install c-development`.

The current app only strictly needs `libwebkit2gtk-4.1-dev` and a C toolchain.
The rest are for tray icons, menus and bundling, which later features will use.
Sound playback uses GStreamer through WebKitGTK (`gstreamer1.0-plugins-good`).
If the window is blank on NVIDIA or some Wayland setups, run with
`WEBKIT_DISABLE_DMABUF_RENDERER=1`.

**Windows**: Microsoft C++ Build Tools ("Desktop development with C++")
and WebView2, which ships with Windows 10/11. WebView2 is Chromium-based,
so rendering can differ slightly from WebKitGTK. Tested on Windows 11.

**macOS**: Xcode Command Line Tools (`xcode-select --install`). It uses
WKWebView, which is close to WebKitGTK.

## Build and run

```bash
cd app
npm install
npm run tauri dev      # run the app with hot reload
npm run tauri build    # release bundle
```

## Test

```bash
cargo test --workspace                   # Rust: rules, engines, gameroom client, app logic
cargo clippy --workspace --all-targets
cd app
npm run check                            # svelte-check / TypeScript
npm test                                 # vitest
npm run e2e                              # Playwright smoke tests (needs Chromium)
```

CI runs these on Linux, Windows and macOS and builds the installers.

## Artwork and sounds

The classic board, piece and sound assets come from the original arimaa.com
client and have been released to the public domain. See
`app/src/themes/classic/ATTRIBUTION.md`.

## License

MIT; see [LICENSE](LICENSE).
