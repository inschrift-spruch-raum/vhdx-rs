//! io module facade.

mod core;
mod platform;

pub use self::core::{IO, Sector};

#[cfg(test)]
mod tests;
