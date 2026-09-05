pub mod machine;
pub mod router;

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
