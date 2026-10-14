//! The Win32 test-window application: opens plain Win32 windows when the guest daemon says so.

#[cfg(windows)]
pub mod window;

use std::io::{BufRead, Write};

use vmtest_protocol::{AppCommand, AppReply, decode_line, encode_line};

/// Carries out every command read from `reader`, answering each with one reply on `writer`, until end of input.
///
/// `open` opens window `id` and returns its handle once the window exists.
pub fn serve(
    reader: impl BufRead,
    mut writer: impl Write,
    mut open: impl FnMut(u32) -> Result<i64, String>,
) -> std::io::Result<()> {
    for line in reader.lines() {
        let reply = match decode_line::<AppCommand>(&line?) {
            Ok(AppCommand::Open { id }) => match open(id) {
                Ok(handle) => AppReply::Opened { id, handle },
                Err(message) => AppReply::Error { message },
            },
            Err(error) => AppReply::Error {
                message: error.to_string(),
            },
        };
        writer.write_all(encode_line(&reply).as_bytes())?;
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(commands: &[AppCommand], open: impl FnMut(u32) -> Result<i64, String>) -> Vec<AppReply> {
        let input: String = commands.iter().map(encode_line).collect();
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output, open).unwrap();
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
            |id| Ok(100 + i64::from(id)),
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
    fn answers_an_open_that_failed_with_its_error_and_goes_on() {
        let replies = run(
            &[AppCommand::Open { id: 0 }, AppCommand::Open { id: 1 }],
            |id| {
                if id == 0 {
                    Err("CreateWindowExW failed".into())
                } else {
                    Ok(5)
                }
            },
        );

        assert_eq!(
            replies,
            [
                AppReply::Error {
                    message: "CreateWindowExW failed".into()
                },
                AppReply::Opened { id: 1, handle: 5 }
            ]
        );
    }

    #[test]
    fn answers_a_line_it_cannot_read_with_an_error() {
        let mut output = Vec::new();

        serve("not json\n".as_bytes(), &mut output, |_| Ok(0)).unwrap();

        let reply: AppReply = decode_line(std::str::from_utf8(&output).unwrap()).unwrap();
        assert!(matches!(reply, AppReply::Error { .. }));
    }
}
