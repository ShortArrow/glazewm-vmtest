//! The guest daemon's side of a test-window application's connection.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use vmtest_protocol::{AppCommand, AppReply, decode_line, encode_line};

use crate::limits::Limits;

/// Test windows to open in one request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub count: u32,
    /// `None` opens each once the one before is the foreground window; a span opens them at even
    /// intervals within it.
    pub within: Option<Duration>,
    /// `None` keeps them; a number has each destroyed that many milliseconds after its creation.
    pub lifetime_ms: Option<u64>,
}

impl Batch {
    /// `count` persistent windows, each opened once the one before is the foreground window.
    pub fn one_by_one(count: u32) -> Self {
        Self {
            count,
            within: None,
            lifetime_ms: None,
        }
    }
}

/// A window a test-window application has opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedWindow {
    pub id: u32,
    pub handle: i64,
    /// Whether its message processing is stopped.
    pub hung: bool,
}

/// One running test-window application and the windows it has opened.
pub struct TestWindowApp<R, W> {
    reader: R,
    writer: W,
    opened: Vec<OpenedWindow>,
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

    /// Opens the windows of `batch` and returns their ids.
    ///
    /// Ids continue from the windows opened before, so that a window keeps its id for the whole run.
    pub fn open(&mut self, batch: &Batch) -> Result<Vec<u32>, String> {
        let count = batch.count;
        let first = u32::try_from(self.opened.len()).map_err(|_| "too many test windows")?;
        let end = first
            .checked_add(count)
            .ok_or_else(|| format!("{count} more test windows would exhaust the ids"))?;
        let start = Instant::now();
        if let Some(span) = batch.within {
            start
                .checked_add(span)
                .ok_or_else(|| format!("a span of {span:?} is beyond the clock"))?;
        }
        (first..end)
            .map(|id| {
                if let Some(span) = batch.within {
                    let step = span / count;
                    let due = start + step * (id - first);
                    std::thread::sleep(due.saturating_duration_since(Instant::now()));
                }
                let handle = self.open_one(id, batch)?;
                self.opened.push(OpenedWindow {
                    id,
                    handle,
                    hung: false,
                });
                Ok(id)
            })
            .collect()
    }

    /// Every window opened so far, in the order of opening.
    pub fn windows(&self) -> &[OpenedWindow] {
        &self.opened
    }

    /// Stops window `id` from processing messages; returns once the application says it has stopped.
    pub fn hang(&mut self, id: u32) -> Result<(), String> {
        self.change(id, AppCommand::Hang { id }, AppReply::Hung { id }, true)
    }

    /// Lets window `id` process messages again.
    pub fn resume(&mut self, id: u32) -> Result<(), String> {
        self.change(
            id,
            AppCommand::Resume { id },
            AppReply::Resumed { id },
            false,
        )
    }

    fn change(
        &mut self,
        id: u32,
        command: AppCommand,
        expected: AppReply,
        hung: bool,
    ) -> Result<(), String> {
        let index = self
            .opened
            .iter()
            .position(|window| window.id == id)
            .ok_or_else(|| format!("no test window {id} has been opened"))?;
        match self.command(&command)? {
            reply if reply == expected => {
                self.opened[index].hung = hung;
                Ok(())
            }
            other => Err(format!(
                "the test-window application answered {command:?} with {other:?}"
            )),
        }
    }

