//! Shared statusline segment model, renderer, and data types.
//!
//! Intentionally free of network/keychain/sqlite deps so the configure editor
//! (and clankerbox) can render previews without them. The `statusline` binary
//! layers fetching/keychain/accounts on top of this crate.

pub mod browser;
pub mod catalog;
pub mod constants;
pub mod context_window;
pub mod format;
pub mod input;
pub mod sample;
pub mod segment;
pub mod settings;
pub mod usage;
pub mod usage_bridge;
pub mod util;
