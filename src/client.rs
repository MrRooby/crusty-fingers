//! The `get` and `watch` commands: send one request and print what comes back.

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

pub fn run(socket: &Path, request: &str) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket).map_err(|err| {
        let message = format!("cannot reach the daemon at {}: {err}", socket.display());
        io::Error::new(err.kind(), message)
    })?;
    stream.write_all(format!("{request}\n").as_bytes())?;
    io::copy(&mut stream, &mut io::stdout())?;
    Ok(())
}
