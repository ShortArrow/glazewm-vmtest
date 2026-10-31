//! Messages between the runner and the guest daemon, and between the guest daemon and the test-window
//! applications: JSON, one object per line.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// What the runner asks the guest daemon for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Topology,
    /// Runs a read-only `glazewm-cli query` in the interactive session, stopping it after `limit_ms`.
    ///
    /// Only queries are expressible: free-form arguments would let anyone reaching the port run
    /// `glazewm-cli command shell-exec` and so any program in the guest.
    QueryGlazewm {
        query: GlazewmQuery,
        limit_ms: u64,
    },
    /// Asks whether the SUT program the harness deployed is running in the guest.
    SutRunning {
        program: SutProgram,
    },
    /// Opens `count` more test windows and answers once each has been created.
    ///
    /// Without `within_ms`, each is opened once the one before is the foreground window; with it, they are
    /// opened at even intervals within that many milliseconds. Without `lifetime_ms` they persist; with it,
    /// each is destroyed that many milliseconds after its creation.
    OpenTestWindows {
        count: u32,
        within_ms: Option<u64>,
        lifetime_ms: Option<u64>,
    },
    /// Asks what the OS says of each window handle, in the same order.
    WindowPresence {
        handles: Vec<i64>,
    },
    /// Asks for every test window opened so far, with its frame as the OS reports it.
    TestWindows,
    /// Stops test window `id` from processing messages and answers once it has stopped.
    HangTestWindow {
        id: u32,
    },
    /// Lets test window `id` process messages again.
    ResumeTestWindow {
        id: u32,
    },
    /// Starts a load of `kind` at `percent`, from 1 to 100, replacing a running one of the same kind.
    StartLoad {
        kind: LoadKindDto,
        percent: u8,
    },
    /// Stops the running load of `kind`, if any.
    StopLoad {
        kind: LoadKindDto,
    },
}

/// The SUT programs the harness deploys into the guest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SutProgram {
    Glazewm,
}

/// The `glazewm-cli query` subjects the runner reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlazewmQuery {
    Monitors,
}

impl GlazewmQuery {
    /// The `glazewm-cli` arguments that perform this query.
    pub fn cli_args(self) -> [&'static str; 2] {
        match self {
            Self::Monitors => ["query", "monitors"],
        }
    }
}

/// What the guest daemon answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong,
    Topology {
        displays: Vec<DisplayDto>,
    },
    /// What a command printed and how it ended; `exit_code` is `None` when it was terminated.
    Output {
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
    },
    /// The command had not finished after `limit_ms`, measured in the guest, and was terminated.
    TimedOut {
        limit_ms: u64,
    },
    Running {
        running: bool,
    },
    TestWindowsOpened {
        ids: Vec<u32>,
    },
    TestWindows {
        windows: Vec<TestWindowDto>,
    },
    WindowPresence {
        presences: Vec<PresenceDto>,
    },
    LoadStarted {
        kind: LoadKindDto,
    },
    LoadStopped {
        kind: LoadKindDto,
    },
    TestWindowHung {
        id: u32,
    },
    TestWindowResumed {
        id: u32,
    },
    Error {
        message: String,
    },
}

/// A rectangle in virtual-screen coordinates, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RectDto {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// One display as the guest OS reports it to the daemon.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayDto {
    pub device_name: String,
    /// The display adapter that drives it, as `EnumDisplayDevices` names it.
    pub adapter: String,
    pub bounds: RectDto,
    pub working_area: RectDto,
    pub is_primary: bool,
}

/// One test window as the guest daemon reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestWindowDto {
    /// The number the guest daemon gave it, counting from 0 in the order of opening.
    pub id: u32,
    /// The window handle, which glazewm reports for the windows it manages.
    pub handle: i64,
    /// The visible frame, in physical pixels, or `None` when the window no longer exists.
    pub frame: Option<RectDto>,
    /// Whether its message processing has been stopped.
    pub hung: bool,
}

