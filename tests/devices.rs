//! Runs keyboard discovery against a fake `/dev/input` and `/sys/class/input`.

mod common;

use std::path::Path;

use common::{KEYBOARD, MOUSE, Root, press};
use rusty_word_count::devices::Keyboards;
use rusty_word_count::stats::Key;

fn read(keyboards: &mut Keyboards) -> Vec<Key> {
    let mut keys = Vec::new();
    keyboards.read(|key| keys.push(key)).unwrap();
    keys
}

#[test]
fn reads_only_from_keyboards() {
    let root = Root::new("reads_only_from_keyboards");
    let mut keyboard = root.plug("event0", KEYBOARD);
    press(&mut keyboard, 2);
    press(&mut root.plug("event1", MOUSE), 1);
    // Another node of a keyboard, which does not speak the evdev protocol.
    press(&mut root.plug("mouse0", KEYBOARD), 1);

    let mut keyboards = Keyboards::open_in(&root.0).unwrap();
    assert_eq!(read(&mut keyboards), [Key::Char; 2]);
    assert_eq!(read(&mut keyboards), []);

    press(&mut keyboard, 1);
    assert_eq!(read(&mut keyboards), [Key::Char]);
}

#[test]
fn picks_up_a_keyboard_plugged_in_later() {
    let root = Root::new("picks_up_a_keyboard_plugged_in_later");
    let mut keyboards = Keyboards::open_in(&root.0).unwrap();
    assert!(keyboards.is_empty());

    let mut keyboard = root.plug("event0", KEYBOARD);
    press(&mut keyboard, 1);
    // The first read notices the new node, the second one reads from it.
    assert_eq!(read(&mut keyboards), []);
    assert_eq!(read(&mut keyboards), [Key::Char]);
}

#[test]
fn fails_without_an_input_directory() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no_such_root");

    assert!(Keyboards::open_in(&root).is_err());
}
