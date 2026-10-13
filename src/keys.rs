//! Turns raw evdev bytes into the keystrokes the score counts.

use std::ffi::c_long;

use crate::stats::Key;

/// Size of the kernel's `struct input_event`: a `timeval`, then a `u16` type, a `u16` code
/// and an `i32` value.
pub const EVENT_SIZE: usize = TIME_SIZE + 8;
const TIME_SIZE: usize = 2 * size_of::<c_long>();

const EV_KEY: u16 = 1;
const RELEASE: i32 = 0;
const PRESS: i32 = 1;

// Key codes from linux/input-event-codes.h.
const KEY_1: u16 = 2;
const KEY_EQUAL: u16 = 13;
const KEY_BACKSPACE: u16 = 14;
const KEY_Q: u16 = 16;
const KEY_ENTER: u16 = 28;
const KEY_LEFTCTRL: u16 = 29;
const KEY_A: u16 = 30;
const KEY_GRAVE: u16 = 41;
const KEY_BACKSLASH: u16 = 43;
const KEY_SLASH: u16 = 53;
const KEY_LEFTALT: u16 = 56;
const KEY_SPACE: u16 = 57;
const KEY_102ND: u16 = 86;
const KEY_RIGHTCTRL: u16 = 97;
const KEY_LEFTMETA: u16 = 125;
const KEY_RIGHTMETA: u16 = 126;

/// Modifiers that turn a key press into a shortcut. Shift and right Alt (AltGr) are left
/// out because they produce characters.
const SHORTCUT_MODIFIERS: [u16; 5] = [
    KEY_LEFTCTRL,
    KEY_RIGHTCTRL,
    KEY_LEFTALT,
    KEY_LEFTMETA,
    KEY_RIGHTMETA,
];

/// Whether a device is a keyboard, judged by the lowest word of its evdev key bitmap, in
/// which bit `n` stands for key code `n`.
pub fn is_keyboard(key_bitmap: u64) -> bool {
    const REQUIRED: u64 = 1 << KEY_Q | 1 << KEY_ENTER | 1 << KEY_A;
    key_bitmap & REQUIRED == REQUIRED
}

/// Follows one keyboard's modifier state, so that shortcuts are not counted as typing.
#[derive(Debug, Default)]
pub struct Decoder {
    /// Shortcut modifiers currently held down, one bit per entry of `SHORTCUT_MODIFIERS`.
    held: u8,
}

impl Decoder {
    /// Decodes a buffer of whole `input_event`s, as read from an evdev device.
    pub fn keys<'a>(&'a mut self, events: &'a [u8]) -> impl Iterator<Item = Key> + 'a {
        events.chunks_exact(EVENT_SIZE).filter_map(|event| {
            let event = &event[TIME_SIZE..];
            let kind = u16::from_ne_bytes([event[0], event[1]]);
            let code = u16::from_ne_bytes([event[2], event[3]]);
            let value = i32::from_ne_bytes([event[4], event[5], event[6], event[7]]);
            if kind != EV_KEY {
                return None;
            }
            self.key(code, value)
        })
    }

