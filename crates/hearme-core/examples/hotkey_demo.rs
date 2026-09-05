//! Manual smoke test for the global hotkey. Run and try:
//!   hold Right Option ≥150ms, release      → Start … Finish (Dictate)
//!   quick-tap Right Option                 → Start, CancelDiscard
//!   double-tap, wait, tap                  → Start (lock) … Finish
//!   hold Shift + Right Option              → Polish mode
//! Ctrl+C to exit. cargo run -p hearme-core --example hotkey_demo

use hearme_core::hotkey::listener::{parse_key, spawn_listener};

fn main() -> anyhow::Result<()> {
    let key = parse_key("RightAlt").unwrap();
    spawn_listener(key, true, |mode, out| println!("{mode:?} {out:?}"))?;
    println!("listening — Right Option is the hotkey (Ctrl+C to quit)");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
