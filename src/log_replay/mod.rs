//! `log_replay` module facade.

mod core;

pub(crate) use self::core::{
    ReplayOverlay, build_replay_overlay, detect_active_sequence, has_pending_log, replay_to_file,
};

#[cfg(test)]
mod tests;
