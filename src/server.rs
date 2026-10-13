//! The Unix socket API: a client connects, sends one request line and reads snapshot lines.

use std::fs;
use std::io::{self, ErrorKind, Read, Write};
use std::iter;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

use crate::protocol::{Request, UNKNOWN_REQUEST, View};
use crate::stats::Snapshot;

/// Connections beyond this many are turned away, so that clients cannot use up the
/// daemon's file descriptors.
const MAX_CLIENTS: usize = 64;
/// No valid request line is this long.
const MAX_REQUEST: usize = 64;

struct Client {
    stream: UnixStream,
    /// What has arrived of the request line so far.
    request: Vec<u8>,
    /// The view to send updates in, once the client has asked to watch.
    watching: Option<View>,
}

impl Client {
    /// Handles what the client has sent. Returns whether to keep the connection.
    fn serve(&mut self, snapshot: &Snapshot) -> bool {
        let mut bytes = [0; MAX_REQUEST];
        let len = match self.stream.read(&mut bytes) {
            Ok(0) => return false,
            Ok(len) => len,
            Err(err) => {
                return matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted);
            }
        };
        if self.watching.is_some() {
            return true;
        }

        self.request.extend_from_slice(&bytes[..len]);
        let Some(end) = self.request.iter().position(|&byte| byte == b'\n') else {
            return self.request.len() < MAX_REQUEST;
        };
        let request = str::from_utf8(&self.request[..end]).ok();
        let Some(request) = request.and_then(Request::parse) else {
            self.send(UNKNOWN_REQUEST);
            return false;
        };
        if request.watch {
            self.watching = Some(request.view);
        }
        self.send(&request.view.render(snapshot)) && request.watch
    }

    /// Sends one line. Returns false if the client is gone or is not reading fast enough.
    fn send(&mut self, line: &str) -> bool {
        self.stream.write_all(format!("{line}\n").as_bytes()).is_ok()
    }
}

pub struct Server {
    listener: UnixListener,
    clients: Vec<Client>,
}

impl Server {
    /// Starts listening on `path`, unless another daemon already does.
    pub fn bind(path: &Path) -> io::Result<Self> {
        if UnixStream::connect(path).is_ok() {
            let message = format!("a daemon is already serving {}", path.display());
            return Err(io::Error::new(ErrorKind::AddrInUse, message));
        }
        // Nobody answered, so a socket file that is still there was left behind by a daemon
        // that was killed.
        let _ = fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            clients: Vec::new(),
        })
    }

    /// The descriptors to wait on before calling `serve`.
    pub fn fds(&self) -> impl Iterator<Item = BorrowedFd<'_>> {
        let clients = self.clients.iter().map(|client| client.stream.as_fd());
        iter::once(self.listener.as_fd()).chain(clients)
    }

    /// Accepts new clients and answers the requests that have arrived with `snapshot`,
    /// without blocking.
    pub fn serve(&mut self, snapshot: &Snapshot) {
        while let Ok((stream, _)) = self.listener.accept() {
            if self.clients.len() < MAX_CLIENTS && stream.set_nonblocking(true).is_ok() {
                self.clients.push(Client {
                    stream,
                    request: Vec::new(),
                    watching: None,
                });
            }
        }
        self.clients.retain_mut(|client| client.serve(snapshot));
    }

    /// Sends `snapshot` to the clients that are watching.
    pub fn publish(&mut self, snapshot: &Snapshot) {
        self.clients.retain_mut(|client| match client.watching {
            Some(view) => client.send(&view.render(snapshot)),
            None => true,
        });
    }
}
