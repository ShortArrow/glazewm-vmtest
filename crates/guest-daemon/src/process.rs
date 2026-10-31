//! Runs the SUT programs and the SUT's command-line client as child processes.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use vmtest_protocol::SutProgram;

use crate::{CliOutcome, CommandOutput, GlazewmCli};

/// Runs the `glazewm-cli` executable at a fixed path.
pub struct ProcessGlazewmCli {
    pub executable: PathBuf,
}

/// A SUT program the harness deployed into a directory named after it: glazewm with the `config.yaml` there,
/// zebar with the `config` directory there. Started, it writes its output to `stdout.log` and `stderr.log` there,
/// with a backtrace if it panics.
pub struct DeployedSut {
    directory: PathBuf,
    program: SutProgram,
}

impl DeployedSut {
    /// The `program` deployed under `base`.
    pub fn new(base: &Path, program: SutProgram) -> Self {
        let name = match program {
            SutProgram::Glazewm => "glazewm",
            SutProgram::Zebar => "zebar",
        };
        Self {
            directory: base.join(name),
            program,
        }
    }

    pub fn executable(&self) -> PathBuf {
        self.directory.join(match self.program {
            SutProgram::Glazewm => "glazewm.exe",
            SutProgram::Zebar => "zebar.exe",
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(self.executable());
        match self.program {
            SutProgram::Glazewm => command
                .arg("start")
                .arg("--config")
                .arg(self.directory.join("config.yaml")),
            SutProgram::Zebar => command
                .arg("startup")
                .arg("--config-dir")
                .arg(self.directory.join("config")),
        };
        command
            .current_dir(&self.directory)
            .env("RUST_BACKTRACE", "1");
        without_window(&mut command);
        command
    }

    /// Starts the program and returns once its process exists, leaving it running.
    pub fn start(&self) -> Result<(), String> {
        let log = |name: &str| {
            let path = self.directory.join(name);
            File::create(&path)
                .map_err(|error| format!("cannot create {}: {error}", path.display()))
        };
        self.command()
            .stdin(Stdio::null())
            .stdout(log("stdout.log")?)
            .stderr(log("stderr.log")?)
            .spawn()
            .map(drop)
            .map_err(|error| format!("cannot start {}: {error}", self.executable().display()))
    }
}

/// Keeps a console child of the windowless daemon from opening a console window on the desktop under test.
fn without_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
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
        without_window(&mut command);
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

    fn arguments(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn starts_glazewm_with_the_configuration_file_beside_it() {
        let command = DeployedSut::new(Path::new(r"C:\vmtest"), SutProgram::Glazewm).command();

        assert_eq!(command.get_program(), r"C:\vmtest\glazewm\glazewm.exe");
        assert_eq!(
            arguments(&command),
            ["start", "--config", r"C:\vmtest\glazewm\config.yaml"]
        );
    }

    #[test]
    fn starts_zebar_with_the_configuration_directory_beside_it() {
        let command = DeployedSut::new(Path::new(r"C:\vmtest"), SutProgram::Zebar).command();

        assert_eq!(command.get_program(), r"C:\vmtest\zebar\zebar.exe");
        assert_eq!(
            arguments(&command),
            ["startup", "--config-dir", r"C:\vmtest\zebar\config"]
        );
    }

    #[test]
    fn a_sut_that_panics_writes_a_backtrace() {
        let command = DeployedSut::new(Path::new(r"C:\vmtest"), SutProgram::Zebar).command();

        assert!(
            command
                .get_envs()
                .any(|(name, value)| name == "RUST_BACKTRACE" && value == Some("1".as_ref()))
        );
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
