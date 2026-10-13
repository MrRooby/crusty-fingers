use std::env;
use std::process::ExitCode;

use rusty_word_count::daemon;

const USAGE: &str = "\
usage: rusty-word-count <command>

commands:
  daemon   measure typing speed until stopped";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    let result = match args[..] {
        ["daemon"] => daemon::run(),
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
