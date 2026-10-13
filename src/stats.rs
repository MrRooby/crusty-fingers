//! Words-per-minute maths. Time is always passed in, so nothing here does I/O.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Length of the rolling window behind the live figure.
const WINDOW: Duration = Duration::from_secs(10);
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
    /// Speed over the rolling window.
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
    /// Keys inside the rolling window, as (when, +1 or -1 character).
    window: VecDeque<(Instant, i32)>,
    last_key: Option<Instant>,
    chars: u64,
    active: Duration,
    peak: u32,
}

impl Stats {
    pub fn record(&mut self, key: Key, now: Instant) {
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

    pub fn snapshot(&mut self, now: Instant) -> Snapshot {
        while let Some(&(at, _)) = self.window.front()
            && now.duration_since(at) > WINDOW
        {
            self.window.pop_front();
        }

        let span = match self.window.front() {
            Some(&(oldest, _)) => now.duration_since(oldest),
            None => Duration::ZERO,
        };
        let recent: i32 = self.window.iter().map(|&(_, delta)| delta).sum();
        let wpm = rate(recent.max(0) as u64, span);
        if span >= PEAK_MIN_SPAN {
            self.peak = self.peak.max(wpm);
        }

        Snapshot {
            wpm,
            average: rate(self.chars, self.active),
            peak: self.peak,
            chars: self.chars,
            active_secs: self.active.as_secs(),
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
        assert_eq!(Stats::default().snapshot(Instant::now()), Snapshot::default());
    }

    #[test]
    fn steady_typing_fills_the_window() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 50, ms(200));

        let snapshot = stats.snapshot(t0 + ms(10_000));
        assert_eq!(snapshot.wpm, 60);
        assert_eq!(snapshot.peak, 60);
        assert_eq!(snapshot.chars, 50);
    }

    #[test]
    fn live_rate_is_accurate_before_the_window_is_full() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 20, ms(200));

        assert_eq!(stats.snapshot(t0 + ms(4_000)).wpm, 60);
    }

    #[test]
    fn short_burst_is_measured_over_the_minimum_span() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 10, ms(100));

        let snapshot = stats.snapshot(t0 + ms(1_000));
        assert_eq!(snapshot.wpm, 40);
        assert_eq!(snapshot.peak, 0);
    }

    #[test]
    fn live_rate_decays_to_zero_but_peak_stays() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 50, ms(200));
        stats.snapshot(t0 + ms(10_000));

        assert_eq!(stats.snapshot(t0 + ms(15_000)).wpm, 30);
        let snapshot = stats.snapshot(t0 + ms(20_000));
        assert_eq!(snapshot.wpm, 0);
        assert_eq!(snapshot.peak, 60);
    }

    #[test]
    fn backspace_subtracts() {
        let (mut stats, t0) = (Stats::default(), Instant::now());
        press(&mut stats, Key::Char, t0, 20, ms(100));
        press(&mut stats, Key::Backspace, t0 + ms(2_000), 5, ms(100));

        let snapshot = stats.snapshot(t0 + ms(3_000));
        assert_eq!(snapshot.chars, 15);
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
