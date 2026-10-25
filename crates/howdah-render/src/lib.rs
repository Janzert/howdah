//! Draws Arimaa positions as images with the app's image themes (the
//! classic arimaa.com marble and stone artwork, embedded from
//! `packages/board/src/themes/classic/`), placed as the app's board does it: pieces
//! centred in the theme's grid, scaled by its `pieces.scale`.

use std::collections::HashMap;

use howdah_arimaa::{Position, Square};
use image::imageops::{FilterType, overlay, resize};
use image::{DynamicImage, RgbaImage};
use serde::Deserialize;

macro_rules! classic {
    ($($file:literal),* $(,)?) => {
        &[$(($file, include_bytes!(concat!("../../../packages/board/src/themes/classic/", $file)) as &[u8])),*]
    };
}

/// Embedded files by their path relative to the theme manifest.
type Files = &'static [(&'static str, &'static [u8])];

/// Theme manifests with every file they name.
const THEMES: &[(&str, Files)] = &[
    (include_str!("../../../packages/board/src/themes/classic/marble.theme.json"), CLASSIC_FILES),
    (include_str!("../../../packages/board/src/themes/classic/stone.theme.json"), CLASSIC_FILES),
];

const CLASSIC_FILES: Files = classic!(
    "BoardMarbleBig.jpg",
    "BoardStoneBig.jpg",
    "pieces/GoldElephant.png",
    "pieces/GoldCamel.png",
    "pieces/GoldHorse.png",
    "pieces/GoldDog.png",
    "pieces/GoldCat.png",
    "pieces/GoldRabbit.png",
    "pieces/SilverElephant.png",
    "pieces/SilverCamel.png",
    "pieces/SilverHorse.png",
    "pieces/SilverDog.png",
    "pieces/SilverCat.png",
    "pieces/SilverRabbit.png",
);

#[derive(Deserialize)]
struct Manifest {
    id: String,
    board: BoardSpec,
    pieces: PieceSpec,
}

#[derive(Deserialize)]
struct BoardSpec {
    src: String,
    grid: Grid,
}

#[derive(Deserialize)]
struct Grid {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Deserialize)]
struct PieceSpec {
    scale: Option<f64>,
    srcs: HashMap<String, String>,
}

/// An image theme, decoded.
pub struct Theme {
    board: RgbaImage,
    grid: Grid,
    scale: f64,
    pieces: HashMap<char, RgbaImage>,
}

/// The ids of the themes [`Theme::load`] knows, such as `classic-marble`.
pub fn theme_ids() -> Vec<String> {
    THEMES.iter().map(|(json, _)| manifest(json).id).collect()
}

fn manifest(json: &str) -> Manifest {
    serde_json::from_str(json).expect("embedded theme manifest is valid")
}

impl Theme {
    /// The theme with this id, or `None`.
    pub fn load(id: &str) -> Option<Theme> {
        let (json, files) = THEMES.iter().find(|(json, _)| manifest(json).id == id)?;
        let m = manifest(json);
        let file = |name: &str| {
            let (_, bytes) = files.iter().find(|(f, _)| *f == name).expect("theme file is embedded");
            image::load_from_memory(bytes).expect("embedded theme image decodes").to_rgba8()
        };
        let pieces = m
            .pieces
            .srcs
            .iter()
            .filter_map(|(letter, src)| Some((letter.chars().next()?, file(src))))
            .collect();
        Some(Theme {
            board: file(&m.board.src),
            grid: m.board.grid,
            scale: m.pieces.scale.unwrap_or(1.0),
            pieces,
        })
    }
}

/// Draws `position` with gold at the bottom (or silver, if `flip`),
/// `size` pixels square.
pub fn render(position: &Position, theme: &Theme, size: u32, flip: bool) -> RgbaImage {
    let mut img = theme.board.clone();
    let (cw, ch) = (theme.grid.width / 8.0, theme.grid.height / 8.0);
    let (pw, ph) = ((cw * theme.scale).round() as u32, (ch * theme.scale).round() as u32);
    for (sq, piece) in position.pieces() {
        let Some(src) = theme.pieces.get(&piece.letter()) else { continue };
        let p = resize(src, pw, ph, FilterType::Lanczos3);
        let (file, row) = screen_cell(sq, flip);
        let x = theme.grid.x + file as f64 * cw + (cw - pw as f64) / 2.0;
        let y = theme.grid.y + row as f64 * ch + (ch - ph as f64) / 2.0;
        overlay(&mut img, &p, x.round() as i64, y.round() as i64);
    }
    resize(&img, size, size, FilterType::Lanczos3)
}

/// Column and row from the top left of the square as drawn.
fn screen_cell(sq: Square, flip: bool) -> (u8, u8) {
    if flip { (7 - sq.file(), sq.rank()) } else { (sq.file(), 7 - sq.rank()) }
}

/// Saves `img`, as JPEG for a `.jpg`/`.jpeg` path (which has no alpha
/// channel) and otherwise by the extension.
pub fn save(img: RgbaImage, path: &str) -> image::ImageResult<()> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        DynamicImage::ImageRgba8(img).to_rgb8().save(path)
    } else {
        img.save(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_loads_with_all_pieces() {
        for id in theme_ids() {
            let t = Theme::load(&id).unwrap();
            assert_eq!(t.pieces.len(), 12, "{id}");
        }
    }

    #[test]
    fn draws_at_the_size_asked() {
        let pos = howdah_arimaa::default_start();
        let img = render(&pos, &Theme::load("classic-marble").unwrap(), 200, false);
        assert_eq!(img.dimensions(), (200, 200));
    }

    #[test]
    fn gold_is_at_the_bottom_unless_flipped() {
        let a1 = Square::from_coords(0, 0).unwrap();
        assert_eq!(screen_cell(a1, false), (0, 7));
        assert_eq!(screen_cell(a1, true), (7, 0));
    }
}
