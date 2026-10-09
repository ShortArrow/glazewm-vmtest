//! Messages between the runner and the guest daemon: JSON, one object per line.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// What the runner asks the guest daemon for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Topology,
}

/// What the guest daemon answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong,
    Topology { displays: Vec<DisplayDto> },
    Error { message: String },
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
    fn an_unknown_request_type_is_an_error() {
        assert!(decode_line::<Request>("{\"type\":\"reboot\"}").is_err());
    }
}
