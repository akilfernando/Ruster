//! Shared workspace utilities: strict JSON parsing and local editor IPC.

pub mod strict_json;

#[cfg(all(unix, feature = "ipc"))]
pub mod connection;
#[cfg(all(unix, feature = "ipc"))]
pub mod discovery;
#[cfg(all(unix, feature = "ipc"))]
pub mod trust;
