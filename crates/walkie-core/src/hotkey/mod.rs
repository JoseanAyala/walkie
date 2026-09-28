pub mod engine;
pub mod keys;
pub mod machine;
#[cfg(target_os = "macos")]
pub mod tap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Start,
    Finish,
    CancelDiscard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dictate,
    Polish,
}