    fn open_one(&mut self, id: u32, batch: &Batch) -> Result<i64, String> {
        let command = AppCommand::Open {
            id,
            wait_for_foreground: batch.within.is_none(),
            lifetime_ms: batch.lifetime_ms,
        };
        match self.command(&command)? {
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

/// A test-window application started on the first request and kept for the rest of the daemon's life.
pub struct LaunchedApp {
    executable: PathBuf,
    limits: Limits,
    app: Mutex<Option<TestWindowApp<BufReader<TcpStream>, TcpStream>>>,
}

impl LaunchedApp {
    pub fn new(executable: PathBuf, limits: Limits) -> Self {
        Self {
            executable,
            limits,
            app: Mutex::new(None),
        }
    }

    /// Opens the windows of `batch`, starting the application first if it is not running.
    pub fn open(&self, batch: &Batch) -> Result<Vec<u32>, String> {
        self.limits.check(batch)?;
        let mut app = self.app.lock().map_err(|_| "a previous request panicked")?;
        let app = match app.as_mut() {
            Some(app) => app,
            None => app.insert(self.start()?),
        };
        app.open(batch)
    }

    /// Every window opened so far.
    pub fn windows(&self) -> Result<Vec<OpenedWindow>, String> {
        let app = self.app.lock().map_err(|_| "a previous request panicked")?;
        Ok(app
            .as_ref()
            .map(|app| app.windows().to_vec())
            .unwrap_or_default())
    }

    pub fn hang(&self, id: u32) -> Result<(), String> {
        self.with_running(|app| app.hang(id))
    }

    pub fn resume(&self, id: u32) -> Result<(), String> {
        self.with_running(|app| app.resume(id))
    }

    fn with_running(
        &self,
        action: impl FnOnce(&mut TestWindowApp<BufReader<TcpStream>, TcpStream>) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut app = self.app.lock().map_err(|_| "a previous request panicked")?;
        action(app.as_mut().ok_or("no test window has been opened")?)
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
        let connect_limit = Duration::from_millis(self.limits.test_window_connect_limit_ms);
        let start = Instant::now();
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() != std::io::ErrorKind::WouldBlock => {
                    return Err(cannot_start(error));
                }
                Err(_) if start.elapsed() >= connect_limit => {
                    return Err(format!(
                        "the test-window application did not connect within {connect_limit:?}"
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

    fn open_command(id: u32) -> AppCommand {
        AppCommand::Open {
            id,
            wait_for_foreground: true,
            lifetime_ms: None,
        }
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

        assert_eq!(app.open(&Batch::one_by_one(2)), Ok(vec![0, 1]));
        assert_eq!(app.open(&Batch::one_by_one(1)), Ok(vec![2]));
        assert_eq!(
            commands(app.writer()),
            [open_command(0), open_command(1), open_command(2)]
        );
        let handles: Vec<_> = app.windows().iter().map(|w| (w.id, w.handle)).collect();
        assert_eq!(handles, [(0, 10), (1, 11), (2, 12)]);
    }

    fn opened(
        replies_after_opening: &[AppReply],
    ) -> TestWindowApp<std::io::Cursor<Vec<u8>>, Vec<u8>> {
        let mut all = vec![
            AppReply::Opened { id: 0, handle: 10 },
            AppReply::Opened { id: 1, handle: 11 },
        ];
        all.extend_from_slice(replies_after_opening);
        let mut app = TestWindowApp::new(replies(&all), Vec::new());
        app.open(&Batch::one_by_one(2)).unwrap();
        app
    }

    fn hung_ids<R: BufRead, W: Write>(app: &TestWindowApp<R, W>) -> Vec<u32> {
        app.windows()
            .iter()
            .filter(|window| window.hung)
            .map(|window| window.id)
            .collect()
    }

    #[test]
    fn stops_and_resumes_a_window_and_remembers_which_is_stopped() {
        let mut app = opened(&[AppReply::Hung { id: 1 }, AppReply::Resumed { id: 1 }]);

        app.hang(1).unwrap();
        let while_hung = hung_ids(&app);
        app.resume(1).unwrap();

        assert_eq!(while_hung, [1]);
        assert!(hung_ids(&app).is_empty());
        assert_eq!(
            commands(app.writer())[2..],
            [AppCommand::Hang { id: 1 }, AppCommand::Resume { id: 1 }]
        );
    }

    #[test]
    fn refuses_to_stop_a_window_it_has_not_opened() {
        let mut app = opened(&[]);

        assert!(app.hang(5).is_err());
        assert_eq!(commands(app.writer()).len(), 2);
    }

    #[test]
    fn does_not_count_a_window_as_stopped_when_the_application_answers_otherwise() {
        let mut app = opened(&[AppReply::Error {
            message: "window 1 is already hung".into(),
        }]);

        assert!(app.hang(1).is_err());
        assert!(hung_ids(&app).is_empty());
    }

    #[test]
    fn opens_a_batch_within_a_span_without_waiting_for_the_foreground() {
        let mut app = TestWindowApp::new(
            replies(&[
                AppReply::Opened { id: 0, handle: 10 },
                AppReply::Opened { id: 1, handle: 11 },
            ]),
            Vec::new(),
        );
        let batch = Batch {
            count: 2,
            within: Some(Duration::ZERO),
            lifetime_ms: Some(5),
        };

        app.open(&batch).unwrap();

        let short_lived = |id| AppCommand::Open {
            id,
            wait_for_foreground: false,
            lifetime_ms: Some(5),
        };
        assert_eq!(commands(app.writer()), [short_lived(0), short_lived(1)]);
    }

    #[test]
    fn rejects_a_span_beyond_the_clock_without_sending_anything() {
        let mut app = TestWindowApp::new(replies(&[]), Vec::new());
        let batch = Batch {
            count: 2,
            within: Some(Duration::MAX),
            lifetime_ms: None,
        };

        assert!(app.open(&batch).is_err());
        assert!(commands(app.writer()).is_empty());
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
            app.open(&Batch::one_by_one(1)),
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

        assert!(app.open(&Batch::one_by_one(1)).is_err());
        assert!(app.windows().is_empty());
    }

    #[test]
    fn refuses_a_batch_beyond_its_limits_before_starting_anything() {
        let limits = Limits {
            max_test_windows_per_request: 1,
            ..Limits::default()
        };
        let host = LaunchedApp::new("vmtest-no-such-application.exe".into(), limits);

        let error = host.open(&Batch::one_by_one(2)).unwrap_err();

        assert!(error.contains("exceed"), "{error}");
    }

    #[test]
    fn reports_an_application_that_cannot_be_started() {
        let host = LaunchedApp::new("vmtest-no-such-application.exe".into(), Limits::default());

        let error = host.open(&Batch::one_by_one(1)).unwrap_err();

        assert!(
            error.starts_with("cannot start the test-window application"),
            "{error}"
        );
    }

    #[test]
    fn gives_up_on_an_application_that_does_not_connect_within_the_configured_limit() {
        let limits = Limits {
            test_window_connect_limit_ms: 200,
            ..Limits::default()
        };
        let never_connects =
            PathBuf::from(std::env::var("SystemRoot").unwrap()).join(r"System32\whoami.exe");
        let host = LaunchedApp::new(never_connects, limits);

        let error = host.open(&Batch::one_by_one(1)).unwrap_err();

        assert_eq!(
            error,
            "the test-window application did not connect within 200ms"
        );
    }

    #[test]
    fn rejects_a_count_whose_ids_would_overflow_without_sending_anything() {
        let mut app = TestWindowApp::new(
            replies(&[AppReply::Opened { id: 0, handle: 10 }]),
            Vec::new(),
        );
        app.open(&Batch::one_by_one(1)).unwrap();

        assert!(app.open(&Batch::one_by_one(u32::MAX)).is_err());
        assert_eq!(commands(app.writer()), [open_command(0)]);
    }

    #[test]
    fn reports_a_closed_connection_as_an_error() {
        let mut app = TestWindowApp::new(replies(&[]), Vec::new());

        assert!(app.open(&Batch::one_by_one(1)).is_err());
    }
}
