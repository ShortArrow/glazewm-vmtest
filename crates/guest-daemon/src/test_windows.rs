//! The guest daemon's side of a test-window application's connection.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use vmtest_protocol::{AppCommand, AppReply, decode_line, encode_line};

/// One running test-window application and the windows it has opened.
pub struct TestWindowApp<R, W> {
    reader: R,
    writer: W,
    opened: Vec<(u32, i64)>,
}

impl<R: BufRead, W: Write> TestWindowApp<R, W> {
    /// Talks to an application over `reader` and `writer`, the two halves of its connection.
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            opened: Vec::new(),
        }
    }

    /// Opens `count` more windows, one at a time, and returns their ids.
    ///
    /// Ids continue from the windows opened before, so that a window keeps its id for the whole run.
    pub fn open(&mut self, count: u32) -> Result<Vec<u32>, String> {
        let first = u32::try_from(self.opened.len()).map_err(|_| "too many test windows")?;
        (first..first + count)
            .map(|id| {
                let handle = self.open_one(id)?;
                self.opened.push((id, handle));
                Ok(id)
            })
            .collect()
    }

    /// The id and handle of every window opened so far, in the order of opening.
    pub fn handles(&self) -> &[(u32, i64)] {
        &self.opened
    }

    fn open_one(&mut self, id: u32) -> Result<i64, String> {
        match self.command(&AppCommand::Open { id })? {
            AppReply::Opened { id: opened, handle } if opened == id => Ok(handle),
            AppReply::Error { message } => Err(format!(
                "the test-window application could not open window {id}: {message}"
            )),
            other => Err(format!(
                "the test-window application answered opening window {id} with {other:?}"
            )),
        }
    }

    fn command(&mut self, command: &AppCommand) -> Result<AppReply, String> {
        let unreachable =
            |error: std::io::Error| format!("the test-window application is unreachable: {error}");
        self.writer
            .write_all(encode_line(command).as_bytes())
            .and_then(|()| self.writer.flush())
            .map_err(unreachable)?;
        let mut line = String::new();
        if self.reader.read_line(&mut line).map_err(unreachable)? == 0 {
            return Err("the test-window application closed its connection".into());
        }
        decode_line(&line).map_err(|error| error.to_string())
    }

    #[cfg(test)]
    fn writer(&self) -> &W {
        &self.writer
    }
}

/// How long a started test-window application may take to connect back.
const CONNECT_LIMIT: Duration = Duration::from_secs(3);

/// A test-window application started on the first request and kept for the rest of the daemon's life.
pub struct LaunchedApp {
    executable: PathBuf,
    app: Mutex<Option<TestWindowApp<BufReader<TcpStream>, TcpStream>>>,
}

impl LaunchedApp {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            app: Mutex::new(None),
        }
    }

    /// Opens `count` more windows, starting the application first if it is not running.
    pub fn open(&self, count: u32) -> Result<Vec<u32>, String> {
        let mut app = self.app.lock().map_err(|_| "a previous request panicked")?;
        let app = match app.as_mut() {
            Some(app) => app,
            None => app.insert(self.start()?),
        };
        app.open(count)
    }

    /// The id and handle of every window opened so far.
    pub fn handles(&self) -> Result<Vec<(u32, i64)>, String> {
        let app = self.app.lock().map_err(|_| "a previous request panicked")?;
        Ok(app
            .as_ref()
            .map(|app| app.handles().to_vec())
            .unwrap_or_default())
    }

    /// Starts the application with the address of a fresh loopback listener and waits for it to connect.
    fn start(&self) -> Result<TestWindowApp<BufReader<TcpStream>, TcpStream>, String> {
        let cannot_start =
            |error: std::io::Error| format!("cannot start the test-window application: {error}");
        let listener = TcpListener::bind("127.0.0.1:0").map_err(cannot_start)?;
        let address = listener.local_addr().map_err(cannot_start)?;
        Command::new(&self.executable)
            .arg(address.to_string())
            .spawn()
            .map_err(cannot_start)?;
        listener.set_nonblocking(true).map_err(cannot_start)?;
        let start = Instant::now();
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() != std::io::ErrorKind::WouldBlock => {
                    return Err(cannot_start(error));
                }
                Err(_) if start.elapsed() >= CONNECT_LIMIT => {
                    return Err(format!(
                        "the test-window application did not connect within {CONNECT_LIMIT:?}"
                    ));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        stream.set_nonblocking(false).map_err(cannot_start)?;
        let reader = stream.try_clone().map_err(cannot_start)?;
        Ok(TestWindowApp::new(BufReader::new(reader), stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replies(replies: &[AppReply]) -> std::io::Cursor<Vec<u8>> {
        let text: String = replies.iter().map(encode_line).collect();
        std::io::Cursor::new(text.into_bytes())
    }

    fn commands(written: &[u8]) -> Vec<AppCommand> {
        String::from_utf8(written.to_vec())
            .unwrap()
            .lines()
            .map(|line| decode_line(line).unwrap())
            .collect()
    }

    #[test]
    fn numbers_windows_in_the_order_they_are_opened_across_requests() {
        let mut app = TestWindowApp::new(
            replies(&[
                AppReply::Opened { id: 0, handle: 10 },
                AppReply::Opened { id: 1, handle: 11 },
                AppReply::Opened { id: 2, handle: 12 },
            ]),
            Vec::new(),
        );

        assert_eq!(app.open(2), Ok(vec![0, 1]));
        assert_eq!(app.open(1), Ok(vec![2]));
        assert_eq!(
            commands(app.writer()),
            [
                AppCommand::Open { id: 0 },
                AppCommand::Open { id: 1 },
                AppCommand::Open { id: 2 }
            ]
        );
        assert_eq!(app.handles(), [(0, 10), (1, 11), (2, 12)]);
    }

    #[test]
    fn passes_on_the_error_the_application_reports() {
        let mut app = TestWindowApp::new(
            replies(&[AppReply::Error {
                message: "CreateWindowExW failed".into(),
            }]),
            Vec::new(),
        );

        assert_eq!(
            app.open(1),
            Err(
                "the test-window application could not open window 0: CreateWindowExW failed"
                    .into()
            )
        );
    }

    #[test]
    fn rejects_a_reply_for_another_window() {
        let mut app = TestWindowApp::new(
            replies(&[AppReply::Opened { id: 7, handle: 10 }]),
            Vec::new(),
        );

        assert!(app.open(1).is_err());
        assert!(app.handles().is_empty());
    }

    #[test]
    fn reports_an_application_that_cannot_be_started() {
        let host = LaunchedApp::new("vmtest-no-such-application.exe".into());

        let error = host.open(1).unwrap_err();

        assert!(
            error.starts_with("cannot start the test-window application"),
            "{error}"
        );
    }

    #[test]
    fn reports_a_closed_connection_as_an_error() {
        let mut app = TestWindowApp::new(replies(&[]), Vec::new());

        assert!(app.open(1).is_err());
    }
}