/// What a load uses up: every logical processor, or the physical memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadKindDto {
    Cpu,
    Memory,
}

/// Whether a window exists, and whether it is a top-level window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceDto {
    TopLevel,
    Child,
    Gone,
}

/// What the guest daemon tells a test-window application to do.
///
/// Every test-window application, whatever its UI framework, connects to the address given as its only
/// argument and reads these, one per line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppCommand {
    /// Opens a normal top-level window, persistent and responsive, identified by `id`.
    /// Opens window `id`, answering once it is created, or once it is the foreground window when
    /// `wait_for_foreground` is set; with `lifetime_ms`, the window destroys itself that long after creation.
    Open {
        id: u32,
        wait_for_foreground: bool,
        lifetime_ms: Option<u64>,
    },
    /// Stops window `id`'s thread from processing messages until `Resume`.
    Hang { id: u32 },
    /// Lets window `id`'s thread process messages again.
    Resume { id: u32 },
}

/// What a test-window application answers, one line per command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppReply {
    /// The window exists, with this handle.
    Opened {
        id: u32,
        handle: i64,
    },
    /// The window's thread has stopped processing messages.
    Hung {
        id: u32,
    },
    Resumed {
        id: u32,
    },
    Error {
        message: String,
    },
}

/// A line that is not a valid message.
#[derive(Debug)]
pub struct DecodeError(serde_json::Error);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid message: {}", self.0)
    }
}

impl std::error::Error for DecodeError {}

/// Encodes a message as one line of JSON, ending with a single newline.
///
/// The message types hold no maps with non-string keys, so encoding cannot fail.
pub fn encode_line<T: Serialize>(message: &T) -> String {
    let mut line = serde_json::to_string(message).expect("protocol messages always encode to JSON");
    line.push('\n');
    line
}

