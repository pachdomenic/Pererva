//! Відтворення звуків: шкільний дзвінок, сирена тривоги, відбій,
//! хвилина мовчання і гімн.

pub mod player;
pub mod synth;

pub use player::{Player, Sound};
