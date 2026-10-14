//! The Win32 test-window application: opens plain Win32 windows when the guest daemon says so.

pub mod gate;
#[cfg(windows)]
pub mod window;

use std::io::{BufRead, Write};

use vmtest_protocol::{AppCommand, AppReply, decode_line, encode_line};

/// What the application does to its windows.
pub trait WindowHost {
    /// Opens window `id` and returns its handle once the window exists.
    fn open(&mut self, id: u32) -> Result<i64, String>;
    /// Stops window `id` from processing messages and returns once it has stopped.
    fn hang(&mut self, id: u32) -> Result<(), String>;
    /// Lets window `id` process messages again.
    fn resume(&mut self, id: u32) -> Result<(), String>;
}

/// Carries out every command read from `reader`, answering each with one reply on `writer`, until end of input.
pub fn serve(
    reader: impl BufRead,
    mut writer: impl Write,
    windows: &mut impl WindowHost,
) -> std::io::Result<()> {
    for line in reader.lines() {
        let reply = match decode_line::<AppCommand>(&line?) {
            Ok(command) => carry_out(command, windows),
            Err(error) => AppReply::Error {
                message: error.to_string(),
            },
        };
        writer.write_all(encode_line(&reply).as_bytes())?;
        writer.flush()?;
    }
    Ok(())
}

fn carry_out(command: AppCommand, windows: &mut impl WindowHost) -> AppReply {
    let done = match command {
        AppCommand::Open { id } => windows
            .open(id)
            .map(|handle| AppReply::Opened { id, handle }),
        AppCommand::Hang { id } => windows.hang(id).map(|()| AppReply::Hung { id }),
        AppCommand::Resume { id } => windows.resume(id).map(|()| AppReply::Resumed { id }),
    };
    done.unwrap_or_else(|message| AppReply::Error { message })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows that open with handle 100 + id, except `failing`, and that record what was done to them.
    #[derive(Default)]
    struct FakeWindows {
        failing: Option<u32>,
        done: Vec<String>,
    }

    impl WindowHost for FakeWindows {
        fn open(&mut self, id: u32) -> Result<i64, String> {
            if self.failing == Some(id) {
                return Err("CreateWindowExW failed".into());
            }
            self.done.push(format!("open {id}"));
            Ok(100 + i64::from(id))
        }
        fn hang(&mut self, id: u32) -> Result<(), String> {
            if self.failing == Some(id) {
                return Err(format!("window {id} is already hung"));
            }
            self.done.push(format!("hang {id}"));
            Ok(())
        }
        fn resume(&mut self, id: u32) -> Result<(), String> {
            self.done.push(format!("resume {id}"));
            Ok(())
        }
    }

    fn run(commands: &[AppCommand], windows: &mut FakeWindows) -> Vec<AppReply> {
        let input: String = commands.iter().map(encode_line).collect();
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output, windows).unwrap();
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| decode_line(line).unwrap())
            .collect()
    }

    #[test]
    fn answers_each_open_with_the_handle_of_the_new_window() {
        let replies = run(
            &[AppCommand::Open { id: 0 }, AppCommand::Open { id: 1 }],
            &mut FakeWindows::default(),
        );

        assert_eq!(
            replies,
            [
                AppReply::Opened { id: 0, handle: 100 },
                AppReply::Opened { id: 1, handle: 101 }
            ]
        );
    }

    #[test]
    fn stops_and_resumes_a_window_in_the_order_told() {
        let mut windows = FakeWindows::default();

        let replies = run(
            &[
                AppCommand::Open { id: 0 },
                AppCommand::Hang { id: 0 },
                AppCommand::Resume { id: 0 },
            ],
            &mut windows,
        );

        assert_eq!(
            replies[1..],
            [AppReply::Hung { id: 0 }, AppReply::Resumed { id: 0 }]
        );
        assert_eq!(windows.done, ["open 0", "hang 0", "resume 0"]);
    }

    #[test]
    fn answers_a_command_that_failed_with_its_error_and_goes_on() {
        let replies = run(
            &[AppCommand::Open { id: 0 }, AppCommand::Open { id: 1 }],
            &mut FakeWindows {
                failing: Some(0),
                ..Default::default()
            },
        );

        assert_eq!(
            replies,
            [
                AppReply::Error {
                    message: "CreateWindowExW failed".into()
                },
                AppReply::Opened { id: 1, handle: 101 }
            ]
        );
    }

    #[test]
    fn answers_a_line_it_cannot_read_with_an_error() {
        let mut output = Vec::new();

        serve(
            "not json\n".as_bytes(),
            &mut output,
            &mut FakeWindows::default(),
        )
        .unwrap();

        let reply: AppReply = decode_line(std::str::from_utf8(&output).unwrap()).unwrap();
        assert!(matches!(reply, AppReply::Error { .. }));
    }
}
