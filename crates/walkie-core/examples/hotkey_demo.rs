//! Manual smoke test for the keyboard hook with the default (Wispr-style)
//! bindings. Run and try:
//!   hold Fn ≥150ms, release            → Start … Finish
//!   quick-tap Fn                       → Start, Cancel
//!   double-tap Fn, wait, tap           → Start (locked) … Finish
//!   Shift+Fn, release                  → Polish
//! Needs Accessibility for the process running it (your terminal).
//! cargo run -p walkie-core --example hotkey_demo

use std::sync::{Arc, Mutex};
use walkie_core::config::Hotkeys;
use walkie_core::hotkey::engine::{Bindings, Engine};
use walkie_core::hotkey::keys::parse_binding;
use walkie_core::hotkey::tap::{self, TapStatus};

fn main() -> anyhow::Result<()> {
    let h = Hotkeys::default();
    let parse = |v: &Vec<String>| parse_binding(v).map_err(anyhow::Error::msg);
    let bindings = Bindings {
        dictate: parse(&h.dictate)?,
        polish: parse(&h.polish)?,
        paste_last: parse(&h.paste_last)?,
    };
    let status = Arc::new(TapStatus::default());
    tap::spawn(
        Arc::new(Mutex::new(Engine::new(bindings))),
        status.clone(),
        |s| println!("{s:?}"),
    );
    std::thread::sleep(std::time::Duration::from_millis(500));
    if let Some(e) = status.error.lock().unwrap().as_ref() {
        anyhow::bail!("{e}");
    }
    println!("listening — Fn is the hotkey (Ctrl+C to quit)");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
