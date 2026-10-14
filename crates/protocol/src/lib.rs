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
    /// Opens `count` more test windows and answers once each exists.
    OpenTestWindows {
        count: u32,
    },
    /// Asks for every test window opened so far, with its frame as the OS reports it.
    TestWindows,
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
}

/// What the guest daemon tells a test-window application to do.
///
/// Every test-window application, whatever its UI framework, connects to the address given as its only
/// argument and reads these, one per line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppCommand {
    /// Opens a normal top-level window, persistent and responsive, identified by `id`.
    Open { id: u32 },
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
    fn asks_for_a_number_of_test_windows() {
        assert_eq!(
            encode_line(&Request::OpenTestWindows { count: 3 }),
            "{\"type\":\"open_test_windows\",\"count\":3}
"
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
                },
                TestWindowDto {
                    id: 1,
                    handle: 0x5678,
                    frame: None,
                },
            ],
        };

        let decoded: Response = decode_line(&encode_line(&response)).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn tells_a_test_window_application_to_open_a_window_by_id() {
        assert_eq!(
            encode_line(&AppCommand::Open { id: 4 }),
            "{\"type\":\"open\",\"id\":4}
"
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
