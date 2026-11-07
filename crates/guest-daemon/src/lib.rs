//! The guest daemon: answers the runner's requests from inside the guest's interactive session.

#[cfg(windows)]
pub mod win32;

pub mod limits;
pub mod load;
pub mod process;
pub mod test_windows;

use std::io::{BufRead, Write};
use std::time::Duration;

use vmtest_protocol::{
    DisplayDto, LoadKindDto, PresenceDto, Request, Response, SutProgram, TestWindowDto,
    decode_line, encode_line,
};

use crate::test_windows::Batch;

/// Reads the displays the guest OS currently reports.
pub trait TopologySource {
    /// Returns the current displays, or why they could not be read.
    fn displays(&self) -> Result<Vec<DisplayDto>, String>;
}

/// What a finished command printed and how it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// How a command run with a time limit ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliOutcome {
    Finished(CommandOutput),
    /// It had not finished when the limit passed, and was terminated.
    TimedOut,
}

/// Runs the SUT's `glazewm-cli`.
pub trait GlazewmCli {
    /// Runs `glazewm-cli` with `args` for at most `limit`, or says why it could not be started.
    fn run(&self, args: &[String], limit: Duration) -> Result<CliOutcome, String>;
}

/// Finds the SUT programs the harness deployed among the guest's processes.
pub trait SutProcesses {
    /// Says whether `program`, as deployed by the harness, has a running process, or why that cannot be known.
    fn is_running(&self, program: SutProgram) -> Result<bool, String>;
    /// Starts `program`, as deployed by the harness, with no window of its own, and returns once its process exists.
    fn start(&self, program: SutProgram) -> Result<(), String>;
}

/// Opens test windows through a test-window application and reads where they are.
pub trait TestWindowHost {
    /// Opens the test windows of `batch` and returns their ids once each has been created.
    fn open(&self, batch: &Batch) -> Result<Vec<u32>, String>;
    /// What the OS says of each window handle, in the same order.
    fn presence(&self, handles: &[i64]) -> Vec<PresenceDto>;
    /// Every test window opened so far, in the order of opening, with its frame as the OS reports it now.
    fn windows(&self) -> Result<Vec<TestWindowDto>, String>;
    /// Stops test window `id` from processing messages and returns once it has stopped.
    fn hang(&self, id: u32) -> Result<(), String>;
    /// Lets test window `id` process messages again.
    fn resume(&self, id: u32) -> Result<(), String>;
}

/// Starts and stops load in the guest.
pub trait LoadHost {
    /// Starts a load of `kind` at `percent`, replacing a running one of the same kind.
    fn start_load(&self, kind: LoadKindDto, percent: u8) -> Result<(), String>;
    /// Stops the running load of `kind`, if any.
    fn stop_load(&self, kind: LoadKindDto) -> Result<(), String>;
}

/// Turns the displays of the interactive session off and on.
pub trait DisplayPower {
    /// Turns the displays off, or wakes them with input, and returns once the OS has been asked.
    fn set_display_power(&self, on: bool) -> Result<(), String>;
}

/// The mouse and the foreground window of the interactive session.
pub trait Pointer {
    /// Moves the mouse to `x`, `y`, in virtual-screen coordinates and physical pixels, clicks the left button there,
    /// and returns once the OS has taken the input.
    fn click(&self, x: i32, y: i32) -> Result<(), String>;
    /// The handle of the foreground window, or `None` when no window is in the foreground.
    fn foreground_window(&self) -> Result<Option<i64>, String>;
}

/// Everything the daemon reads from or does in the guest.
pub trait Guest:
    TopologySource + GlazewmCli + SutProcesses + TestWindowHost + LoadHost + DisplayPower + Pointer
{
}

impl<T> Guest for T where
    T: TopologySource
        + GlazewmCli
        + SutProcesses
        + TestWindowHost
        + LoadHost
        + DisplayPower
        + Pointer
{
}

