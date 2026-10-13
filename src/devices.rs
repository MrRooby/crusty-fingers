//! Finds the keyboards under /dev/input and reads keystrokes from them.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read};
use std::iter;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::keys::{self, Decoder, EVENT_SIZE};
use crate::stats::Key;
use crate::sys;

struct Keyboard {
    path: PathBuf,
    file: File,
    decoder: Decoder,
}

/// Every keyboard that is currently plugged in.
pub struct Keyboards {
    dev_dir: PathBuf,
    sys_dir: PathBuf,
    /// Becomes readable when a device node appears or its permissions change.
    hotplug: File,
    open: Vec<Keyboard>,
}

impl Keyboards {
    pub fn open() -> io::Result<Self> {
        Self::open_in(Path::new("/"))
    }

    /// Like `open`, with `root` standing in for `/`.
    pub fn open_in(root: &Path) -> io::Result<Self> {
        let dev_dir = root.join("dev/input");
        let mut keyboards = Self {
            hotplug: sys::watch_dir(&dev_dir)?,
            sys_dir: root.join("sys/class/input"),
            dev_dir,
            open: Vec::new(),
        };
        keyboards.scan()?;
        Ok(keyboards)
    }

    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// The descriptors to wait on before calling `read`.
    pub fn fds(&self) -> impl Iterator<Item = BorrowedFd<'_>> {
        let keyboards = self.open.iter().map(|keyboard| keyboard.file.as_fd());
        iter::once(self.hotplug.as_fd()).chain(keyboards)
    }

    /// Passes the keystrokes that are waiting to `on_key`, without blocking, and keeps up
    /// with keyboards being plugged in and out.
    pub fn read(&mut self, mut on_key: impl FnMut(Key)) -> io::Result<()> {
        let mut events = [0; 64 * EVENT_SIZE];
        self.open.retain_mut(|keyboard| match keyboard.file.read(&mut events) {
            Ok(len) => {
                keyboard.decoder.keys(&events[..len]).for_each(&mut on_key);
                true
            }
            // Any other error means the keyboard was unplugged.
            Err(err) => matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted),
        });

        if matches!(self.hotplug.read(&mut events), Ok(1..)) {
            self.scan()?;
        }
        Ok(())
    }

    /// Opens the keyboards that are not open yet.
    fn scan(&mut self) -> io::Result<()> {
        for entry in fs::read_dir(&self.dev_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !self.is_keyboard(&entry.file_name()) || self.open.iter().any(|k| k.path == path) {
                continue;
            }
            let node = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path);
            // A new node cannot be read until udev has set its permissions, and that change
            // triggers another scan.
            if let Ok(file) = node {
                let decoder = Decoder::default();
                self.open.push(Keyboard { path, file, decoder });
            }
        }
        Ok(())
    }

    /// Whether the device node `name` belongs to a keyboard.
    fn is_keyboard(&self, name: &OsStr) -> bool {
        // Other nodes of the same device, such as `mouse0`, speak a different protocol.
        if !name.as_encoded_bytes().starts_with(b"event") {
            return false;
        }
        // The key bitmap is printed as hex words, the one with the lowest key codes last.
        let bitmap = self.sys_dir.join(name).join("device/capabilities/key");
        fs::read_to_string(bitmap)
            .ok()
            .and_then(|bitmap| u64::from_str_radix(bitmap.split_whitespace().next_back()?, 16).ok())
            .is_some_and(keys::is_keyboard)
    }
}
