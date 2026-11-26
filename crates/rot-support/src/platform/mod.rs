//! Platform-specific directories and optional local IPC primitives.

pub mod dirs;
#[cfg(feature = "ipc")]
pub mod ipc;