    fn key(&mut self, code: u16, value: i32) -> Option<Key> {
        if let Some(index) = SHORTCUT_MODIFIERS.iter().position(|&m| m == code) {
            let bit = 1 << index;
            if value == RELEASE {
                self.held &= !bit;
            } else {
                self.held |= bit;
            }
            return None;
        }
        // Anything but PRESS is a release or an auto-repeat.
        if value != PRESS || self.held != 0 {
            return None;
        }
        match code {
            KEY_BACKSPACE => Some(Key::Backspace),
            KEY_1..=KEY_EQUAL
            | KEY_Q..=KEY_ENTER
            | KEY_A..=KEY_GRAVE
            | KEY_BACKSLASH..=KEY_SLASH
            | KEY_SPACE
            | KEY_102ND => Some(Key::Char),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Key::{Backspace, Char};

    const REPEAT: i32 = 2;
    const KEY_ESC: u16 = 1;
    const KEY_TAB: u16 = 15;
    const KEY_LEFTSHIFT: u16 = 42;
    const KEY_F1: u16 = 59;
    const KEY_RIGHTALT: u16 = 100;
    const KEY_LEFT: u16 = 105;

    fn event(kind: u16, code: u16, value: i32) -> [u8; EVENT_SIZE] {
        let mut raw = [0; EVENT_SIZE];
        raw[TIME_SIZE..TIME_SIZE + 2].copy_from_slice(&kind.to_ne_bytes());
        raw[TIME_SIZE + 2..TIME_SIZE + 4].copy_from_slice(&code.to_ne_bytes());
        raw[TIME_SIZE + 4..].copy_from_slice(&value.to_ne_bytes());
        raw
    }

    fn key(code: u16, value: i32) -> [u8; EVENT_SIZE] {
        event(EV_KEY, code, value)
    }

    fn decode(decoder: &mut Decoder, events: &[[u8; EVENT_SIZE]]) -> Vec<Key> {
        decoder.keys(events.as_flattened()).collect()
    }

    #[test]
    fn keyboards_are_told_apart_by_their_keys() {
        assert!(is_keyboard(0xffff_ffff_ffff_fffe));
        // A media-key device that has Esc and Enter but no letters.
        assert!(!is_keyboard(0x1000_0002));
        assert!(!is_keyboard(0));
    }

    #[test]
    fn typing_keys_are_characters() {
        let presses = [KEY_1, KEY_Q, KEY_ENTER, KEY_A, KEY_SLASH, KEY_SPACE, KEY_102ND];
        let events = presses.map(|code| key(code, PRESS));

        assert_eq!(decode(&mut Decoder::default(), &events), [Char; 7]);
    }

    #[test]
    fn backspace_is_reported_as_such() {
        let events = [key(KEY_A, PRESS), key(KEY_BACKSPACE, PRESS)];

        assert_eq!(decode(&mut Decoder::default(), &events), [Char, Backspace]);
    }

    #[test]
    fn other_keys_are_ignored() {
        let events = [KEY_ESC, KEY_TAB, KEY_F1, KEY_LEFT].map(|code| key(code, PRESS));

        assert_eq!(decode(&mut Decoder::default(), &events), []);
    }

    #[test]
    fn releases_and_repeats_are_ignored() {
        let events = [key(KEY_A, PRESS), key(KEY_A, REPEAT), key(KEY_A, RELEASE)];

        assert_eq!(decode(&mut Decoder::default(), &events), [Char]);
    }

    #[test]
    fn events_that_are_not_keys_are_ignored() {
        const EV_SYN: u16 = 0;
        const EV_MSC: u16 = 4;
        let events = [event(EV_MSC, KEY_A, PRESS), event(EV_SYN, 0, 0)];

        assert_eq!(decode(&mut Decoder::default(), &events), []);
    }

    #[test]
    fn shortcuts_are_ignored_until_the_modifier_is_released() {
        let mut decoder = Decoder::default();

        // The modifier stays held across separate reads from the device.
        assert_eq!(decode(&mut decoder, &[key(KEY_LEFTCTRL, PRESS)]), []);
        assert_eq!(decode(&mut decoder, &[key(KEY_A, PRESS)]), []);
        assert_eq!(decode(&mut decoder, &[key(KEY_BACKSPACE, PRESS)]), []);
        let events = [key(KEY_LEFTCTRL, RELEASE), key(KEY_A, PRESS)];
        assert_eq!(decode(&mut decoder, &events), [Char]);
    }

    #[test]
    fn one_of_two_held_modifiers_still_blocks() {
        let events = [
            key(KEY_LEFTCTRL, PRESS),
            key(KEY_LEFTALT, PRESS),
            key(KEY_LEFTCTRL, RELEASE),
            key(KEY_A, PRESS),
        ];

        assert_eq!(decode(&mut Decoder::default(), &events), []);
    }

    #[test]
    fn shift_and_altgr_still_type() {
        let events = [
            key(KEY_LEFTSHIFT, PRESS),
            key(KEY_A, PRESS),
            key(KEY_LEFTSHIFT, RELEASE),
            key(KEY_RIGHTALT, PRESS),
            key(KEY_A, PRESS),
        ];

        assert_eq!(decode(&mut Decoder::default(), &events), [Char, Char]);
    }
}
