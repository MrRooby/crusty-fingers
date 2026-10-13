//! A fake `/dev/input` and `/sys/class/input`, with FIFOs standing in for device nodes.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use rusty_word_count::keys::EVENT_SIZE;

/// Key capabilities as sysfs prints them.
pub const KEYBOARD: &str = "402000007 ff803078f800d001 feffffdfffcfffff fffffffffffffffe\n";
#[allow(dead_code)]
pub const MOUSE: &str = "ff0000 0 0 0 0\n";

/// A directory standing in for `/`.
pub struct Root(pub PathBuf);

impl Root {
    /// An empty root for the test called `test`.
    pub fn new(test: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("dev/input")).unwrap();
        Self(root)
    }

    /// Plugs in a device and returns its node, to which events can be written.
    pub fn plug(&self, name: &str, capabilities: &str) -> File {
        let sys = self.0.join("sys/class/input").join(name).join("device/capabilities");
        fs::create_dir_all(&sys).unwrap();
        fs::write(sys.join("key"), capabilities).unwrap();

        let node = self.0.join("dev/input").join(name);
        assert!(Command::new("mkfifo").arg(&node).status().unwrap().success());
        // Opened for reading as well, so that the FIFO always has a reader and a writer.
        File::options().read(true).write(true).open(node).unwrap()
    }
}

/// Writes `count` presses of the A key to a device node.
pub fn press(node: &mut File, count: usize) {
    const EV_KEY: u16 = 1;
    const KEY_A: u16 = 30;
    const PRESS: i32 = 1;

    let mut event = [0; EVENT_SIZE];
    let fields = &mut event[EVENT_SIZE - 8..];
    fields[..2].copy_from_slice(&EV_KEY.to_ne_bytes());
    fields[2..4].copy_from_slice(&KEY_A.to_ne_bytes());
    fields[4..].copy_from_slice(&PRESS.to_ne_bytes());
    node.write_all(&event.repeat(count)).unwrap();
}
