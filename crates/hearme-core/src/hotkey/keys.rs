//! Physical keys, and the names config.toml and the settings UI use for them.
//!
//! Modifiers carry a side. A physical event always says `Left` or `Right`; a
//! binding may say `Any` (plain `"Shift"`), which matches either.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Fn,
    Cmd(Side),
    Opt(Side),
    Ctrl(Side),
    Shift(Side),
    /// Every other key, by macOS virtual keycode.
    Code(u16),
}

pub const ESCAPE: Key = Key::Code(53);
const CAPS_LOCK: u16 = 57;

/// macOS virtual keycodes (kVK_*) for keys with a stable name. Anything not
/// listed is still bindable and round-trips as `Key<code>`.
const NAMED: &[(u16, &str)] = &[
    (0, "A"), (11, "B"), (8, "C"), (2, "D"), (14, "E"), (3, "F"), (5, "G"), (4, "H"),
    (34, "I"), (38, "J"), (40, "K"), (37, "L"), (46, "M"), (45, "N"), (31, "O"),
    (35, "P"), (12, "Q"), (15, "R"), (1, "S"), (17, "T"), (32, "U"), (9, "V"),
    (13, "W"), (7, "X"), (16, "Y"), (6, "Z"),
    (29, "0"), (18, "1"), (19, "2"), (20, "3"), (21, "4"), (23, "5"), (22, "6"),
    (26, "7"), (28, "8"), (25, "9"),
    (49, "Space"), (36, "Return"), (48, "Tab"), (51, "Delete"), (53, "Escape"),
    (50, "Backquote"), (27, "Minus"), (24, "Equal"), (33, "LeftBracket"),
    (30, "RightBracket"), (42, "Backslash"), (41, "Semicolon"), (39, "Quote"),
    (43, "Comma"), (47, "Period"), (44, "Slash"),
    (123, "Left"), (124, "Right"), (125, "Down"), (126, "Up"),
    (115, "Home"), (119, "End"), (116, "PageUp"), (121, "PageDown"),
    (117, "ForwardDelete"), (114, "Help"),
    (122, "F1"), (120, "F2"), (99, "F3"), (118, "F4"), (96, "F5"), (97, "F6"),
    (98, "F7"), (100, "F8"), (101, "F9"), (109, "F10"), (103, "F11"), (111, "F12"),
    (105, "F13"), (107, "F14"), (113, "F15"), (106, "F16"), (64, "F17"), (79, "F18"),
    (80, "F19"), (90, "F20"),
];

/// Keys that may be bound on their own. Anything else alone (a letter, Space,
/// Return, an arrow) would stop working for normal typing, since a bound key
/// is swallowed.
const LONE_OK: &[u16] = &[122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90, 114];

impl Key {
    pub fn from_keycode(code: u16) -> Key {
        use Side::*;
        match code {
            63 => Key::Fn,
            55 => Key::Cmd(Left),
            54 => Key::Cmd(Right),
            58 => Key::Opt(Left),
            61 => Key::Opt(Right),
            59 => Key::Ctrl(Left),
            62 => Key::Ctrl(Right),
            56 => Key::Shift(Left),
            60 => Key::Shift(Right),
            c => Key::Code(c),
        }
    }

    pub fn is_modifier(self) -> bool {
        !matches!(self, Key::Code(_))
    }

    /// The device-dependent bit in a CGEvent's flags that is set while this
    /// physical modifier is down. Reading these is exact, unlike comparing
    /// device-independent flags (which merge left and right).
    pub fn flag_bit(self) -> Option<u64> {
        use Side::*;
        Some(match self {
            Key::Ctrl(Left) => 0x0000_0001,
            Key::Shift(Left) => 0x0000_0002,
            Key::Shift(Right) => 0x0000_0004,
            Key::Cmd(Left) => 0x0000_0008,
            Key::Cmd(Right) => 0x0000_0010,
            Key::Opt(Left) => 0x0000_0020,
            Key::Opt(Right) => 0x0000_0040,
            Key::Ctrl(Right) => 0x0000_2000,
            Key::Fn => 0x0080_0000,
            _ => return None,
        })
    }

    /// Does this (binding) key match that physical key?
    pub fn matches(self, physical: Key) -> bool {
        fn side(a: Side, b: Side) -> bool {
            a == Side::Any || b == Side::Any || a == b
        }
        match (self, physical) {
            (Key::Cmd(a), Key::Cmd(b))
            | (Key::Opt(a), Key::Opt(b))
            | (Key::Ctrl(a), Key::Ctrl(b))
            | (Key::Shift(a), Key::Shift(b)) => side(a, b),
            (a, b) => a == b,
        }
    }

