use super::router::{Router, RouterKey};
use super::{Mode, Output};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub fn parse_key(name: &str) -> Option<rdev::Key> {
    use rdev::Key::*;
    Some(match name {
        "RightAlt" => AltGr,
        "LeftAlt" => Alt,
        "RightCmd" => MetaRight,
        "LeftCmd" => MetaLeft,
        "RightCtrl" => ControlRight,
        "LeftCtrl" => ControlLeft,
        "CapsLock" => CapsLock,
        "F1" => F1, "F2" => F2, "F3" => F3, "F4" => F4, "F5" => F5, "F6" => F6,
        "F7" => F7, "F8" => F8, "F9" => F9, "F10" => F10, "F11" => F11, "F12" => F12,
        _ => return None,
    })
}

/// Starts the global key listener. On macOS this requires the Input
/// Monitoring permission (granted to your terminal during dev); without it
/// rdev::listen errors and we log loudly instead of crashing the app.
pub fn spawn_listener(
    hot: rdev::Key,
    use_shift_modifier: bool,
    on_output: impl Fn(Mode, Output) + Send + 'static,
) -> Result<()> {
    let router = Arc::new(Mutex::new(Router::new()));
    let start = Instant::now();
    // Set HEARME_DEBUG_EVENTS=1 to dump every tap event. Useful for the one
    // failure this can't detect on its own: without Input Monitoring, macOS
    // hands the tap mouse events but silently withholds key events, so
    // `rdev::listen` succeeds and the hotkey simply never fires.
    let debug_events = std::env::var_os("HEARME_DEBUG_EVENTS").is_some();

    {
        let router = router.clone();
        std::thread::spawn(move || {
            let result = rdev::listen(move |ev| {
                if debug_events {
                    eprintln!("hearme: raw event {:?}", ev.event_type);
                }
                let (key, down) = match ev.event_type {
                    rdev::EventType::KeyPress(k) => (k, true),
                    rdev::EventType::KeyRelease(k) => (k, false),
                    _ => return,
                };
                let rk = if key == hot {
                    RouterKey::Hot
                } else if use_shift_modifier
                    && (key == rdev::Key::ShiftLeft || key == rdev::Key::ShiftRight)
                {
                    RouterKey::Modifier
                } else {
                    RouterKey::Other
                };
                if rk == RouterKey::Other {
                    return;
                }
                let t = start.elapsed().as_millis();
                // Extract the result before calling `on_output`: an `if let` on the
                // lock expression directly would hold the MutexGuard for the whole
                // arm (it's a temporary in the scrutinee, dropped only at arm end),
                // stalling the poll thread's 50ms tap-window-expiry cadence for as
                // long as the callback takes to run.
                let routed = router.lock().unwrap_or_else(|e| e.into_inner()).handle(rk, down, t);
                if let Some((mode, out)) = routed {
                    on_output(mode, out);
                }
            });
            if let Err(e) = result {
                eprintln!(
                    "hearme: hotkey listener failed: {e:?} — grant Input Monitoring and restart"
                );
            }
        });
    }

    {
        let router = router.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(50));
            router
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .poll(start.elapsed().as_millis());
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_key;

    #[test]
    fn known_keys_parse() {
        assert_eq!(parse_key("RightAlt"), Some(rdev::Key::AltGr));
        assert_eq!(parse_key("LeftCmd"), Some(rdev::Key::MetaLeft));
        assert_eq!(parse_key("F5"), Some(rdev::Key::F5));
        assert_eq!(parse_key("CapsLock"), Some(rdev::Key::CapsLock));
    }

    #[test]
    fn unknown_keys_dont_parse() {
        assert_eq!(parse_key("Fn"), None);
        assert_eq!(parse_key(""), None);
    }
}
