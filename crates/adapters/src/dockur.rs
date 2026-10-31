//! A Windows guest in a dockur/windows container, reached over SSH and the guest daemon.

use std::path::PathBuf;
use std::process::Command;

use vmtest_runner::Hypervisor;

pub use crate::windows_guest::SystemClock;
use crate::windows_guest::{GuestConfig, WindowsGuest, run};

/// Where the container, the guest, and the files to deploy are.
#[derive(Clone, Debug)]
pub struct DockurConfig {
    /// Directory holding compose.yaml.
    pub compose_dir: PathBuf,
    pub service: String,
    pub volume: String,
    pub image: String,
    pub snapshot: String,
    pub ssh_key: PathBuf,
    /// The port on the host's loopback that the container publishes the guest's SSH on.
    pub ssh_port: u16,
    pub user: String,
    pub daemon_address: String,
    pub daemon_executable: PathBuf,
    /// The Win32 test-window application, delivered beside the daemon.
    pub test_window_executable: PathBuf,
    /// The load generator, delivered beside the daemon.
    pub load_executable: PathBuf,
    /// Limits on the daemon's requests, delivered as `guest-daemon.json`; `None` keeps the daemon's defaults.
    pub daemon_limits: Option<PathBuf>,
    /// Directory holding glazewm.exe, glazewm-cli.exe and glazewm-watcher.exe.
    pub glazewm_build: PathBuf,
    pub glazewm_config: PathBuf,
    pub start_glazewm_script: PathBuf,
}

/// The dockur/windows container, restored from an internal snapshot of its qcow2 disk.
pub struct DockurContainer {
    compose_dir: PathBuf,
    service: String,
    volume: String,
    image: String,
    snapshot: String,
}

/// The runner's ports for one dockur/windows guest.
pub type DockurGuest = WindowsGuest<DockurContainer>;

impl DockurGuest {
    pub fn new(config: DockurConfig) -> Self {
        let container = DockurContainer {
            compose_dir: config.compose_dir,
            service: config.service,
            volume: config.volume,
            image: config.image,
            snapshot: config.snapshot,
        };
        let guest = GuestConfig {
            ssh_host: "127.0.0.1".into(),
            ssh_port: config.ssh_port,
            ssh_key: config.ssh_key,
            user: config.user,
            daemon_address: config.daemon_address,
            daemon_executable: config.daemon_executable,
            test_window_executable: config.test_window_executable,
            load_executable: config.load_executable,
            daemon_limits: config.daemon_limits,
            glazewm_build: config.glazewm_build,
            glazewm_config: config.glazewm_config,
            start_glazewm_script: config.start_glazewm_script,
        };
        WindowsGuest::with_hypervisor(guest, container)
    }
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