    /// The same modifier, either side.
    pub fn any_side(self) -> Key {
        match self {
            Key::Cmd(_) => Key::Cmd(Side::Any),
            Key::Opt(_) => Key::Opt(Side::Any),
            Key::Ctrl(_) => Key::Ctrl(Side::Any),
            Key::Shift(_) => Key::Shift(Side::Any),
            k => k,
        }
    }

    fn sort_rank(self) -> (u8, u16) {
        match self {
            Key::Fn => (0, 0),
            Key::Ctrl(_) => (1, 0),
            Key::Opt(_) => (2, 0),
            Key::Shift(_) => (3, 0),
            Key::Cmd(_) => (4, 0),
            Key::Code(c) => (5, c),
        }
    }

    pub fn name(self) -> String {
        let m = |base: &str, s: Side| match s {
            Side::Any => base.to_string(),
            Side::Left => format!("Left{base}"),
            Side::Right => format!("Right{base}"),
        };
        match self {
            Key::Fn => "Fn".into(),
            Key::Cmd(s) => m("Cmd", s),
            Key::Opt(s) => m("Opt", s),
            Key::Ctrl(s) => m("Ctrl", s),
            Key::Shift(s) => m("Shift", s),
            Key::Code(c) => NAMED
                .iter()
                .find(|(k, _)| *k == c)
                .map(|(_, n)| n.to_string())
                .unwrap_or_else(|| format!("Key{c}")),
        }
    }

    pub fn parse(name: &str) -> Option<Key> {
        let lower = name.trim().to_ascii_lowercase();
        let (side, base) = if let Some(b) = lower.strip_prefix("left") {
            (Side::Left, b)
        } else if let Some(b) = lower.strip_prefix("right") {
            (Side::Right, b)
        } else {
            (Side::Any, lower.as_str())
        };
        let modifier = match base {
            "cmd" | "command" | "meta" => Some(Key::Cmd(side)),
            "opt" | "option" | "alt" => Some(Key::Opt(side)),
            "ctrl" | "control" => Some(Key::Ctrl(side)),
            "shift" => Some(Key::Shift(side)),
            _ => None,
        };
        if modifier.is_some() {
            return modifier;
        }
        if lower == "fn" || lower == "globe" {
            return Some(Key::Fn);
        }
        if let Some(code) = lower.strip_prefix("key").and_then(|c| c.parse::<u16>().ok()) {
            return Some(Key::Code(code));
        }
        NAMED
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(&lower))
            .map(|(c, _)| Key::Code(*c))
    }
}

/// Turns the keys held during a recording into a binding. Modifiers become
/// side-agnostic when the binding also has Fn or a regular key (so `Ctrl+Opt+D`
/// works with either Ctrl), but stay sided in a modifiers-only binding (so
/// binding Right Cmd leaves Left Cmd free for normal shortcuts).
pub fn normalize(keys: &[Key]) -> Vec<Key> {
    let generic = keys.iter().any(|k| matches!(k, Key::Fn | Key::Code(_)));
    let mut out: Vec<Key> = Vec::new();
    for &k in keys {
        let k = if generic { k.any_side() } else { k };
        if !out.contains(&k) {
            out.push(k);
        }
    }
    out.sort_by_key(|k| k.sort_rank());
    out
}

/// Why a binding can't be used, if it can't.
pub fn validate(keys: &[Key]) -> Result<(), String> {
    if keys.is_empty() {
        return Ok(()); // empty = action disabled
    }
    if keys.len() > 3 {
        return Err("use at most 3 keys".into());
    }
    if keys.contains(&Key::Code(CAPS_LOCK)) {
        return Err("Caps Lock can't be held reliably on macOS — pick another key".into());
    }
    if let [Key::Code(c)] = keys {
        if !LONE_OK.contains(c) {
            return Err(format!(
                "{} on its own would stop typing it — add a modifier (e.g. Fn or Ctrl)",
                Key::Code(*c).name()
            ));
        }
    }
    Ok(())
}

pub fn parse_binding(names: &[String]) -> Result<Vec<Key>, String> {
    let keys = names
        .iter()
        .map(|n| Key::parse(n).ok_or_else(|| format!("unknown key \"{n}\"")))
        .collect::<Result<Vec<_>, _>>()?;
    validate(&keys)?;
    Ok(keys)
}

