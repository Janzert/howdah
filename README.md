# Arimaa Desktop

A cross-platform desktop client for [Arimaa](https://en.wikipedia.org/wiki/Arimaa),
built with Rust and [Tauri 2](https://tauri.app/) (Svelte + TypeScript frontend).

**Status: early spike.** You can set up, play and review games locally.
Planned: arimaa.com gameroom play, bots via AEI, engine-assisted analysis,
bot tournaments, postal games and bot development tooling.

## Layout

- `crates/arimaa-core`: rules, positions, notation and game records. Pure
  Rust with no UI dependencies.
- `app/`: the Tauri application. `src-tauri/` is the Rust shell and `src/`
  is the Svelte frontend.

## Prerequisites

- Rust (stable, 1.90+) and Node.js 20+ with npm.
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
so rendering can differ slightly from WebKitGTK.

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
cargo test --workspace   # core rules + app session logic
cd app && npm run check  # svelte-check / TypeScript
```

## Artwork and sounds

The classic board, piece and sound assets come from the original arimaa.com
client and have been released to the public domain. See
`app/src/themes/classic/ATTRIBUTION.md`.
