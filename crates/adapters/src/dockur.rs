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
}

impl DockurContainer {
    fn docker(&self, args: &[&str]) -> Result<(), String> {
        run(Command::new("docker")
            .args(args)
            .current_dir(&self.compose_dir))
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
