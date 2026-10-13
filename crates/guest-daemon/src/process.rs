//! Runs the SUT's command-line client as a child process.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::{CliOutcome, CommandOutput, GlazewmCli};

/// Runs the `glazewm-cli` executable at a fixed path.
pub struct ProcessGlazewmCli {
    pub executable: PathBuf,
}

/// How often a running child is checked for having exited.
const EXIT_POLL: Duration = Duration::from_millis(20);

impl GlazewmCli for ProcessGlazewmCli {
    fn run(&self, args: &[String], limit: Duration) -> Result<CliOutcome, String> {
        let mut command = Command::new(&self.executable);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            /// `CREATE_NO_WINDOW`: a console child of a windowless daemon would otherwise open a console window on the desktop under test.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("cannot start {}: {error}", self.executable.display()))?;
        let stdout = read_to_end(child.stdout.take());
        let stderr = read_to_end(child.stderr.take());
        let Some(exit_code) = wait_within(&mut child, limit)? else {
            return Ok(CliOutcome::TimedOut);
        };
        Ok(CliOutcome::Finished(CommandOutput {
            exit_code,
            stdout: joined(stdout),
            stderr: joined(stderr),
        }))
    }
}

/// Waits for `child` to exit and returns its exit code; `None` when `limit` passed first and the child was killed.
fn wait_within(child: &mut Child, limit: Duration) -> Result<Option<Option<i32>>, String> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Ok(Some(status.code()));
        }
        if started.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(EXIT_POLL);
    }
}

/// Drains a pipe on its own thread, so that a child filling it cannot block on writing.
fn read_to_end(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn joined(reader: JoinHandle<String>) -> String {
    reader.join().unwrap_or_default()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn powershell(script: &str, limit: Duration) -> Result<CliOutcome, String> {
        let cli = ProcessGlazewmCli {
            executable: "powershell.exe".into(),
        };
        cli.run(
            &["-NoProfile".into(), "-Command".into(), script.into()],
            limit,
        )
    }

    #[test]
    fn returns_what_a_command_printed_when_it_finishes_within_the_limit() {
        let outcome = powershell("Write-Output ok; exit 3", Duration::from_secs(30)).unwrap();

        let CliOutcome::Finished(output) = outcome else {
            panic!("expected the command to finish, got {outcome:?}");
        };
        assert_eq!(output.exit_code, Some(3));
        assert_eq!(output.stdout.trim(), "ok");
    }

    #[test]
    fn terminates_a_command_still_running_at_the_limit() {
        let started = Instant::now();

        let outcome = powershell("Start-Sleep -Seconds 30", Duration::from_millis(500)).unwrap();

        assert_eq!(outcome, CliOutcome::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
