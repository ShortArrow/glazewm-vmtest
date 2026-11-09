//! Messages between the runner and the guest daemon, and between the guest daemon and the test-window
//! applications: JSON, one object per line.

use std::time::Duration;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// How long a test-window application may take to connect to the guest daemon after it is started, unless the
/// daemon's limits say otherwise.
pub const TEST_WINDOW_CONNECT_LIMIT: Duration = Duration::from_secs(3);

/// How long a new test window may take to become the foreground window before it is reported opened anyway.
pub const TEST_WINDOW_FOREGROUND_LIMIT: Duration = Duration::from_secs(2);

/// How long the guest daemon waits for a test-window application to answer one command, opening a window
/// included.
pub const TEST_WINDOW_ANSWER_LIMIT: Duration = Duration::from_secs(3);

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
    /// Starts the SUT program the harness deployed, with the configuration deployed beside it, in the
    /// interactive session and without a window of its own; answers once the process exists.
    StartSut {
        program: SutProgram,
    },
    /// Asks whether the SUT program the harness deployed is running in the guest.
    SutRunning {
        program: SutProgram,
    },
    /// Asks for the text of each dialog the SUT program the harness deployed has open, as its title and message.
    SutDialogs {
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
    /// Turns the displays off, as the OS does when the user is away, or wakes them with input, as the user does.
    SetDisplayPower {
        on: bool,
    },
    /// Moves the mouse to `x`, `y`, in virtual-screen coordinates and physical pixels, and clicks the left button
    /// there, as the user does.
    Click {
        x: i32,
        y: i32,
    },
    /// Switches which displays the OS uses, as Win+P does; the monitors stay connected.
    SetProjection {
        projection: ProjectionDto,
    },
    /// Presses the keys of `shortcut`, written as glazewm's configuration writes its bindings, such as
    /// `alt+shift+f`, in order, and releases them in reverse, as the user does.
    PressShortcut {
        shortcut: String,
    },
    /// Asks which window is the foreground window.
    ForegroundWindow,
}

impl Request {
    /// How long the guest daemon may spend on this request by design, on top of the time any answer takes.
    ///
    /// Opening test windows may start the test-window application first; the guest daemon is assumed to keep
    /// `TEST_WINDOW_CONNECT_LIMIT` for it.
    pub fn designed_wait(&self) -> Duration {
        let milliseconds = |ms: &u64| Duration::from_millis(*ms);
        match self {
            Self::OpenTestWindows {
                count, within_ms, ..
            } => {
                TEST_WINDOW_CONNECT_LIMIT
                    + within_ms.as_ref().map_or(Duration::ZERO, milliseconds)
                    + TEST_WINDOW_ANSWER_LIMIT * *count
            }
            _ => Duration::ZERO,
        }
    }
}

/// The display modes Win+P offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionDto {
    PcScreenOnly,
    Duplicate,
    Extend,
    SecondScreenOnly,
}

/// The SUT programs the harness deploys into the guest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SutProgram {
    Glazewm,
    Zebar,
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
    SutStarted {
        program: SutProgram,
    },
    Running {
        running: bool,
    },
    /// The text of each dialog, as `title: message`.
    Dialogs {
        texts: Vec<String>,
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
    DisplayPowerSet {
        on: bool,
    },
    Clicked {
        x: i32,
        y: i32,
    },
    ProjectionSet {
        projection: ProjectionDto,
    },
    ShortcutPressed {
        shortcut: String,
    },
    /// The handle of the foreground window, or `None` when no window is in the foreground.
    ForegroundWindow {
        handle: Option<i64>,
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
    fn a_click_names_its_point_in_virtual_screen_coordinates() {
        assert_eq!(
            encode_line(&Request::Click { x: -640, y: 360 }),
            "{\"type\":\"click\",\"x\":-640,\"y\":360}\n"
        );
    }

    #[test]
    fn a_shortcut_is_written_as_glazewm_writes_its_bindings() {
        assert_eq!(
            encode_line(&Request::PressShortcut {
                shortcut: "alt+shift+f".into()
            }),
            "{\"type\":\"press_shortcut\",\"shortcut\":\"alt+shift+f\"}\n"
        );
    }

    #[test]
    fn a_projection_is_named_as_win_p_names_it() {
        assert_eq!(
            encode_line(&Request::SetProjection {
                projection: ProjectionDto::PcScreenOnly
            }),
            "{\"type\":\"set_projection\",\"projection\":\"pc_screen_only\"}\n"
        );
    }

    #[test]
    fn the_foreground_window_survives_a_round_trip() {
        let response = Response::ForegroundWindow { handle: Some(10) };

        let decoded: Response = decode_line(&encode_line(&response)).unwrap();

        assert_eq!(decoded, response);
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
    fn asks_to_start_a_sut_program_by_name_only() {
        let request = Request::StartSut {
            program: SutProgram::Glazewm,
        };

        assert_eq!(
            encode_line(&request),
            "{\"type\":\"start_sut\",\"program\":\"glazewm\"}
"
        );
    }

    #[test]
    fn names_zebar_as_a_sut_program() {
        let request = Request::StartSut {
            program: SutProgram::Zebar,
        };

        assert_eq!(
            encode_line(&request),
            "{\"type\":\"start_sut\",\"program\":\"zebar\"}
"
        );
    }

    #[test]
    fn a_started_answer_survives_a_round_trip() {
        let response = Response::SutStarted {
            program: SutProgram::Glazewm,
        };

        let decoded: Response = decode_line(&encode_line(&response)).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn a_running_answer_survives_a_round_trip() {
        let response = Response::Running { running: false };

        let decoded: Response = decode_line(&encode_line(&response)).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn allows_test_windows_opened_one_by_one_to_start_the_application_and_wait_for_each_foreground()
    {
        let request = Request::OpenTestWindows {
            count: 2,
            within_ms: None,
            lifetime_ms: None,
        };

        assert_eq!(request.designed_wait(), Duration::from_secs(3 + 2 * 3));
    }

    #[test]
    fn allows_test_windows_opened_within_a_span_to_start_the_application_and_take_the_span() {
        let request = Request::OpenTestWindows {
            count: 20,
            within_ms: Some(500),
            lifetime_ms: Some(0),
        };

        assert_eq!(
            request.designed_wait(),
            Duration::from_millis(3_000 + 500 + 20 * 3_000)
        );
    }

    #[test]
    fn allows_a_topology_request_no_designed_wait() {
        assert_eq!(Request::Topology.designed_wait(), Duration::ZERO);
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
    fn asks_to_turn_the_displays_off_and_answers_that_they_are() {
        assert_eq!(
            encode_line(&Request::SetDisplayPower { on: false }),
            "{\"type\":\"set_display_power\",\"on\":false}
"
        );
        assert_eq!(
            decode_line::<Response>(r#"{"type":"display_power_set","on":false}"#).unwrap(),
            Response::DisplayPowerSet { on: false }
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