/// Decodes one line of JSON, with or without its trailing newline.
pub fn decode_line<T: DeserializeOwned>(line: &str) -> Result<T, DecodeError> {
    serde_json::from_str(line.trim_end_matches(['\r', '\n'])).map_err(DecodeError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_one_line_of_json() {
        assert_eq!(encode_line(&Request::Topology), "{\"type\":\"topology\"}\n");
    }

    #[test]
    fn a_topology_response_survives_a_round_trip() {
        let rect = RectDto {
            x: 1280,
            y: 0,
            width: 800,
            height: 600,
        };
        let response = Response::Topology {
            displays: vec![DisplayDto {
                device_name: "\\\\.\\DISPLAY2".into(),
                adapter: "Virtual Display Driver".into(),
                bounds: rect,
                working_area: rect,
                is_primary: false,
            }],
        };

        let decoded: Response = decode_line(&encode_line(&response)).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn a_glazewm_query_carries_its_time_limit_in_milliseconds() {
        let request = Request::QueryGlazewm {
            query: GlazewmQuery::Monitors,
            limit_ms: 2000,
        };

        assert_eq!(
            encode_line(&request),
            "{\"type\":\"query_glazewm\",\"query\":\"monitors\",\"limit_ms\":2000}
"
        );
    }

    #[test]
    fn a_timed_out_query_survives_a_round_trip() {
        let response = Response::TimedOut { limit_ms: 2000 };

        let decoded: Response = decode_line(&encode_line(&response)).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn asks_whether_a_sut_program_is_running_by_name_only() {
        let request = Request::SutRunning {
            program: SutProgram::Glazewm,
        };

        assert_eq!(
            encode_line(&request),
            "{\"type\":\"sut_running\",\"program\":\"glazewm\"}
"
        );
    }

    #[test]
    fn a_running_answer_survives_a_round_trip() {
        let response = Response::Running { running: false };

        let decoded: Response = decode_line(&encode_line(&response)).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn asks_for_test_windows_by_count_span_and_lifetime() {
        let request = Request::OpenTestWindows {
            count: 3,
            within_ms: Some(200),
            lifetime_ms: None,
        };

        assert_eq!(
            encode_line(&request),
            r#"{"type":"open_test_windows","count":3,"within_ms":200,"lifetime_ms":null}"#
                .to_owned()
                + "\n"
        );
    }

    #[test]
    fn a_test_windows_answer_survives_a_round_trip() {
        let response = Response::TestWindows {
            windows: vec![
                TestWindowDto {
                    id: 0,
                    handle: 0x1234,
                    frame: Some(RectDto {
                        x: 20,
                        y: 20,
                        width: 600,
                        height: 400,
                    }),
                    hung: false,
                },
                TestWindowDto {
                    id: 1,
                    handle: 0x5678,
                    frame: None,
                    hung: true,
                },
            ],
        };

        let decoded: Response = decode_line(&encode_line(&response)).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn starts_and_stops_a_load_by_kind() {
        assert_eq!(
            encode_line(&Request::StartLoad {
                kind: LoadKindDto::Memory,
                percent: 70
            }),
            r#"{"type":"start_load","kind":"memory","percent":70}"#.to_owned() + "\n"
        );
        assert_eq!(
            decode_line::<Response>(r#"{"type":"load_stopped","kind":"cpu"}"#).unwrap(),
            Response::LoadStopped {
                kind: LoadKindDto::Cpu
            }
        );
    }

    #[test]
    fn asks_to_stop_and_resume_a_test_window_by_id() {
        assert_eq!(
            encode_line(&Request::HangTestWindow { id: 1 }),
            "{\"type\":\"hang_test_window\",\"id\":1}
"
        );
        assert_eq!(
            encode_line(&Request::ResumeTestWindow { id: 1 }),
            "{\"type\":\"resume_test_window\",\"id\":1}
"
        );
    }

    #[test]
    fn tells_a_test_window_application_to_stop_and_resume_a_window() {
        assert_eq!(
            encode_line(&AppCommand::Hang { id: 2 }),
            "{\"type\":\"hang\",\"id\":2}
"
        );
        assert_eq!(
            decode_line::<AppReply>("{\"type\":\"resumed\",\"id\":2}").unwrap(),
            AppReply::Resumed { id: 2 }
        );
    }

    #[test]
    fn tells_a_test_window_application_how_to_open_a_window() {
        let command = AppCommand::Open {
            id: 4,
            wait_for_foreground: false,
            lifetime_ms: Some(0),
        };

        assert_eq!(
            encode_line(&command),
            r#"{"type":"open","id":4,"wait_for_foreground":false,"lifetime_ms":0}"#.to_owned()
                + "\n"
        );
    }

    #[test]
    fn asks_what_the_os_says_of_window_handles() {
        assert_eq!(
            encode_line(&Request::WindowPresence {
                handles: vec![10, 11]
            }),
            r#"{"type":"window_presence","handles":[10,11]}"#.to_owned() + "\n"
        );
        let response = Response::WindowPresence {
            presences: vec![PresenceDto::TopLevel, PresenceDto::Child, PresenceDto::Gone],
        };
        assert_eq!(
            encode_line(&response),
            r#"{"type":"window_presence","presences":["top_level","child","gone"]}"#.to_owned()
                + "\n"
        );
    }

    #[test]
    fn a_test_window_application_reports_the_handle_of_an_opened_window() {
        let line = "{\"type\":\"opened\",\"id\":4,\"handle\":4660}";

        assert_eq!(
            decode_line::<AppReply>(line).unwrap(),
            AppReply::Opened {
                id: 4,
                handle: 4660
            }
        );
    }

    #[test]
    fn a_request_carrying_a_command_line_is_rejected() {
        let line = r#"{"type":"run_glazewm_cli","args":["command","shell-exec","calc"]}"#;

        assert!(decode_line::<Request>(line).is_err());
    }

    #[test]
    fn an_unknown_request_type_is_an_error() {
        assert!(decode_line::<Request>("{\"type\":\"reboot\"}").is_err());
    }
}
