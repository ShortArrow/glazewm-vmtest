//! A Windows guest in a dockur/windows container, restored from an internal snapshot of its qcow2 disk.

use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use vmtest_runner::Hypervisor;

use crate::windows_guest::run;

/// The container, and the snapshot of its disk that a restore returns to.
#[derive(Clone, Debug)]
pub struct DockurContainer {
    /// Directory holding compose.yaml.
    pub compose_dir: PathBuf,
    pub service: String,
    pub volume: String,
    pub image: String,
    pub snapshot: String,
    /// The `ssh` destination of the Docker host the container runs on, such as `user@host`; `None` uses the
    /// Docker daemon the `docker` command reaches by default.
    pub docker_host: Option<String>,
}

impl DockurContainer {
    fn docker(&self, args: &[&str]) -> Result<(), String> {
        run(&mut self.docker_command(args))
    }

    fn docker_output(&self, args: &[&str]) -> Result<String, String> {
        let mut command = self.docker_command(args);
        let output = command
            .output()
            .map_err(|error| format!("cannot start {command:?}: {error}"))?;
        if output.status.success() {
            Ok(format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        } else {
            Err(format!(
                "{command:?} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    /// The boot that follows starting the container, read from the container's state and its log since that start.
    fn boot(&self) -> Result<Boot, String> {
        let id = self.docker_output(&["compose", "ps", "-a", "-q", &self.service])?;
        let state = self.docker_output(&[
            "inspect",
            "-f",
            "{{.State.Running}} {{.State.StartedAt}}",
            id.trim(),
        ])?;
        let (running, started_at) = state
            .trim()
            .split_once(' ')
            .ok_or_else(|| format!("unexpected container state {state:?}"))?;
        let log = self.docker_output(&["logs", "--since", started_at, id.trim()])?;
        Ok(boot_outcome(&log, running == "true"))
    }

    /// Starts the container until QEMU boots Windows, starting it again after a boot that dockur/windows gave up on.
    fn start_until_booted(&self) -> Result<(), String> {
        for _ in 0..BOOT_ATTEMPTS {
            self.docker(&["compose", "start", &self.service])?;
            let deadline = Instant::now() + BOOT_LIMIT;
            loop {
                match self.boot()? {
                    Boot::Started => return Ok(()),
                    Boot::Failed => break,
                    Boot::Booting if Instant::now() < deadline => sleep(BOOT_POLL),
                    Boot::Booting => {
                        return Err(format!("QEMU did not boot Windows within {BOOT_LIMIT:?}"));
                    }
                }
            }
        }
        Err(format!("QEMU failed to boot Windows {BOOT_ATTEMPTS} times"))
    }

    fn docker_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("docker");
        command.args(args).current_dir(&self.compose_dir);
        if let Some(host) = &self.docker_host {
            command.env("DOCKER_HOST", format!("ssh://{host}"));
        }
        command
    }
}

impl Hypervisor for DockurContainer {
    fn restore(&mut self) -> Result<(), String> {
        self.docker(&["compose", "stop", &self.service])?;
        let volume = format!("{}:/storage", self.volume);
        self.docker(&[
            "run",
            "--rm",
            "-v",
            &volume,
            "--entrypoint",
            "qemu-img",
            &self.image,
            "snapshot",
            "-a",
            &self.snapshot,
            "/storage/data.qcow2",
        ])?;
        self.start_until_booted()
    }
}

/// How many times the container is started before a restore gives up on QEMU booting Windows.
const BOOT_ATTEMPTS: usize = 3;
/// How long one start waits for the boot; dockur/windows itself gives up 30 seconds after starting QEMU.
const BOOT_LIMIT: Duration = Duration::from_secs(120);
const BOOT_POLL: Duration = Duration::from_secs(2);

/// What the container's log since its last start says about QEMU booting Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boot {
    Booting,
    Started,
    Failed,
}

/// The boot's state from `log`, the container's log since its last start, and whether the container is running.
///
/// dockur/windows logs "Windows started successfully" once QEMU has booted, and stops the container when it gives up.
fn boot_outcome(log: &str, running: bool) -> Boot {
    if log.contains("Windows started successfully") {
        Boot::Started
    } else if running {
        Boot::Booting
    } else {
        Boot::Failed
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    fn container(docker_host: Option<&str>) -> DockurContainer {
        DockurContainer {
            compose_dir: PathBuf::from("guests/dockur-windows"),
            service: "windows".into(),
            volume: "storage".into(),
            image: "dockurr/windows:6.06".into(),
            snapshot: "daemon-task".into(),
            docker_host: docker_host.map(String::from),
        }
    }

    fn docker_host_of(command: &Command) -> Option<Option<&OsStr>> {
        command
            .get_envs()
            .find(|(name, _)| *name == "DOCKER_HOST")
            .map(|(_, value)| value)
    }

    #[test]
    fn a_running_container_without_the_started_line_is_still_booting() {
        let log = "❯ Booting Windows using QEMU v11.1.1...\n";

        assert_eq!(boot_outcome(log, true), Boot::Booting);
    }

    #[test]
    fn the_started_line_means_windows_started() {
        let log = "❯ Booting Windows using QEMU v11.1.1...\n❯ Windows started successfully, visit http://127.0.0.1:8006/ to view the screen...\n";

        assert_eq!(boot_outcome(log, true), Boot::Started);
    }

    #[test]
    fn a_container_that_stopped_before_the_started_line_failed_to_boot() {
        let log = "❯ Booting Windows using QEMU v11.1.1...\n❯ ERROR: Timeout while waiting for QEMU to boot the machine, terminating...\n";

        assert_eq!(boot_outcome(log, false), Boot::Failed);
    }

    #[test]
    fn uses_the_default_docker_daemon_when_no_host_is_set() {
        assert_eq!(
            docker_host_of(&container(None).docker_command(&["ps"])),
            None
        );
    }

    #[test]
    fn reaches_the_docker_host_over_ssh_when_one_is_set() {
        let command = container(Some("vmtest@192.0.2.8")).docker_command(&["ps"]);

        assert_eq!(
            docker_host_of(&command),
            Some(Some(OsStr::new("ssh://vmtest@192.0.2.8")))
        );
    }
}
