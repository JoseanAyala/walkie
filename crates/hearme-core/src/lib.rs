//! hearme-core: all dictation logic. No UI imports allowed in this crate.

pub mod config;
pub mod pipeline;

#[cfg(test)]
mod smoke {
    #[test]
    fn workspace_builds() {
        assert_eq!(2 + 2, 4);
    }
}
