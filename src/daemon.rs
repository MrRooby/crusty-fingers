//! The event loop: keystrokes in, score out.

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::devices::Keyboards;
use crate::server::Server;
use crate::stats::{Snapshot, Stats};
use crate::sys;

/// How often the score is published while there is recent typing.
const TICK: Duration = Duration::from_secs(1);

/// Measures the typing on `keyboards` and serves the score on `socket` until an error
/// occurs.
pub fn run(mut keyboards: Keyboards, socket: &Path) -> io::Result<()> {
    let mut server = Server::bind(socket)?;
    if keyboards.is_empty() {
        eprintln!("rusty-word-count: no readable keyboard; is this user in the `input` group?");
    }
    let mut stats = Stats::default();
    let mut published = Snapshot::default();
    // When the score is next due to be published. `None` while idle, and the loop then
    // sleeps until a key is pressed or a client calls.
    let mut next_tick: Option<Instant> = None;

    loop {
        let fds: Vec<_> = keyboards.fds().chain(server.fds()).collect();
        let timeout = next_tick.map(|at| at.saturating_duration_since(Instant::now()));
        sys::wait(&fds, timeout)?;

        let now = Instant::now();
        keyboards.read(|key| stats.record(key, now))?;
        let snapshot = stats.snapshot(now);
        server.serve(&snapshot);

        match next_tick {
            None if !stats.is_idle() => next_tick = Some(now + TICK),
            Some(at) if now >= at => {
                if snapshot != published {
                    server.publish(&snapshot);
                    published = snapshot;
                }
                next_tick = (!stats.is_idle()).then_some(now + TICK);
            }
            _ => {}
        }
    }
}