/// Answers one request.
pub fn handle(request: Request, guest: &dyn Guest) -> Response {
    match request {
        Request::Ping => Response::Pong,
        Request::Topology => match guest.displays() {
            Ok(displays) => Response::Topology { displays },
            Err(message) => Response::Error { message },
        },
        Request::QueryGlazewm { query, limit_ms } => {
            let args = query.cli_args().map(String::from);
            match guest.run(&args, Duration::from_millis(limit_ms)) {
                Ok(CliOutcome::Finished(output)) => Response::Output {
                    exit_code: output.exit_code,
                    stdout: output.stdout,
                    stderr: output.stderr,
                },
                Ok(CliOutcome::TimedOut) => Response::TimedOut { limit_ms },
                Err(message) => Response::Error { message },
            }
        }
        Request::StartSut { program } => match guest.start(program) {
            Ok(()) => Response::SutStarted { program },
            Err(message) => Response::Error { message },
        },
        Request::SutRunning { program } => match guest.is_running(program) {
            Ok(running) => Response::Running { running },
            Err(message) => Response::Error { message },
        },
        Request::OpenTestWindows {
            count,
            within_ms,
            lifetime_ms,
        } => match guest.open(&Batch {
            count,
            within: within_ms.map(Duration::from_millis),
            lifetime_ms,
        }) {
            Ok(ids) => Response::TestWindowsOpened { ids },
            Err(message) => Response::Error { message },
        },
        Request::StartLoad { kind, percent } => match guest.start_load(kind, percent) {
            Ok(()) => Response::LoadStarted { kind },
            Err(message) => Response::Error { message },
        },
        Request::StopLoad { kind } => match guest.stop_load(kind) {
            Ok(()) => Response::LoadStopped { kind },
            Err(message) => Response::Error { message },
        },
        Request::WindowPresence { handles } => Response::WindowPresence {
            presences: guest.presence(&handles),
        },
        Request::TestWindows => match guest.windows() {
            Ok(windows) => Response::TestWindows { windows },
            Err(message) => Response::Error { message },
        },
        Request::HangTestWindow { id } => match guest.hang(id) {
            Ok(()) => Response::TestWindowHung { id },
            Err(message) => Response::Error { message },
        },
        Request::ResumeTestWindow { id } => match guest.resume(id) {
            Ok(()) => Response::TestWindowResumed { id },
            Err(message) => Response::Error { message },
        },
        Request::SetDisplayPower { on } => match guest.set_display_power(on) {
            Ok(()) => Response::DisplayPowerSet { on },
            Err(message) => Response::Error { message },
        },
        Request::Click { x, y } => match guest.click(x, y) {
            Ok(()) => Response::Clicked { x, y },
            Err(message) => Response::Error { message },
        },
        Request::ForegroundWindow => match guest.foreground_window() {
            Ok(handle) => Response::ForegroundWindow { handle },
            Err(message) => Response::Error { message },
        },
    }
}

/// The address the daemon listens on: `given`, or else the guest's loopback, which the runner reaches through an
/// SSH port forward and nothing else on the guest's network reaches at all.
pub fn listen_address(given: Option<String>) -> String {
    given.unwrap_or_else(|| "127.0.0.1:7041".into())
}

