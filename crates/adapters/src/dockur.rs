//! A Windows guest in a dockur/windows container, reached over SSH and the guest daemon.

use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use vmtest_domain::scenario::{
    GlazewmIpc, Observation, TestWindow, TestWindowBatch, WindowLifetime, WindowPresence,
};
use vmtest_domain::topology::Rect;
use vmtest_guest_client::GuestDaemonClient;
use vmtest_protocol::{
    GlazewmQuery, PresenceDto, RectDto, Request, Response, SutProgram, TestWindowDto,
};
use vmtest_runner::{
    Clock, DisplayController, GuestExecutor, Hypervisor, Observer, WorkloadController,
};

use crate::glazewm::{self, WindowIdentity};

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
    pub ssh_port: u16,
    pub user: String,
    pub daemon_address: String,
    pub daemon_executable: PathBuf,
    /// The Win32 test-window application, delivered beside the daemon.
    pub test_window_executable: PathBuf,
    /// Directory holding glazewm.exe, glazewm-cli.exe and glazewm-watcher.exe.
    pub glazewm_build: PathBuf,
    pub glazewm_config: PathBuf,
    pub start_glazewm_script: PathBuf,
}

/// The runner's ports for one dockur/windows guest.
pub struct DockurGuest {
    config: DockurConfig,
    daemon: Option<GuestDaemonClient<TcpStream>>,
}

const VIRTUAL_DISPLAY: &str = "Get-PnpDevice -FriendlyName 'Virtual Display Driver'";

/// How long one connect, read, or write to the guest daemon may take.
const DAEMON_TIMEOUT: Duration = Duration::from_secs(5);

/// The longest IPC limit whose timed-out answer still reaches the runner within `DAEMON_TIMEOUT`.
const LONGEST_IPC_LIMIT: Duration = Duration::from_secs(4);

impl DockurGuest {
    pub fn new(config: DockurConfig) -> Self {
        Self {
            config,
            daemon: None,
        }
    }

    fn docker(&self, args: &[&str]) -> Result<(), String> {
        run(Command::new("docker")
            .args(args)
            .current_dir(&self.config.compose_dir))
    }

