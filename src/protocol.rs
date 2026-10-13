//! The text protocol spoken over the socket: one request line in, snapshot lines out.

use crate::stats::Snapshot;

/// The reply to a request line that is not understood.
pub const UNKNOWN_REQUEST: &str = r#"{"error":"unknown request"}"#;

/// How a snapshot is rendered for a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Every field of the snapshot.
    Json,
    /// What a waybar custom module with `"return-type": "json"` expects.
    Waybar,
}

impl View {
    /// Renders `snapshot` as one line, without the newline.
    pub fn render(self, snapshot: &Snapshot) -> String {
        let Snapshot {
            wpm,
            average,
            peak,
            chars,
            active_secs,
        } = *snapshot;
        match self {
            Self::Json => format!(
                r#"{{"wpm":{wpm},"average":{average},"peak":{peak},"chars":{chars},"active_secs":{active_secs}}}"#
            ),
            Self::Waybar => {
                let class = if wpm > 0 { "typing" } else { "idle" };
                format!(
                    r#"{{"text":"{wpm}","tooltip":"average {average} · peak {peak}","class":"{class}"}}"#
                )
            }
        }
    }
}

/// A request line: `get` or `watch`, optionally followed by `json` or `waybar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// Whether to keep sending the score as it changes, rather than just once.
    pub watch: bool,
    pub view: View,
}

impl Request {
    pub fn parse(line: &str) -> Option<Self> {
        let mut words = line.split_ascii_whitespace();
        let watch = match words.next()? {
            "get" => false,
            "watch" => true,
            _ => return None,
        };
        let view = match words.next() {
            None | Some("json") => View::Json,
            Some("waybar") => View::Waybar,
            Some(_) => return None,
        };
        words.next().is_none().then_some(Self { watch, view })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAPSHOT: Snapshot = Snapshot {
        wpm: 72,
        average: 65,
        peak: 98,
        chars: 1234,
        active_secs: 228,
    };

    #[test]
    fn json_view_has_every_field() {
        assert_eq!(
            View::Json.render(&SNAPSHOT),
            r#"{"wpm":72,"average":65,"peak":98,"chars":1234,"active_secs":228}"#
        );
    }

    #[test]
    fn waybar_view_shows_the_live_figure() {
        assert_eq!(
            View::Waybar.render(&SNAPSHOT),
            r#"{"text":"72","tooltip":"average 65 · peak 98","class":"typing"}"#
        );
    }

    #[test]
    fn waybar_view_is_idle_at_zero() {
        let idle = Snapshot { wpm: 0, ..SNAPSHOT };

        assert!(View::Waybar.render(&idle).ends_with(r#""class":"idle"}"#));
    }

    #[test]
    fn requests_name_a_command_and_optionally_a_view() {
        let request = |watch, view| Some(Request { watch, view });

        assert_eq!(Request::parse("get"), request(false, View::Json));
        assert_eq!(Request::parse("get json"), request(false, View::Json));
        assert_eq!(Request::parse("watch"), request(true, View::Json));
        assert_eq!(Request::parse("watch waybar\r"), request(true, View::Waybar));
    }

    #[test]
    fn anything_else_is_not_a_request() {
        for line in ["", "daemon", "get xml", "get waybar now", "GET"] {
            assert_eq!(Request::parse(line), None, "{line:?}");
        }
    }
}