/// Answers every request line read from `reader` with one response line on `writer`, until end of input.
///
/// A line that is not a valid request is answered with `Response::Error`, and serving continues.
pub fn serve(
    reader: impl BufRead,
    mut writer: impl Write,
    guest: &dyn Guest,
) -> std::io::Result<()> {
    for line in reader.lines() {
        let response = match decode_line::<Request>(&line?) {
            Ok(request) => handle(request, guest),
            Err(error) => Response::Error {
                message: error.to_string(),
            },
        };
        writer.write_all(encode_line(&response).as_bytes())?;
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_windows::Batch;
    use vmtest_protocol::{
        GlazewmQuery, LoadKindDto, PresenceDto, RectDto, SutProgram, TestWindowDto,
    };

    /// A guest whose test-window host answers from fixed results, and whose other parts are unused.
    struct FixedWindows {
        opened: Result<Vec<u32>, String>,
        windows: Vec<TestWindowDto>,
    }

    #[test]
    fn listens_on_the_guests_loopback_when_no_address_is_given() {
        assert_eq!(listen_address(None), "127.0.0.1:7041");
    }

    #[test]
    fn listens_on_the_given_address() {
        assert_eq!(listen_address(Some("0.0.0.0:7041".into())), "0.0.0.0:7041");
    }

    impl TopologySource for FixedWindows {
        fn displays(&self) -> Result<Vec<DisplayDto>, String> {
            Ok(vec![])
        }
    }

    impl GlazewmCli for FixedWindows {
        fn run(&self, _args: &[String], _limit: Duration) -> Result<CliOutcome, String> {
            Err("no glazewm-cli in this fake".into())
        }
    }

    impl SutProcesses for FixedWindows {
        fn is_running(&self, _program: SutProgram) -> Result<bool, String> {
            Err("no processes in this fake".into())
        }
        fn start(&self, _program: SutProgram) -> Result<(), String> {
            Err("no processes in this fake".into())
        }
    }

    impl LoadHost for FixedWindows {
        fn start_load(&self, kind: LoadKindDto, percent: u8) -> Result<(), String> {
            assert_eq!((kind, percent), (LoadKindDto::Cpu, 100));
            Ok(())
        }
        fn stop_load(&self, _kind: LoadKindDto) -> Result<(), String> {
            Err("no load generator in this fake".into())
        }
    }

    impl Pointer for FixedWindows {
        fn click(&self, _x: i32, _y: i32) -> Result<(), String> {
            Ok(())
        }

        fn foreground_window(&self) -> Result<Option<i64>, String> {
            Ok(Some(10))
        }
    }

    impl DisplayPower for FixedWindows {
        fn set_display_power(&self, on: bool) -> Result<(), String> {
            if on {
                Err("no input to wake the displays in this fake".into())
            } else {
                Ok(())
            }
        }
    }

    impl TestWindowHost for FixedWindows {
        fn open(&self, batch: &Batch) -> Result<Vec<u32>, String> {
            assert_eq!(batch, &Batch::one_by_one(2));
            self.opened.clone()
        }
        fn presence(&self, handles: &[i64]) -> Vec<PresenceDto> {
            handles
                .iter()
                .map(|&handle| {
                    if handle == 10 {
                        PresenceDto::TopLevel
                    } else {
                        PresenceDto::Gone
                    }
                })
                .collect()
        }
        fn windows(&self) -> Result<Vec<TestWindowDto>, String> {
            Ok(self.windows.clone())
        }
        fn hang(&self, id: u32) -> Result<(), String> {
            assert_eq!(id, 1);
            Ok(())
        }
        fn resume(&self, id: u32) -> Result<(), String> {
            assert_eq!(id, 1);
            Err("the test-window application closed its connection".into())
        }
    }

    #[test]
    fn answers_what_the_os_says_of_each_window_handle() {
        let guest = FixedWindows {
            opened: Ok(vec![]),
            windows: vec![],
        };

        assert_eq!(
            handle(
                Request::WindowPresence {
                    handles: vec![10, 11]
                },
                &guest
            ),
            Response::WindowPresence {
                presences: vec![PresenceDto::TopLevel, PresenceDto::Gone]
            }
        );
    }

    #[test]
    fn answers_with_the_ids_of_the_test_windows_it_opened() {
        let guest = FixedWindows {
            opened: Ok(vec![0, 1]),
            windows: vec![],
        };

        assert_eq!(
            handle(
                Request::OpenTestWindows {
                    count: 2,
                    within_ms: None,
                    lifetime_ms: None
                },
                &guest
            ),
            Response::TestWindowsOpened { ids: vec![0, 1] }
        );
    }

    #[test]
    fn answers_with_an_error_when_test_windows_cannot_be_opened() {
        let guest = FixedWindows {
            opened: Err("the test-window application closed its connection".into()),
            windows: vec![],
        };

        assert_eq!(
            handle(
                Request::OpenTestWindows {
                    count: 2,
                    within_ms: None,
                    lifetime_ms: None
                },
                &guest
            ),
            Response::Error {
                message: "the test-window application closed its connection".into()
            }
        );
    }

    #[test]
    fn answers_that_a_test_window_has_stopped_or_why_it_cannot_resume() {
        let guest = FixedWindows {
            opened: Ok(vec![]),
            windows: vec![],
        };

        assert_eq!(
            handle(Request::HangTestWindow { id: 1 }, &guest),
            Response::TestWindowHung { id: 1 }
        );
        assert_eq!(
            handle(Request::ResumeTestWindow { id: 1 }, &guest),
            Response::Error {
                message: "the test-window application closed its connection".into()
            }
        );
    }

    #[test]
    fn answers_that_the_displays_were_turned_off_or_why_not() {
        let guest = FixedWindows {
            opened: Ok(vec![]),
            windows: vec![],
        };

        assert_eq!(
            handle(Request::SetDisplayPower { on: false }, &guest),
            Response::DisplayPowerSet { on: false }
        );
        assert_eq!(
            handle(Request::SetDisplayPower { on: true }, &guest),
            Response::Error {
                message: "no input to wake the displays in this fake".into()
            }
        );
    }

    #[test]
    fn answers_that_it_clicked_and_which_window_is_in_the_foreground() {
        let guest = FixedWindows {
            opened: Ok(vec![]),
            windows: vec![],
        };

        assert_eq!(
            handle(Request::Click { x: -640, y: 360 }, &guest),
            Response::Clicked { x: -640, y: 360 }
        );
        assert_eq!(
            handle(Request::ForegroundWindow, &guest),
            Response::ForegroundWindow { handle: Some(10) }
        );
    }

    #[test]
    fn answers_why_it_cannot_click() {
        let guest = FixedProcesses(Ok(false));

        assert_eq!(
            handle(Request::Click { x: 0, y: 0 }, &guest),
            Response::Error {
                message: "no mouse in this fake".into()
            }
        );
    }

    #[test]
    fn answers_with_every_test_window_and_its_frame() {
        let window = TestWindowDto {
            id: 0,
            handle: 10,
            frame: None,
            hung: false,
        };
        let guest = FixedWindows {
            opened: Ok(vec![]),
            windows: vec![window.clone()],
        };

        assert_eq!(
            handle(Request::TestWindows, &guest),
            Response::TestWindows {
                windows: vec![window]
            }
        );
    }

    /// A guest in which glazewm's process is running or not, or starts or not, and topology and `glazewm-cli`
    /// are unused.
    struct FixedProcesses(Result<bool, String>);

    impl TopologySource for FixedProcesses {
        fn displays(&self) -> Result<Vec<DisplayDto>, String> {
            Ok(vec![])
        }
    }

    impl GlazewmCli for FixedProcesses {
        fn run(&self, _args: &[String], _limit: Duration) -> Result<CliOutcome, String> {
            Err("no glazewm-cli in this fake".into())
        }
    }

    impl Pointer for FixedProcesses {
        fn click(&self, _x: i32, _y: i32) -> Result<(), String> {
            Err("no mouse in this fake".into())
        }

        fn foreground_window(&self) -> Result<Option<i64>, String> {
            Err("no mouse in this fake".into())
        }
    }

    impl DisplayPower for FixedProcesses {
        fn set_display_power(&self, _on: bool) -> Result<(), String> {
            Err("no displays in this fake".into())
        }
    }

    impl LoadHost for FixedProcesses {
        fn start_load(&self, kind: LoadKindDto, percent: u8) -> Result<(), String> {
            assert_eq!((kind, percent), (LoadKindDto::Cpu, 100));
            Ok(())
        }
        fn stop_load(&self, _kind: LoadKindDto) -> Result<(), String> {
            Err("no load generator in this fake".into())
        }
    }

    impl TestWindowHost for FixedProcesses {
        fn open(&self, _batch: &Batch) -> Result<Vec<u32>, String> {
            Err("no test windows in this fake".into())
        }
        fn presence(&self, handles: &[i64]) -> Vec<PresenceDto> {
            vec![PresenceDto::Gone; handles.len()]
        }
        fn windows(&self) -> Result<Vec<TestWindowDto>, String> {
            Err("no test windows in this fake".into())
        }
        fn hang(&self, _id: u32) -> Result<(), String> {
            Err("no test windows in this fake".into())
        }
        fn resume(&self, _id: u32) -> Result<(), String> {
            Err("no test windows in this fake".into())
        }
    }

    impl SutProcesses for FixedProcesses {
        fn is_running(&self, program: SutProgram) -> Result<bool, String> {
            assert_eq!(program, SutProgram::Glazewm);
            self.0.clone()
        }
        fn start(&self, program: SutProgram) -> Result<(), String> {
            assert_eq!(program, SutProgram::Glazewm);
            self.0.clone().map(drop)
        }
    }

    #[test]
    fn starts_the_sut_program_and_answers_that_it_has_started() {
        let request = Request::StartSut {
            program: SutProgram::Glazewm,
        };

        assert_eq!(
            handle(request, &FixedProcesses(Ok(true))),
            Response::SutStarted {
                program: SutProgram::Glazewm
            }
        );
    }

    #[test]
    fn answers_with_an_error_when_the_sut_program_cannot_be_started() {
        let request = Request::StartSut {
            program: SutProgram::Glazewm,
        };

        assert_eq!(
            handle(
                request,
                &FixedProcesses(Err("glazewm.exe not found".into()))
            ),
            Response::Error {
                message: "glazewm.exe not found".into()
            }
        );
    }

    #[test]
    fn answers_whether_the_sut_program_is_running() {
        let request = Request::SutRunning {
            program: SutProgram::Glazewm,
        };

        assert_eq!(
            handle(request, &FixedProcesses(Ok(false))),
            Response::Running { running: false }
        );
    }

    #[test]
    fn answers_with_an_error_when_processes_cannot_be_listed() {
        let request = Request::SutRunning {
            program: SutProgram::Glazewm,
        };

        assert_eq!(
            handle(request, &FixedProcesses(Err("snapshot failed".into()))),
            Response::Error {
                message: "snapshot failed".into()
            }
        );
    }

    struct FixedTopology(Result<Vec<DisplayDto>, String>);

    impl TopologySource for FixedTopology {
        fn displays(&self) -> Result<Vec<DisplayDto>, String> {
            self.0.clone()
        }
    }

    impl SutProcesses for FixedTopology {
        fn is_running(&self, _program: SutProgram) -> Result<bool, String> {
            Err("no processes in this fake".into())
        }
        fn start(&self, _program: SutProgram) -> Result<(), String> {
            Err("no processes in this fake".into())
        }
    }

    impl Pointer for FixedTopology {
        fn click(&self, _x: i32, _y: i32) -> Result<(), String> {
            Err("no mouse in this fake".into())
        }

        fn foreground_window(&self) -> Result<Option<i64>, String> {
            Err("no mouse in this fake".into())
        }
    }

    impl DisplayPower for FixedTopology {
        fn set_display_power(&self, _on: bool) -> Result<(), String> {
            Err("no displays in this fake".into())
        }
    }

    impl LoadHost for FixedTopology {
        fn start_load(&self, kind: LoadKindDto, percent: u8) -> Result<(), String> {
            assert_eq!((kind, percent), (LoadKindDto::Cpu, 100));
            Ok(())
        }
        fn stop_load(&self, _kind: LoadKindDto) -> Result<(), String> {
            Err("no load generator in this fake".into())
        }
    }

    impl TestWindowHost for FixedTopology {
        fn open(&self, _batch: &Batch) -> Result<Vec<u32>, String> {
            Err("no test windows in this fake".into())
        }
        fn presence(&self, handles: &[i64]) -> Vec<PresenceDto> {
            vec![PresenceDto::Gone; handles.len()]
        }
        fn windows(&self) -> Result<Vec<TestWindowDto>, String> {
            Err("no test windows in this fake".into())
        }
        fn hang(&self, _id: u32) -> Result<(), String> {
            Err("no test windows in this fake".into())
        }
        fn resume(&self, _id: u32) -> Result<(), String> {
            Err("no test windows in this fake".into())
        }
    }

    impl GlazewmCli for FixedTopology {
        fn run(&self, _args: &[String], _limit: Duration) -> Result<CliOutcome, String> {
            Err("no glazewm-cli in this fake".into())
        }
    }

    /// A guest whose `glazewm-cli` records its arguments and answers with a fixed result.
    struct FakeCli {
        answer: Result<CliOutcome, String>,
        received: std::cell::RefCell<Vec<String>>,
        limit: std::cell::Cell<Duration>,
    }

    impl FakeCli {
        fn answering(answer: Result<CliOutcome, String>) -> Self {
            Self {
                answer,
                received: Default::default(),
                limit: Default::default(),
            }
        }
    }

    impl TopologySource for FakeCli {
        fn displays(&self) -> Result<Vec<DisplayDto>, String> {
            Ok(vec![])
        }
    }

    impl SutProcesses for FakeCli {
        fn is_running(&self, _program: SutProgram) -> Result<bool, String> {
            Err("no processes in this fake".into())
        }
        fn start(&self, _program: SutProgram) -> Result<(), String> {
            Err("no processes in this fake".into())
        }
    }

    impl Pointer for FakeCli {
        fn click(&self, _x: i32, _y: i32) -> Result<(), String> {
            Err("no mouse in this fake".into())
        }

        fn foreground_window(&self) -> Result<Option<i64>, String> {
            Err("no mouse in this fake".into())
        }
    }

    impl DisplayPower for FakeCli {
        fn set_display_power(&self, _on: bool) -> Result<(), String> {
            Err("no displays in this fake".into())
        }
    }

    impl LoadHost for FakeCli {
        fn start_load(&self, kind: LoadKindDto, percent: u8) -> Result<(), String> {
            assert_eq!((kind, percent), (LoadKindDto::Cpu, 100));
            Ok(())
        }
        fn stop_load(&self, _kind: LoadKindDto) -> Result<(), String> {
            Err("no load generator in this fake".into())
        }
    }

    impl TestWindowHost for FakeCli {
        fn open(&self, _batch: &Batch) -> Result<Vec<u32>, String> {
            Err("no test windows in this fake".into())
        }
        fn presence(&self, handles: &[i64]) -> Vec<PresenceDto> {
            vec![PresenceDto::Gone; handles.len()]
        }
        fn windows(&self) -> Result<Vec<TestWindowDto>, String> {
            Err("no test windows in this fake".into())
        }
        fn hang(&self, _id: u32) -> Result<(), String> {
            Err("no test windows in this fake".into())
        }
        fn resume(&self, _id: u32) -> Result<(), String> {
            Err("no test windows in this fake".into())
        }
    }

    impl GlazewmCli for FakeCli {
        fn run(&self, args: &[String], limit: Duration) -> Result<CliOutcome, String> {
            *self.received.borrow_mut() = args.to_vec();
            self.limit.set(limit);
            self.answer.clone()
        }
    }

    #[test]
    fn runs_the_requested_query_through_glazewm_cli() {
        let output = CommandOutput {
            exit_code: Some(0),
            stdout: "{}".into(),
            stderr: String::new(),
        };
        let guest = FakeCli::answering(Ok(CliOutcome::Finished(output)));

        let response = handle(
            Request::QueryGlazewm {
                query: GlazewmQuery::Monitors,
                limit_ms: 2000,
            },
            &guest,
        );

        assert_eq!(*guest.received.borrow(), ["query", "monitors"]);
        assert_eq!(guest.limit.get(), Duration::from_millis(2000));
        assert_eq!(
            response,
            Response::Output {
                exit_code: Some(0),
                stdout: "{}".into(),
                stderr: String::new()
            }
        );
    }

    #[test]
    fn answers_with_an_error_when_glazewm_cli_cannot_start() {
        let guest = FakeCli::answering(Err("glazewm-cli.exe not found".into()));

        let response = handle(
            Request::QueryGlazewm {
                query: GlazewmQuery::Monitors,
                limit_ms: 2000,
            },
            &guest,
        );

        assert_eq!(
            response,
            Response::Error {
                message: "glazewm-cli.exe not found".into()
            }
        );
    }

    #[test]
    fn answers_timed_out_when_glazewm_cli_does_not_finish_within_the_limit() {
        let guest = FakeCli::answering(Ok(CliOutcome::TimedOut));

        let response = handle(
            Request::QueryGlazewm {
                query: GlazewmQuery::Monitors,
                limit_ms: 1500,
            },
            &guest,
        );

        assert_eq!(response, Response::TimedOut { limit_ms: 1500 });
    }

    fn one_display() -> Vec<DisplayDto> {
        let rect = RectDto {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
        };
        vec![DisplayDto {
            device_name: "\\\\.\\DISPLAY1".into(),
            adapter: "Microsoft Hyper-V Video".into(),
            bounds: rect,
            working_area: rect,
            is_primary: true,
        }]
    }

    #[test]
    fn answers_that_a_load_has_started_or_why_it_has_not_stopped() {
        let guest = FixedTopology(Ok(vec![]));

        assert_eq!(
            handle(
                Request::StartLoad {
                    kind: LoadKindDto::Cpu,
                    percent: 100
                },
                &guest
            ),
            Response::LoadStarted {
                kind: LoadKindDto::Cpu
            }
        );
        assert_eq!(
            handle(
                Request::StopLoad {
                    kind: LoadKindDto::Cpu
                },
                &guest
            ),
            Response::Error {
                message: "no load generator in this fake".into()
            }
        );
    }

    #[test]
    fn answers_ping_with_pong() {
        assert_eq!(
            handle(Request::Ping, &FixedTopology(Ok(vec![]))),
            Response::Pong
        );
    }

    #[test]
    fn answers_topology_with_the_current_displays() {
        assert_eq!(
            handle(Request::Topology, &FixedTopology(Ok(one_display()))),
            Response::Topology {
                displays: one_display()
            }
        );
    }

    #[test]
    fn answers_topology_with_an_error_when_displays_cannot_be_read() {
        assert_eq!(
            handle(
                Request::Topology,
                &FixedTopology(Err("EnumDisplayMonitors failed".into()))
            ),
            Response::Error {
                message: "EnumDisplayMonitors failed".into()
            }
        );
    }

    #[test]
    fn answers_each_request_line_in_order() {
        let input = "{\"type\":\"ping\"}\n{\"type\":\"topology\"}\n";
        let mut output = Vec::new();

        serve(
            input.as_bytes(),
            &mut output,
            &FixedTopology(Ok(one_display())),
        )
        .unwrap();

        let expected = encode_line(&Response::Pong)
            + &encode_line(&Response::Topology {
                displays: one_display(),
            });
        assert_eq!(String::from_utf8(output).unwrap(), expected);
    }

    #[test]
    fn keeps_serving_after_an_invalid_line() {
        let input = "not json\n{\"type\":\"ping\"}\n";
        let mut output = Vec::new();

        serve(input.as_bytes(), &mut output, &FixedTopology(Ok(vec![]))).unwrap();

        let lines: Vec<Response> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| decode_line(line).unwrap())
            .collect();
        assert!(matches!(lines[0], Response::Error { .. }));
        assert_eq!(lines[1], Response::Pong);
    }
}
