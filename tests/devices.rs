//! Runs keyboard discovery against a fake `/dev/input` and `/sys/class/input` made of plain
//! files.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use rusty_word_count::devices::Keyboards;
use rusty_word_count::keys::EVENT_SIZE;
use rusty_word_count::stats::Key;

/// Key capabilities as sysfs prints them.
const KEYBOARD: &str = "402000007 ff803078f800d001 feffffdfffcfffff fffffffffffffffe\n";
const MOUSE: &str = "ff0000 0 0 0 0\n";

/// A directory standing in for `/`.
struct Root(PathBuf);

impl Root {
    fn new(test: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("dev/input")).unwrap();
        Self(root)
    }

    /// Plugs in a device and returns its node, to which events can be written.
    fn plug(&self, name: &str, capabilities: &str) -> File {
        let sys = self.0.join("sys/class/input").join(name).join("device/capabilities");
        fs::create_dir_all(&sys).unwrap();
        fs::write(sys.join("key"), capabilities).unwrap();
        File::create(self.0.join("dev/input").join(name)).unwrap()
    }
}

/// Writes the press of the A key to a device node.
fn press(node: &mut File) {
    const EV_KEY: u16 = 1;
    const KEY_A: u16 = 30;
    const PRESS: i32 = 1;

    let mut event = [0; EVENT_SIZE];
    let fields = &mut event[EVENT_SIZE - 8..];
    fields[..2].copy_from_slice(&EV_KEY.to_ne_bytes());
    fields[2..4].copy_from_slice(&KEY_A.to_ne_bytes());
    fields[4..].copy_from_slice(&PRESS.to_ne_bytes());
    node.write_all(&event).unwrap();
}

fn read(keyboards: &mut Keyboards) -> Vec<Key> {
    let mut keys = Vec::new();
    keyboards.read(|key| keys.push(key)).unwrap();
    keys
}

#[test]
fn reads_only_from_keyboards() {
    let root = Root::new("reads_only_from_keyboards");
    let mut keyboard = root.plug("event0", KEYBOARD);
    press(&mut keyboard);
    press(&mut keyboard);
    press(&mut root.plug("event1", MOUSE));
    // Another node of a keyboard, which does not speak the evdev protocol.
    press(&mut root.plug("mouse0", KEYBOARD));

    let mut keyboards = Keyboards::open_in(&root.0).unwrap();
    assert_eq!(read(&mut keyboards), [Key::Char; 2]);
    assert_eq!(read(&mut keyboards), []);

    press(&mut keyboard);
    assert_eq!(read(&mut keyboards), [Key::Char]);
}

#[test]
fn picks_up_a_keyboard_plugged_in_later() {
    let root = Root::new("picks_up_a_keyboard_plugged_in_later");
    let mut keyboards = Keyboards::open_in(&root.0).unwrap();
    assert!(keyboards.is_empty());

    press(&mut root.plug("event0", KEYBOARD));
    // The first read notices the new node, the second one reads from it.
    assert_eq!(read(&mut keyboards), []);
    assert_eq!(read(&mut keyboards), [Key::Char]);
}

#[test]
fn fails_without_an_input_directory() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no_such_root");

    assert!(Keyboards::open_in(&root).is_err());
}
