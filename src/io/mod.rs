//! io module facade.

mod core;
pub(crate) mod platform;

pub use self::core::{IO, Sector};

#[cfg(test)]
mod tests;
