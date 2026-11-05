//! A Windows guest in a dockur/windows container, restored from an internal snapshot of its qcow2 disk.

use std::path::PathBuf;
use std::process::Command;

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
        self.docker(&["compose", "start", &self.service])
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
