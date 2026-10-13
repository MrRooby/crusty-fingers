//! Typing speed (words per minute) measured from keyboard activity.

use std::env;
use std::io;
use std::path::PathBuf;

pub mod client;
pub mod daemon;
pub mod devices;
pub mod keys;
pub mod protocol;
pub mod server;
pub mod stats;
mod sys;

/// Where the daemon listens: `$XDG_RUNTIME_DIR/rusty-word-count.sock`.
pub fn socket_path() -> io::Result<PathBuf> {
    let dir = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR is not set"))?;
    Ok(PathBuf::from(dir).join("rusty-word-count.sock"))
}
