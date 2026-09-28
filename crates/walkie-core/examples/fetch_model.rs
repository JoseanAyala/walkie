//! Downloads a model into ~/.cache/walkie/models.
//! Run: cargo run -p walkie-core --example fetch_model -- base

use walkie_core::config::models;

fn main() -> anyhow::Result<()> {
    let key = std::env::args().nth(1).unwrap_or_else(|| "base".into());
    if models::is_downloaded(&key) {
        println!(
            "{key}: already downloaded at {:?}",
            models::model_path(&key).unwrap()
        );
        return Ok(());
    }
    println!("downloading {key}…");
    let path = models::download(&key, &mut |done, total| {
        print!(
            "\r{:>3}% ({} / {} MB)",
            done * 100 / total.max(1),
            done / 1_048_576,
            total / 1_048_576
        );
    })?;
    println!("\ndone: {path:?}");
    Ok(())
}
