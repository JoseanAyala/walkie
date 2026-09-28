//! Generates the app + tray icons: a pixel-art mic in the UI's theme (pink
//! dithered desk, near-black ink, paper white; see ui/src/lib/theme.css).
//! Deterministic, no external tools. Run via `make icons`, which then feeds
//! icons/icon.png to `cargo tauri icon` for the bundle sizes.

use image::{Rgba, RgbaImage};

const DESK: [u8; 4] = [0xe8, 0x89, 0x9a, 0xff];
const INK: [u8; 4] = [0x1c, 0x1a, 0x1b, 0xff];
const ACCENT: [u8; 4] = [0xd4, 0x58, 0x6f, 0xff];
const PAPER: [u8; 4] = [0xf3, 0xf3, 0xf3, 0xff];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// The mic on a 16×16 grid: `#` ink, `o` paper, `g` grille (dithered), `.` empty.
/// Shared by the tray (1 cell = 2px) and the app icon (1 cell = 40px).
const MIC: [&str; 16] = [
    "................",
    "......####......",
    ".....#gggg#.....",
    ".....#gggg#.....",
    ".....#gggg#.....",
    ".....#gggg#.....",
    "...#.#oooo#.#...",
    "...#.#oooo#.#...",
    "...#.#oooo#.#...",
    "...#..####..#...",
    "....#......#....",
    ".....######.....",
    ".......##.......",
    ".......##.......",
    ".....######.....",
    "................",
];

fn cell(x: usize, y: usize) -> u8 {
    MIC[y].as_bytes()[x]
}

/// Tray icon: the mic's silhouette in one color. `dither` checkers the
/// capsule's inside (the "working" state).
fn tray(rgba: [u8; 4], dither: bool) -> RgbaImage {
    RgbaImage::from_fn(32, 32, |x, y| {
        let (cx, cy) = (x as usize / 2, y as usize / 2);
        let on = match cell(cx, cy) {
            b'#' => true,
            b'o' | b'g' => !dither || (cx + cy).is_multiple_of(2),
            _ => false,
        };
        Rgba(if on { rgba } else { CLEAR })
    })
}

/// mulberry32: a tiny seeded PRNG, so the icon is the same every run.
fn rng(seed: u32) -> impl FnMut() -> f32 {
    let mut a = seed;
    move || {
        a = a.wrapping_add(0x6d2b_79f5);
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as f32) / 4_294_967_296.0
    }
}

/// App icon: 1024², macOS layout (824² rounded tile, 100px margin), a
/// dithered pink desk with an ink border and the mic with a hard shadow.
fn app() -> RgbaImage {
    const S: f32 = 1024.0;
    const M: f32 = 100.0; // tile margin
    const R: f32 = 185.0; // corner radius
    const BORDER: f32 = 18.0;
    const U: usize = 40; // px per mic cell
    let ox = (1024 - 16 * U) / 2;
    let oy = (1024 - 16 * U) / 2 - U / 4;
    let ox = ox - U / 4; // room for the shadow

    // Signed distance to the tile's rounded rect (negative inside).
    let sdf = |x: f32, y: f32| {
        let h = S / 2.0 - M - R;
        let dx = ((x - S / 2.0).abs() - h).max(0.0);
        let dy = ((y - S / 2.0).abs() - h).max(0.0);
        let inner = ((x - S / 2.0).abs() - h)
            .max((y - S / 2.0).abs() - h)
            .min(0.0);
        (dx * dx + dy * dy).sqrt() + inner - R
    };

    // Dither clouds: [x, y, radius] as fractions of the tile.
    let clouds = [(0.12, 0.88, 0.55), (0.95, 0.1, 0.35)];
    let mut r = rng(7);
    let mut dots = std::collections::HashSet::new();
    for gy in (0..1024).step_by(18) {
        for gx in (0..1024).step_by(18) {
            let (x, y) = (gx as f32 / S, gy as f32 / S);
            let mut p: f32 = 0.0;
            for (cx, cy, cr) in clouds {
                let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / cr;
                if d < 1.0 {
                    p = p.max((1.0 - d).powf(1.6));
                }
            }
            if p > 0.0 && r() < p * 0.75 {
                dots.insert((gx / 18, gy / 18));
            }
        }
    }

    let mic_at = |x: i64, y: i64| -> u8 {
        let (mx, my) = (x - ox as i64, y - oy as i64);
        if mx < 0 || my < 0 {
            return b'.';
        }
        let (cx, cy) = (mx as usize / U, my as usize / U);
        if cx < 16 && cy < 16 {
            cell(cx, cy)
        } else {
            b'.'
        }
    };

    RgbaImage::from_fn(1024, 1024, |x, y| {
        // 4× supersampled tile edge; everything inside is on the pixel grid.
        let mut cover: f32 = 0.0;
        for sy in 0..4 {
            for sx in 0..4 {
                let d = sdf(
                    x as f32 + (sx as f32 + 0.5) / 4.0,
                    y as f32 + (sy as f32 + 0.5) / 4.0,
                );
                if d <= 0.0 {
                    cover += 1.0 / 16.0;
                }
            }
        }
        if cover == 0.0 {
            return Rgba(CLEAR);
        }
        let d = sdf(x as f32 + 0.5, y as f32 + 0.5);
        let (xi, yi) = (x as i64, y as i64);
        let mut c = if d > -BORDER {
            INK
        } else {
            match mic_at(xi, yi) {
                b'#' => INK,
                b'o' => PAPER,
                b'g' if (x as usize / (U / 4) + y as usize / (U / 4)).is_multiple_of(2) => INK,
                b'g' => PAPER,
                _ if mic_at(xi - U as i64 / 2, yi - U as i64 / 2) != b'.' => blend(DESK, INK, 0.45), // shadow
                _ => {
                    let (gx, gy) = (x / 18, y / 18);
                    let (lx, ly) = (x % 18, y % 18);
                    if dots.contains(&(gx, gy)) && lx < 9 && ly < 9 {
                        INK
                    } else if x % 27 < 5 && y % 27 < 5 {
                        blend(DESK, INK, 0.28) // the faint dot grid
                    } else {
                        DESK
                    }
                }
            }
        };
        c[3] = (cover * 255.0).round() as u8;
        Rgba(c)
    })
}

fn blend(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let m = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    [m(0), m(1), m(2), 0xff]
}

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons");
    std::fs::create_dir_all(&dir).unwrap();
    app().save(dir.join("icon.png")).unwrap();
    // Idle is a template image (macOS tints it for the menu bar); only the
    // alpha matters, so ink is fine. Recording/busy use the accent pink, legible on light and dark bars.
    tray(INK, false).save(dir.join("tray-idle.png")).unwrap();
    tray(ACCENT, false).save(dir.join("tray-rec.png")).unwrap();
    tray(ACCENT, true).save(dir.join("tray-busy.png")).unwrap();
    println!("icons written to {dir:?}");
}
