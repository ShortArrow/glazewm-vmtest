//! The runner's side of the connection to the guest daemon.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use vmtest_protocol::{DecodeError, Request, Response, decode_line, encode_line};

/// Why a request got no usable response.
#[derive(Debug)]
pub enum ClientError {
    Io(std::io::Error),
    Closed,
    Decode(DecodeError),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "connection to the guest daemon failed: {error}"),
            Self::Closed => write!(f, "the guest daemon closed the connection before answering"),
            Self::Decode(error) => write!(f, "the guest daemon answered with {error}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<std::io::Error> for ClientError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// A connection to the guest daemon over any byte stream.
pub struct GuestDaemonClient<S: Read + Write> {
    reader: BufReader<S>,
}

impl<S: Read + Write> GuestDaemonClient<S> {
    /// Wraps an established stream.
    pub fn new(stream: S) -> Self {
        Self {
            reader: BufReader::new(stream),
        }
    }

    /// Sends one request and waits for its response.
    pub fn request(&mut self, request: &Request) -> Result<Response, ClientError> {
        let stream = self.reader.get_mut();
        stream.write_all(encode_line(request).as_bytes())?;
        stream.flush()?;
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(ClientError::Closed);
        }
        decode_line(&line).map_err(ClientError::Decode)
    }
}

impl GuestDaemonClient<TcpStream> {
    /// Connects to the guest daemon at `address`, failing any connect, read, or write that takes longer than `timeout`.
    pub fn connect(address: impl ToSocketAddrs, timeout: Duration) -> Result<Self, ClientError> {
        let address = address
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no address"))?;
        let stream = TcpStream::connect_timeout(&address, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(Self::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A stream that answers with fixed bytes and records what was written.
    struct ScriptedStream {
        answers: Cursor<Vec<u8>>,
        written: Vec<u8>,
    }

    impl ScriptedStream {
        fn answering(answers: &str) -> Self {
            Self {
                answers: Cursor::new(answers.as_bytes().to_vec()),
                written: Vec::new(),
            }
        }
    }

    impl Read for ScriptedStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.answers.read(buf)
        }
    }

    impl Write for ScriptedStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn sends_the_request_as_one_line_and_reads_the_response_line() {
        let mut client = GuestDaemonClient::new(ScriptedStream::answering("{\"type\":\"pong\"}\n"));

        let response = client.request(&Request::Ping).unwrap();

        assert_eq!(response, Response::Pong);
        assert_eq!(client.reader.get_ref().written, b"{\"type\":\"ping\"}\n");
    }

    #[test]
    fn reports_a_connection_closed_before_the_response() {
        let mut client = GuestDaemonClient::new(ScriptedStream::answering(""));

        assert!(matches!(
            client.request(&Request::Ping),
            Err(ClientError::Closed)
        ));
    }

    #[test]
    fn reports_a_response_that_is_not_a_message() {
        let mut client = GuestDaemonClient::new(ScriptedStream::answering("garbage\n"));

        assert!(matches!(
            client.request(&Request::Ping),
            Err(ClientError::Decode(_))
        ));
    }
}
