//! io module facade.

mod core;
mod write;

pub use self::core::{IO, Sector};

#[cfg(test)]
mod tests;
