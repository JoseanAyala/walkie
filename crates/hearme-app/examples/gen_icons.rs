//! Generates the app + tray icons (flat circles). Deterministic, no external
//! tools. Run once: cargo run -p hearme-app --example gen_icons

fn circle(size: u32, rgba: [u8; 4]) -> image::RgbaImage {
    let c = size as f32 / 2.0;
    let r = size as f32 * 0.42;
    image::RgbaImage::from_fn(size, size, |x, y| {
        let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
        if d <= r {
            image::Rgba(rgba)
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    })
}

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons");
    std::fs::create_dir_all(&dir).unwrap();
    circle(512, [74, 144, 217, 255]).save(dir.join("icon.png")).unwrap(); // blue
    circle(32, [140, 140, 140, 255]).save(dir.join("tray-idle.png")).unwrap(); // gray
    circle(32, [231, 76, 60, 255]).save(dir.join("tray-rec.png")).unwrap(); // red
    circle(32, [243, 156, 18, 255]).save(dir.join("tray-busy.png")).unwrap(); // amber
    println!("icons written to {dir:?}");
}
