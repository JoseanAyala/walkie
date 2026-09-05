//! hearme-core: all dictation logic. No UI imports allowed in this crate.

pub mod audio;
pub mod config;
pub mod history;
pub mod hotkey;
pub mod inject;
pub mod pipeline;
pub mod stt;

#[cfg(test)]
mod smoke {
    #[test]
    fn workspace_builds() {
        assert_eq!(2 + 2, 4);
    }
}
