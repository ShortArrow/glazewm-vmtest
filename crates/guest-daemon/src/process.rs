//! Runs the SUT programs and the SUT's command-line client as child processes.

use std::fs::File;
use std::io::{Read, Write};
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
/// and zebar with the `config` directory there. Both have the deployed glazewm first on their `PATH`, where an
/// installed glazewm would be, so that zebar's first run picks the starter widget that shows glazewm, whether the
/// daemon starts zebar or glazewm does. Started, its output goes to `stdout.log` and `stderr.log` there, with a
/// backtrace if it panics, through the daemon, so that the program never waits on the guest's disk to write it.
pub struct DeployedSut {
    base: PathBuf,
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
            base: base.to_path_buf(),
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
            .env("PATH", self.path_with_glazewm_first())
            .current_dir(&self.directory)
            .env("RUST_BACKTRACE", "1");
        without_window(&mut command);
        command
    }

    fn path_with_glazewm_first(&self) -> std::ffi::OsString {
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let directories = std::iter::once(self.base.join("glazewm"))
            .chain(std::env::split_paths(&inherited))
            .collect::<Vec<_>>();
        std::env::join_paths(directories).unwrap_or(inherited)
    }

    /// Starts the program and returns once its process exists, leaving it running.
    pub fn start(&self) -> Result<(), String> {
        let log = |name: &str| {
            let path = self.directory.join(name);
            File::create(&path)
                .map_err(|error| format!("cannot create {}: {error}", path.display()))
        };
        let stdout_log = log("stdout.log")?;
        let stderr_log = log("stderr.log")?;
        let mut child = self
            .command()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("cannot start {}: {error}", self.executable().display()))?;
        if let Some(stdout) = child.stdout.take() {
            relay(stdout, stdout_log);
        }
        if let Some(stderr) = child.stderr.take() {
            relay(stderr, stderr_log);
        }
        Ok(())
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

/// Copies everything read from `source` to `sink` on threads of its own, until `source` ends. Reading never
/// waits on `sink`: what `sink` has not yet taken is held in memory, so a program writing to `source` is not
/// held up by a slow disk behind `sink`.
fn relay(mut source: impl Read + Send + 'static, mut sink: impl Write + Send + 'static) {
    let (chunks, received) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buffer = vec![0; 64 * 1024];
        while let Ok(read @ 1..) = source.read(&mut buffer) {
            if chunks.send(buffer[..read].to_vec()).is_err() {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        for chunk in received {
            if sink.write_all(&chunk).and_then(|()| sink.flush()).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod relay_tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// A sink that takes no bytes until released, then hands each write to the test.
    struct HeldSink {
        release: mpsc::Receiver<()>,
        released: bool,
        written: mpsc::Sender<Vec<u8>>,
    }

    impl Write for HeldSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if !self.released {
                let _ = self.release.recv();
                self.released = true;
            }
            let _ = self.written.send(buf.to_vec());
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn held_sink() -> (HeldSink, mpsc::Sender<()>, mpsc::Receiver<Vec<u8>>) {
        let (release_tx, release) = mpsc::channel();
        let (written, written_rx) = mpsc::channel();
        let sink = HeldSink {
            release,
            released: false,
            written,
        };
        (sink, release_tx, written_rx)
    }

    const OUTPUT_SIZE: usize = 4 * 1024 * 1024;

    #[test]
    fn a_program_writes_its_output_without_waiting_for_the_log_file() {
        let (source, mut program_output) = std::io::pipe().unwrap();
        let (sink, release, _written) = held_sink();
        relay(source, sink);
        let (done_tx, done) = mpsc::channel();

        std::thread::spawn(move || {
            let _ = program_output.write_all(&vec![b'x'; OUTPUT_SIZE]);
            let _ = done_tx.send(());
        });

        let finished = done.recv_timeout(Duration::from_secs(5)).is_ok();
        let _ = release.send(());
        assert!(finished, "the program's write waited for the held log file");
    }

    #[test]
    fn the_log_file_receives_all_output_in_order_once_it_accepts_writes() {
        let (source, mut program_output) = std::io::pipe().unwrap();
        let (sink, release, written) = held_sink();
        relay(source, sink);
        let output = (0..OUTPUT_SIZE)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<_>>();
        let writer = {
            let output = output.clone();
            std::thread::spawn(move || program_output.write_all(&output))
        };

        release.send(()).unwrap();
        writer.join().unwrap().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut received = Vec::new();
        while received.len() < output.len() && Instant::now() < deadline {
            if let Ok(chunk) = written.recv_timeout(Duration::from_millis(100)) {
                received.extend(chunk);
            }
        }

        assert!(
            received == output,
            "received {} of {} bytes, or out of order",
            received.len(),
            output.len()
        );
    }
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

    fn first_on_path(command: &Command) -> Option<PathBuf> {
        let path = command
            .get_envs()
            .find_map(|(name, value)| (name == "PATH").then_some(value))
            .flatten()?;
        std::env::split_paths(path).next()
    }

    #[test]
    fn zebar_finds_the_deployed_glazewm_on_its_path_first() {
        let command = DeployedSut::new(Path::new(r"C:\vmtest"), SutProgram::Zebar).command();

        assert_eq!(first_on_path(&command), Some(r"C:\vmtest\glazewm".into()));
    }

    #[test]
    fn a_zebar_that_glazewm_starts_finds_the_deployed_glazewm_on_its_path_first() {
        let command = DeployedSut::new(Path::new(r"C:\vmtest"), SutProgram::Glazewm).command();

        assert_eq!(first_on_path(&command), Some(r"C:\vmtest\glazewm".into()));
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
