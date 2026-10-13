//! Runs the daemon on a fake keyboard and talks to it over its socket.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use common::{KEYBOARD, Root, press};
use rusty_word_count::daemon;
use rusty_word_count::devices::Keyboards;
use rusty_word_count::server::Server;

const ZERO: &str = r#"{"wpm":0,"average":0,"peak":0,"chars":0,"active_secs":0}"#;

/// Connects, retrying while the daemon is still starting, and sends `request`.
fn request(socket: &Path, request: &str) -> UnixStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match UnixStream::connect(socket) {
            Ok(stream) => break stream,
            Err(err) if Instant::now() > deadline => panic!("daemon did not start: {err}"),
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    };
    stream.write_all(format!("{request}\n").as_bytes()).unwrap();
    stream
}

/// The whole reply to a request that the daemon answers once and then hangs up on.
fn reply(socket: &Path, line: &str) -> String {
    let mut reply = String::new();
    request(socket, line).read_to_string(&mut reply).unwrap();
    reply
}

#[test]
fn serves_the_score_over_the_socket() {
    let root = Root::new("serves_the_score_over_the_socket");
    let mut keyboard = root.plug("event0", KEYBOARD);
    let keyboards = Keyboards::open_in(&root.0).unwrap();
    let socket = root.0.join("socket");
    thread::spawn({
        let socket = socket.clone();
        move || daemon::run(keyboards, &socket)
    });

    assert_eq!(reply(&socket, "get"), format!("{ZERO}\n"));
    assert_eq!(reply(&socket, "bogus"), "{\"error\":\"unknown request\"}\n");

    let mut watcher = BufReader::new(request(&socket, "watch")).lines();
    assert_eq!(watcher.next().unwrap().unwrap(), ZERO);

    // Eleven keys are ten characters after the first, which is two words. A burst this
    // short is measured over three seconds.
    press(&mut keyboard, 11);
    let typed = r#"{"wpm":40,"average":44,"peak":0,"chars":11,"active_secs":0}"#;
    assert_eq!(watcher.next().unwrap().unwrap(), typed);
    assert_eq!(reply(&socket, "get json"), format!("{typed}\n"));
    assert_eq!(
        reply(&socket, "get waybar"),
        "{\"text\":\"40\",\"tooltip\":\"average 44 · peak 0\",\"class\":\"typing\"}\n"
    );

    // Two seconds after the last key the live figure drops to zero.
    let paused = r#"{"wpm":0,"average":44,"peak":0,"chars":11,"active_secs":0}"#;
    assert_eq!(watcher.next().unwrap().unwrap(), paused);
    assert!(reply(&socket, "get waybar").contains(r#""class":"idle""#));
}

#[test]
fn only_one_daemon_serves_a_socket() {
    let root = Root::new("only_one_daemon_serves_a_socket");
    let socket = root.0.join("socket");

    let first = Server::bind(&socket).unwrap();
    assert!(Server::bind(&socket).is_err());

    // What a killed daemon leaves behind is replaced.
    drop(first);
    assert!(socket.exists());
    Server::bind(&socket).unwrap();
}