    fn ssh(&self, command: &str) -> Result<String, String> {
        let output = Command::new("ssh")
            .args(self.ssh_options())
            .arg("-p")
            .arg(self.config.ssh_port.to_string())
            .arg(format!("{}@127.0.0.1", self.config.user))
            .arg(command)
            .output()
            .map_err(|error| format!("cannot start ssh: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "ssh `{command}` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn scp(&self, sources: &[&PathBuf], destination: &str) -> Result<(), String> {
        run(Command::new("scp")
            .args(self.ssh_options())
            .arg("-P")
            .arg(self.config.ssh_port.to_string())
            .args(sources.iter().map(|source| source.as_os_str()))
            .arg(format!("{}@127.0.0.1:{destination}", self.config.user)))
    }

    /// Options shared by ssh and scp.
    ///
    /// The guest's host key changes with every reinstall, so it is not checked; it is recorded beside the key,
    /// because Windows OpenSSH treats `UserKnownHostsFile=NUL` as a file named NUL in the working directory.
    fn ssh_options(&self) -> Vec<String> {
        let known_hosts = self.config.ssh_key.with_file_name("known_hosts");
        vec![
            "-q".into(),
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "StrictHostKeyChecking=no".into(),
            "-o".into(),
            format!("UserKnownHostsFile={}", known_hosts.display()),
            "-o".into(),
            "ConnectTimeout=5".into(),
            "-i".into(),
            self.config.ssh_key.display().to_string(),
        ]
    }

    /// Sends a request over the current connection, opening one if there is none.
    ///
    /// A failed request drops the connection, since a late answer would otherwise be read as the next one's.
    fn request(&mut self, request: &Request) -> Result<Response, String> {
        let daemon = match self.daemon.as_mut() {
            Some(daemon) => daemon,
            None => self.daemon.insert(
                GuestDaemonClient::connect(self.config.daemon_address.as_str(), DAEMON_TIMEOUT)
                    .map_err(|error| error.to_string())?,
            ),
        };
        let response = daemon.request(request).map_err(|error| error.to_string());
        if response.is_err() {
            self.daemon = None;
        }
        response
    }

    /// Queries glazewm's monitors, telling the test windows among its windows apart by `test_windows`,
    /// pairs of id and handle, and asking the guest what the OS says of each window.
    fn query_glazewm_monitors(
        &mut self,
        ipc_limit: Duration,
        test_windows: &[(u32, i64)],
    ) -> Result<GlazewmIpc, String> {
        if ipc_limit > LONGEST_IPC_LIMIT {
            return Err(format!(
                "an IPC limit of {ipc_limit:?} exceeds the {LONGEST_IPC_LIMIT:?} this adapter supports"
            ));
        }
        let request = Request::QueryGlazewm {
            query: GlazewmQuery::Monitors,
            limit_ms: ipc_limit.as_millis().try_into().unwrap_or(u64::MAX),
        };
        let response = self.request(&request)?;
        let handles = match &response {
            Response::Output { stdout, .. } => glazewm::window_handles(stdout),
            _ => Vec::new(),
        };
        let presences = self.window_presence(&handles)?;
        let identify = |handle: i64| WindowIdentity {
            test_window: test_windows
                .iter()
                .find(|(_, test_handle)| *test_handle == handle)
                .map(|(id, _)| *id),
            presence: handles
                .iter()
                .position(|&known| known == handle)
                .map_or(WindowPresence::Gone, |index| presences[index]),
        };
        glazewm::monitors_answer(response, &identify)
    }

    fn window_presence(&mut self, handles: &[i64]) -> Result<Vec<WindowPresence>, String> {
        if handles.is_empty() {
            return Ok(Vec::new());
        }
        let request = Request::WindowPresence {
            handles: handles.to_vec(),
        };
        match self.request(&request)? {
            Response::WindowPresence { presences } if presences.len() == handles.len() => {
                Ok(presences
                    .into_iter()
                    .map(|presence| match presence {
                        PresenceDto::TopLevel => WindowPresence::TopLevel,
                        PresenceDto::Child => WindowPresence::Child,
                        PresenceDto::Gone => WindowPresence::Gone,
                    })
                    .collect())
            }
            other => Err(format!("unexpected answer to window_presence: {other:?}")),
        }
    }

    fn test_windows(&mut self) -> Result<Vec<TestWindowDto>, String> {
        match self.request(&Request::TestWindows)? {
            Response::TestWindows { windows } => Ok(windows),
            other => Err(format!("unexpected answer to test_windows: {other:?}")),
        }
    }

    fn glazewm_is_running(&mut self) -> Result<bool, String> {
        let request = Request::SutRunning {
            program: SutProgram::Glazewm,
        };
        match self.request(&request)? {
            Response::Running { running } => Ok(running),
            other => Err(format!("unexpected answer to sut_running: {other:?}")),
        }
    }

    /// Asks glazewm, then checks that it is still running, so that a glazewm that exits during the query
    /// is observed as not running rather than as a failed query.
    fn observe_glazewm(
        &mut self,
        ipc_limit: Duration,
        test_windows: &[(u32, i64)],
    ) -> Result<GlazewmIpc, String> {
        let answer = self.query_glazewm_monitors(ipc_limit, test_windows);
        if !self.glazewm_is_running()? {
            return Ok(GlazewmIpc::NotRunning);
        }
        answer
    }
}

impl Hypervisor for DockurGuest {
    fn restore(&mut self) -> Result<(), String> {
        self.daemon = None;
        self.docker(&["compose", "stop", &self.config.service])?;
        let volume = format!("{}:/storage", self.config.volume);
        self.docker(&[
            "run",
            "--rm",
            "-v",
            &volume,
            "--entrypoint",
            "qemu-img",
            &self.config.image,
            "snapshot",
            "-a",
            &self.config.snapshot,
            "/storage/data.qcow2",
        ])?;
        self.docker(&["compose", "start", &self.config.service])
    }
}

impl GuestExecutor for DockurGuest {
    fn wait_until_ready(&mut self) -> Result<(), String> {
        poll(Duration::from_secs(300), "SSH to answer", || {
            self.ssh("exit 0").map(|_| ())
        })?;
        self.scp(
            &[
                &self.config.daemon_executable.clone(),
                &self.config.test_window_executable.clone(),
            ],
            "C:/vmtest/",
        )?;
        self.ssh("Start-ScheduledTask -TaskName vmtest-guest-daemon")?;
        let address = self.config.daemon_address.clone();
        let daemon = poll(
            Duration::from_secs(60),
            "the guest daemon to answer",
            || {
                let mut client = GuestDaemonClient::connect(address.as_str(), DAEMON_TIMEOUT)
                    .map_err(|error| error.to_string())?;
                match client.request(&Request::Ping) {
                    Ok(Response::Pong) => Ok(client),
                    Ok(other) => Err(format!("unexpected answer to ping: {other:?}")),
                    Err(error) => Err(error.to_string()),
                }
            },
        )?;
        self.daemon = Some(daemon);
        Ok(())
    }

    fn start_sut(&mut self, ipc_limit: Duration) -> Result<(), String> {
        self.ssh("New-Item -ItemType Directory -Force C:\\vmtest\\glazewm | Out-Null")?;
        let build = &self.config.glazewm_build;
        let binaries = [
            build.join("glazewm.exe"),
            build.join("glazewm-cli.exe"),
            build.join("glazewm-watcher.exe"),
        ];
        self.scp(&binaries.iter().collect::<Vec<_>>(), "C:/vmtest/glazewm/")?;
        self.scp(
            &[&self.config.glazewm_config.clone()],
            "C:/vmtest/glazewm/config.yaml",
        )?;
        self.scp(&[&self.config.start_glazewm_script.clone()], "C:/vmtest/")?;
        self.ssh(&format!(
            "$a = New-ScheduledTaskAction -Execute powershell.exe -Argument '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\\vmtest\\start-glazewm.ps1'; \
             $p = New-ScheduledTaskPrincipal -UserId {} -LogonType Interactive -RunLevel Limited; \
             Register-ScheduledTask -TaskName vmtest-sut-glazewm -Action $a -Principal $p -Force | Out-Null; \
             Start-ScheduledTask -TaskName vmtest-sut-glazewm",
            self.config.user
        ))?;
        poll(Duration::from_secs(60), "glazewm to take a query", || {
            self.query_glazewm_monitors(ipc_limit, &[]).map(|_| ())
        })
    }
}

impl DisplayController for DockurGuest {
    fn set_virtual_monitor_count(&mut self, count: u32) -> Result<(), String> {
        self.ssh(&format!(
            "$p = New-Object System.IO.Pipes.NamedPipeClientStream('.', 'MTTVirtualDisplayPipe', 'InOut'); \
             $p.Connect(5000); $b = [Text.Encoding]::Unicode.GetBytes('SETDISPLAYCOUNT {count}'); \
             $p.Write($b, 0, $b.Length); $p.Flush(); Start-Sleep -Milliseconds 500; $p.Dispose()"
        ))
        .map(|_| ())
    }

    fn remove_virtual_monitors(&mut self) -> Result<(), String> {
        self.ssh(&format!(
            "{VIRTUAL_DISPLAY} | Disable-PnpDevice -Confirm:$false"
        ))
        .map(|_| ())
    }

    fn restore_virtual_monitors(&mut self) -> Result<(), String> {
        self.ssh(&format!(
            "{VIRTUAL_DISPLAY} | Enable-PnpDevice -Confirm:$false"
        ))
        .map(|_| ())
    }
}

impl WorkloadController for DockurGuest {
    fn open_test_windows(&mut self, batch: &TestWindowBatch) -> Result<(), String> {
        let count = batch.count;
        let milliseconds = |duration: Duration| duration.as_millis().try_into().unwrap_or(u64::MAX);
        let request = Request::OpenTestWindows {
            count,
            within_ms: batch.within.map(milliseconds),
            lifetime_ms: match batch.lifetime {
                WindowLifetime::Persistent => None,
                WindowLifetime::DestroyedAfter(lifetime) => Some(milliseconds(lifetime)),
            },
        };
        match self.request(&request)? {
            Response::TestWindowsOpened { ids } if ids.len() == count as usize => Ok(()),
            other => Err(format!(
                "unexpected answer to opening {count} test windows: {other:?}"
            )),
        }
    }

    fn hang_test_window(&mut self, id: u32) -> Result<(), String> {
        match self.request(&Request::HangTestWindow { id })? {
            Response::TestWindowHung { id: hung } if hung == id => Ok(()),
            other => Err(format!(
                "unexpected answer to hanging test window {id}: {other:?}"
            )),
        }
    }

    fn resume_test_window(&mut self, id: u32) -> Result<(), String> {
        match self.request(&Request::ResumeTestWindow { id })? {
            Response::TestWindowResumed { id: resumed } if resumed == id => Ok(()),
            other => Err(format!(
                "unexpected answer to resuming test window {id}: {other:?}"
            )),
        }
    }
}

impl Observer for DockurGuest {
    fn observe(&mut self, ipc_limit: Duration) -> Result<Observation, String> {
        let os_displays = match self.request(&Request::Topology)? {
            Response::Topology { displays } => displays
                .iter()
                .map(|display| rect(display.bounds))
                .collect(),
            other => return Err(format!("unexpected answer to topology: {other:?}")),
        };
        let test_windows = self.test_windows()?;
        let handles: Vec<_> = test_windows
            .iter()
            .map(|window| (window.id, window.handle))
            .collect();
        let glazewm = self.observe_glazewm(ipc_limit, &handles)?;
        let test_windows = test_windows
            .into_iter()
            .map(|window| TestWindow {
                id: window.id,
                frame: window.frame.map(rect),
                hung: window.hung,
            })
            .collect();
        Ok(Observation::new(os_displays, test_windows, glazewm))
    }
}

/// Elapsed wall-clock time since creation.
pub struct SystemClock(Instant);

impl SystemClock {
    pub fn start() -> Self {
        Self(Instant::now())
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

fn rect(dto: RectDto) -> Rect {
    Rect {
        x: dto.x,
        y: dto.y,
        width: dto.width,
        height: dto.height,
    }
}

fn run(command: &mut Command) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("cannot start {command:?}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{command:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Retries `attempt` every second until it succeeds or `deadline` passes, returning the last error.
fn poll<T>(
    deadline: Duration,
    waiting_for: &str,
    mut attempt: impl FnMut() -> Result<T, String>,
) -> Result<T, String> {
    let start = Instant::now();
    loop {
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) if start.elapsed() >= deadline => {
                return Err(format!(
                    "gave up waiting for {waiting_for} after {deadline:?}: {error}"
                ));
            }
            Err(_) => std::thread::sleep(Duration::from_secs(1)),
        }
    }
}
