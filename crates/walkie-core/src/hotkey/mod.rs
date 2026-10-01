pub mod engine;
pub mod keys;
pub mod machine;
pub mod tap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Start,
    Finish,
    CancelDiscard,
}
