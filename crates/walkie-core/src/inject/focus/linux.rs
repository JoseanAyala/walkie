use super::Probe;

// TODO(linux): query focus through AT-SPI (the accessibility bus most
// desktop toolkits — GTK, Qt — answer on), the way `macos` does through AX.
pub fn probe() -> Probe {
    Probe::Failed
}

// TODO(linux): no portable "frontmost app + window title" query without a
// compositor-specific protocol (e.g. wlr-foreign-toplevel on wlroots); X11
// has one (_NET_ACTIVE_WINDOW) but Wayland generally doesn't expose it.
pub fn frontmost() -> Option<(String, String)> {
    None
}

// TODO(linux): AT-SPI's Text interface exposes a selection, where the
// focused app supports it.
pub fn selected_text() -> Option<String> {
    None
}
