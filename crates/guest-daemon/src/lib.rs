//! The guest daemon: answers the runner's requests from inside the guest's interactive session.

#[cfg(windows)]
pub mod win32;

use std::io::{BufRead, Write};

use vmtest_protocol::{DisplayDto, Request, Response, decode_line, encode_line};

/// Reads the displays the guest OS currently reports.
pub trait TopologySource {
    /// Returns the current displays, or why they could not be read.
    fn displays(&self) -> Result<Vec<DisplayDto>, String>;
}

/// Answers one request.
pub fn handle(request: Request, topology: &dyn TopologySource) -> Response {
    match request {
        Request::Ping => Response::Pong,
        Request::Topology => match topology.displays() {
            Ok(displays) => Response::Topology { displays },
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
    topology: &dyn TopologySource,
) -> std::io::Result<()> {
    for line in reader.lines() {
        let response = match decode_line::<Request>(&line?) {
            Ok(request) => handle(request, topology),
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
    use vmtest_protocol::RectDto;

    struct FixedTopology(Result<Vec<DisplayDto>, String>);

    impl TopologySource for FixedTopology {
        fn displays(&self) -> Result<Vec<DisplayDto>, String> {
            self.0.clone()
        }
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
