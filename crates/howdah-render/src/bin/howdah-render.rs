//! Draws a position as an image.
//!
//! Usage: howdah-render [--theme ID] [--size PX] [--flip] POSITION OUT
//!
//! POSITION is a file (or `-` for stdin) in any format `StartPosition`
//! reads: the long format's board diagram, the short format, or setup
//! moves. Lines starting with `#` are ignored. OUT's extension picks the
//! format (`.png`, `.jpg`). The default theme is classic-marble, 360 px.

use std::io::Read;

use howdah_arimaa::StartPosition;
use howdah_render::{Theme, render, save, theme_ids};

fn usage() -> ! {
    eprintln!("usage: howdah-render [--theme ID] [--size PX] [--flip] POSITION OUT");
    eprintln!("themes: {}", theme_ids().join(", "));
    std::process::exit(2);
}

fn main() {
    let mut theme = "classic-marble".to_string();
    let mut size = 360;
    let mut flip = false;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--theme" => theme = args.next().unwrap_or_else(|| usage()),
            "--size" => size = args.next().and_then(|s| s.parse().ok()).unwrap_or_else(|| usage()),
            "--flip" => flip = true,
            "-h" | "--help" => usage(),
            _ => files.push(a),
        }
    }
    let [input, out] = &files[..] else { usage() };
    let mut text = String::new();
    let read = if input == "-" {
        std::io::stdin().read_to_string(&mut text).map(|_| ())
    } else {
        std::fs::read_to_string(input).map(|t| text = t)
    };
    if let Err(e) = read {
        eprintln!("can't read {input}: {e}");
        std::process::exit(1);
    }
    let text: String =
        text.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n");
    let start = StartPosition::parse(&text).unwrap_or_else(|e| {
        eprintln!("{input}: {e}");
        std::process::exit(1);
    });
    let Some(theme) = Theme::load(&theme) else { usage() };
    if let Err(e) = save(render(&start.position, &theme, size, flip), out) {
        eprintln!("can't write {out}: {e}");
        std::process::exit(1);
    }
}
