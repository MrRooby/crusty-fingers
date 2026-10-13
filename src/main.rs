use std::env;
use std::process::ExitCode;

use rusty_word_count::devices::Keyboards;
use rusty_word_count::protocol::Request;
use rusty_word_count::{client, daemon, socket_path};

const USAGE: &str = "\
usage: rusty-word-count <command>

commands:
  daemon         measure typing speed and serve the score
  get [view]     print the score once
  watch [view]   print the score now and every time it changes

views:
  json           every figure (the default)
  waybar         for a waybar module with \"return-type\": \"json\"";

fn main() -> ExitCode {
    let command = env::args().skip(1).collect::<Vec<_>>().join(" ");

    let result = match command.as_str() {
        "daemon" => socket_path().and_then(|socket| daemon::run(Keyboards::open()?, &socket)),
        request if Request::parse(request).is_some() => {
            socket_path().and_then(|socket| client::run(&socket, request))
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("rusty-word-count: {err}");
            ExitCode::FAILURE
        }
    }
}