pub fn binding_names(keys: &[Key]) -> Vec<String> {
    keys.iter().map(|k| k.name()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use Side::*;

    #[test]
    fn names_round_trip() {
        for k in [
            Key::Fn,
            Key::Cmd(Right),
            Key::Opt(Any),
            Key::Ctrl(Left),
            Key::Shift(Any),
            Key::Code(49),
            Key::Code(0),
            Key::Code(105),
            Key::Code(200),
        ] {
            assert_eq!(Key::parse(&k.name()), Some(k), "{}", k.name());
        }
    }

    #[test]
    fn legacy_and_alias_names_parse() {
        assert_eq!(Key::parse("RightAlt"), Some(Key::Opt(Right)));
        assert_eq!(Key::parse("LeftAlt"), Some(Key::Opt(Left)));
        assert_eq!(Key::parse("RightCmd"), Some(Key::Cmd(Right)));
        assert_eq!(Key::parse("globe"), Some(Key::Fn));
        assert_eq!(Key::parse("space"), Some(Key::Code(49)));
        assert_eq!(Key::parse("nope"), None);
    }

    #[test]
    fn keycodes_map_to_sided_modifiers() {
        assert_eq!(Key::from_keycode(63), Key::Fn);
        assert_eq!(Key::from_keycode(54), Key::Cmd(Right));
        assert_eq!(Key::from_keycode(55), Key::Cmd(Left));
        assert_eq!(Key::from_keycode(61), Key::Opt(Right));
        assert_eq!(Key::from_keycode(60), Key::Shift(Right));
        assert_eq!(Key::from_keycode(49), Key::Code(49));
    }

    #[test]
    fn every_physical_modifier_has_a_distinct_flag_bit() {
        let mods: Vec<Key> = [63u16, 54, 55, 58, 61, 59, 62, 56, 60]
            .iter()
            .map(|c| Key::from_keycode(*c))
            .collect();
        let mut bits: Vec<u64> = mods.iter().map(|k| k.flag_bit().unwrap()).collect();
        bits.sort();
        bits.dedup();
        assert_eq!(bits.len(), mods.len());
        assert_eq!(Key::Code(0).flag_bit(), None);
    }

    #[test]
    fn any_side_matches_both_sides_but_sided_does_not() {
        assert!(Key::Shift(Any).matches(Key::Shift(Left)));
        assert!(Key::Shift(Any).matches(Key::Shift(Right)));
        assert!(Key::Cmd(Right).matches(Key::Cmd(Right)));
        assert!(!Key::Cmd(Right).matches(Key::Cmd(Left)));
        assert!(!Key::Cmd(Any).matches(Key::Opt(Left)));
        assert!(Key::Code(49).matches(Key::Code(49)));
    }

    #[test]
    fn normalize_generalizes_only_combos_with_fn_or_a_regular_key() {
        assert_eq!(normalize(&[Key::Shift(Left), Key::Fn]), vec![Key::Fn, Key::Shift(Any)]);
        assert_eq!(
            normalize(&[Key::Code(2), Key::Opt(Right), Key::Ctrl(Left)]),
            vec![Key::Ctrl(Any), Key::Opt(Any), Key::Code(2)]
        );
        assert_eq!(normalize(&[Key::Cmd(Right)]), vec![Key::Cmd(Right)]);
        assert_eq!(
            normalize(&[Key::Opt(Right), Key::Cmd(Right)]),
            vec![Key::Opt(Right), Key::Cmd(Right)]
        );
    }

    #[test]
    fn validate_rejects_unusable_bindings() {
        assert!(validate(&[]).is_ok());
        assert!(validate(&[Key::Fn]).is_ok());
        assert!(validate(&[Key::Code(105)]).is_ok()); // F13 alone
        assert!(validate(&[Key::Ctrl(Any), Key::Opt(Any), Key::Code(2)]).is_ok());
        assert!(validate(&[Key::Code(0)]).is_err()); // A alone
        assert!(validate(&[Key::Code(49)]).is_err()); // Space alone
        assert!(validate(&[Key::Code(57)]).is_err()); // Caps Lock
        assert!(validate(&[Key::Fn, Key::Ctrl(Any), Key::Opt(Any), Key::Code(2)]).is_err());
    }
}
