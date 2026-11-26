//! Shared workspace utilities: strict JSON parsing and local editor IPC.

pub mod platform;
pub mod strict_json;

#[cfg(feature = "ipc")]
pub mod connection;
#[cfg(feature = "ipc")]
pub mod discovery;
#[cfg(feature = "ipc")]
pub mod trust;
