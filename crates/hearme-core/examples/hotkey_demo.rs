//! Manual smoke test for the keyboard hook with the default (Wispr-style)
//! bindings. Run and try:
//!   hold Fn ≥150ms, release            → Start(Dictate) … Finish
//!   quick-tap Fn                       → Start, Cancel
//!   double-tap Fn, wait, tap           → Start (locked) … Finish
//!   hold Fn, add Shift                 → Start(Dictate), SetMode(Polish)
//!   hold Fn, tap Space, release, Fn    → hands-free: Start … Finish
//! Needs Accessibility for the process running it (your terminal).
//! cargo run -p hearme-core --example hotkey_demo

use hearme_core::config::Hotkeys;
use hearme_core::hotkey::engine::{Bindings, Engine};
use hearme_core::hotkey::keys::parse_binding;
use hearme_core::hotkey::tap::{self, TapStatus};
use std::sync::{Arc, Mutex};

fn main() -> anyhow::Result<()> {
    let h = Hotkeys::default();
    let parse = |v: &Vec<String>| parse_binding(v).map_err(anyhow::Error::msg);
    let bindings = Bindings {
        dictate: parse(&h.dictate)?,
        polish: parse(&h.polish)?,
        hands_free: parse(&h.hands_free)?,
        paste_last: parse(&h.paste_last)?,
    };
    let status = Arc::new(TapStatus::default());
    tap::spawn(Arc::new(Mutex::new(Engine::new(bindings))), status.clone(), |s| println!("{s:?}"));
    std::thread::sleep(std::time::Duration::from_millis(500));
    if let Some(e) = status.error.lock().unwrap().as_ref() {
        anyhow::bail!("{e}");
    }
    println!("listening — Fn is the hotkey (Ctrl+C to quit)");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
