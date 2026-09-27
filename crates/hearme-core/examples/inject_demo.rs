//! Manual smoke test: focuses whatever you click within 3 seconds, then
//! injects a phrase with Spanish accents. Run, then click into TextEdit/Notes.
//! cargo run -p hearme-core --example inject_demo [type]

use hearme_core::inject::{Injector, PasteInjector, TypeInjector};

fn main() -> anyhow::Result<()> {
    let use_type = std::env::args().nth(1).as_deref() == Some("type");
    println!(
        "click into a text field… injecting in 3s ({})",
        if use_type { "type" } else { "paste" }
    );
    std::thread::sleep(std::time::Duration::from_secs(3));
    let text = "Hola, hearme funciona — ¡qué rápido! ✓";
    if use_type {
        TypeInjector { main: None }.inject(text)?;
    } else {
        PasteInjector {
            restore_ms: 150,
            main: None,
        }
        .inject(text)?;
    }
    println!("done");
    Ok(())
}
