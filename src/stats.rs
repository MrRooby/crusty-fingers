//! Words-per-minute maths. Time is always passed in, so nothing here does I/O.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How far back from the newest key the live figure looks.
const WINDOW: Duration = Duration::from_secs(10);
/// After this long without a key the live figure drops to 0.
const PAUSE: Duration = Duration::from_secs(2);
/// Shortest time a rate is measured over, so the first few keys do not read as a huge speed.
const MIN_SPAN: Duration = Duration::from_secs(3);
/// The live figure only counts towards the peak once it covers at least this much time.
const PEAK_MIN_SPAN: Duration = Duration::from_secs(8);
/// A pause longer than this is not counted as typing time.
const IDLE_GAP: Duration = Duration::from_secs(5);
const CHARS_PER_WORD: f64 = 5.0;

/// The two kinds of keystroke that affect the score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char,
    Backspace,
}

/// The score at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// Speed of the typing in progress, or 0 during a pause.
    pub wpm: u32,
    /// Speed over all typing time this session.
    pub average: u32,
    /// Highest sustained `wpm` this session.
    pub peak: u32,
    /// Characters typed this session, net of backspaces.
    pub chars: u64,
    /// Time spent typing this session.
    pub active_secs: u64,
}

#[derive(Debug, Default)]
pub struct Stats {
    /// The recent keys of the typing in progress, as (when, +1 or -1 character). Empty
    /// during a pause.
    window: VecDeque<(Instant, i32)>,
    last_key: Option<Instant>,
    chars: u64,
    active: Duration,
    peak: u32,
}

impl Stats {
    pub fn record(&mut self, key: Key, now: Instant) {
        self.end_burst_if_paused(now);
        if let Some(last) = self.last_key {
            let gap = now.duration_since(last);
            if gap <= IDLE_GAP {
                self.active += gap;
            }
        }
        self.last_key = Some(now);

        let delta = match key {
            Key::Char => 1,
            Key::Backspace => -1,
        };
        self.chars = self.chars.saturating_add_signed(delta.into());
        self.window.push_back((now, delta));
    }

    /// Whether there was no typing in progress at the last `record` or `snapshot`. If so,
    /// the live figure stays at 0 until the next key.
    pub fn is_idle(&self) -> bool {
        self.window.is_empty()
    }

    pub fn snapshot(&mut self, now: Instant) -> Snapshot {
        self.end_burst_if_paused(now);

        let mut wpm = 0;
        // The window is measured up to its newest key rather than up to `now`, so that the
        // figure holds steady between keys.
        if let Some(&(newest, _)) = self.window.back() {
            while newest.duration_since(self.window[0].0) > WINDOW {
                self.window.pop_front();
            }
            let span = newest.duration_since(self.window[0].0);
            // The oldest key only marks where the measured time starts.
            let typed: i32 = self.window.iter().skip(1).map(|&(_, delta)| delta).sum();
            wpm = rate(typed.max(0) as u64, span);
            if span >= PEAK_MIN_SPAN {
                self.peak = self.peak.max(wpm);
            }
        }

        Snapshot {
            wpm,
            average: rate(self.chars, self.active),
            peak: self.peak,
            chars: self.chars,
            active_secs: self.active.as_secs(),
        }
    }

    /// Empties the window if the newest key in it is more than `PAUSE` old.
    fn end_burst_if_paused(&mut self, now: Instant) {
        if let Some(&(newest, _)) = self.window.back()
            && now.duration_since(newest) > PAUSE
        {
            self.window.clear();
        }
    }
}

/// Words per minute for `chars` characters typed over `span`.
fn rate(chars: u64, span: Duration) -> u32 {
    let minutes = span.max(MIN_SPAN).as_secs_f64() / 60.0;
    (chars as f64 / CHARS_PER_WORD / minutes).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    /// Records `count` keys, one every `gap`, starting at `start`.
    fn press(stats: &mut Stats, key: Key, start: Instant, count: u32, gap: Duration) {
        for i in 0..count {
            stats.record(key, start + gap * i);
        }
    }

    #[test]
    fn starts_at_zero() {
        let mut stats = Stats::default();

        assert_eq!(stats.snapshot(Instant::now()), Snapshot::default());
        assert!(stats.is_idle());
    }

    #[test]
    fn steady_typing_reads_its_true_speed() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        // Five keys a second for 20 seconds, which is 60 WPM.
        press(&mut stats, Key::Char, t0, 101, ms(200));

        let snapshot = stats.snapshot(t0 + ms(20_000));
        assert_eq!(snapshot.wpm, 60);
        assert_eq!(snapshot.peak, 60);
        assert_eq!(snapshot.chars, 101);
    }

    #[test]
    fn live_rate_is_accurate_before_the_window_is_full() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 21, ms(200));

        assert_eq!(stats.snapshot(t0 + ms(4_000)).wpm, 60);
    }

    #[test]
    fn short_burst_is_measured_over_the_minimum_span() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 11, ms(100));

        let snapshot = stats.snapshot(t0 + ms(1_000));
        assert_eq!(snapshot.wpm, 40);
        assert_eq!(snapshot.peak, 0);
    }

    #[test]
    fn live_rate_holds_through_a_short_pause_then_drops_to_zero() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 51, ms(200));

        assert_eq!(stats.snapshot(t0 + ms(10_000)).wpm, 60);
        assert_eq!(stats.snapshot(t0 + ms(12_000)).wpm, 60);
        assert!(!stats.is_idle());

        let snapshot = stats.snapshot(t0 + ms(12_001));
        assert_eq!(snapshot.wpm, 0);
        assert_eq!(snapshot.peak, 60);
        assert!(stats.is_idle());
    }

    #[test]
    fn typing_after_a_pause_is_measured_on_its_own() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 51, ms(200));
        // Three seconds later, half as fast.
        press(&mut stats, Key::Char, t0 + ms(13_000), 11, ms(400));

        assert_eq!(stats.snapshot(t0 + ms(17_000)).wpm, 30);
    }

    #[test]
    fn backspace_subtracts() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 21, ms(100));
        press(&mut stats, Key::Backspace, t0 + ms(2_100), 5, ms(100));

        let snapshot = stats.snapshot(t0 + ms(3_000));
        assert_eq!(snapshot.chars, 16);
        assert_eq!(snapshot.wpm, 60);
    }

    #[test]
    fn backspace_alone_never_goes_negative() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Backspace, t0, 5, ms(100));

        let snapshot = stats.snapshot(t0 + ms(1_000));
        assert_eq!(snapshot.chars, 0);
        assert_eq!(snapshot.wpm, 0);
    }

    #[test]
    fn average_ignores_idle_time() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 50, ms(200));
        press(&mut stats, Key::Char, t0 + ms(70_000), 50, ms(200));

        // 100 characters over two 9.8 s stretches; the minute between them does not count.
        let snapshot = stats.snapshot(t0 + ms(80_000));
        assert_eq!(snapshot.active_secs, 19);
        assert_eq!(snapshot.average, 61);
    }
}
