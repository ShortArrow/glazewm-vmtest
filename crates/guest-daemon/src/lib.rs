//! The guest daemon: answers the runner's requests from inside the guest's interactive session.

#[cfg(windows)]
pub mod win32;

pub mod process;
pub mod test_windows;

use std::io::{BufRead, Write};
use std::time::Duration;

use vmtest_protocol::{
    DisplayDto, Request, Response, SutProgram, TestWindowDto, decode_line, encode_line,
};

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
}

/// Opens test windows through a test-window application and reads where they are.
pub trait TestWindowHost {
    /// Opens `count` more test windows and returns their ids once each exists.
    fn open(&self, count: u32) -> Result<Vec<u32>, String>;
    /// Every test window opened so far, in the order of opening, with its frame as the OS reports it now.
    fn windows(&self) -> Result<Vec<TestWindowDto>, String>;
    /// Stops test window `id` from processing messages and returns once it has stopped.
    fn hang(&self, id: u32) -> Result<(), String>;
    /// Lets test window `id` process messages again.
    fn resume(&self, id: u32) -> Result<(), String>;
}

/// Everything the daemon reads from or does in the guest.
pub trait Guest: TopologySource + GlazewmCli + SutProcesses + TestWindowHost {}

impl<T: TopologySource + GlazewmCli + SutProcesses + TestWindowHost> Guest for T {}

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
        Request::SutRunning { program } => match guest.is_running(program) {
            Ok(running) => Response::Running { running },
            Err(message) => Response::Error { message },
        },
        Request::OpenTestWindows { count } => match guest.open(count) {
            Ok(ids) => Response::TestWindowsOpened { ids },
            Err(message) => Response::Error { message },
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
    }
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
    use vmtest_protocol::{GlazewmQuery, RectDto, SutProgram, TestWindowDto};

    /// A guest whose test-window host answers from fixed results, and whose other parts are unused.
    struct FixedWindows {
        opened: Result<Vec<u32>, String>,
        windows: Vec<TestWindowDto>,
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
    }

    impl TestWindowHost for FixedWindows {
        fn open(&self, count: u32) -> Result<Vec<u32>, String> {
            assert_eq!(count, 2);
            self.opened.clone()
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
    fn answers_with_the_ids_of_the_test_windows_it_opened() {
        let guest = FixedWindows {
            opened: Ok(vec![0, 1]),
            windows: vec![],
        };

        assert_eq!(
            handle(Request::OpenTestWindows { count: 2 }, &guest),
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
            handle(Request::OpenTestWindows { count: 2 }, &guest),
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

    /// A guest in which glazewm's process is running or not, and topology and `glazewm-cli` are unused.
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

    impl TestWindowHost for FixedProcesses {
        fn open(&self, _count: u32) -> Result<Vec<u32>, String> {
            Err("no test windows in this fake".into())
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
    }

    impl TestWindowHost for FixedTopology {
        fn open(&self, _count: u32) -> Result<Vec<u32>, String> {
            Err("no test windows in this fake".into())
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
    }

    impl TestWindowHost for FakeCli {
        fn open(&self, _count: u32) -> Result<Vec<u32>, String> {
            Err("no test windows in this fake".into())
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
            bounds: rect,
            working_area: rect,
            is_primary: true,
        }]
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
