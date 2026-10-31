//! A Windows guest reached over SSH and the guest daemon, whatever the platform it runs on.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use vmtest_domain::scenario::{
    GlazewmIpc, Load, LoadKind, Observation, TestWindow, TestWindowBatch, WindowLifetime,
    WindowPresence,
};
use vmtest_domain::topology::Rect;
use vmtest_guest_client::GuestDaemonClient;
use vmtest_protocol::{
    GlazewmQuery, LoadKindDto, PresenceDto, RectDto, Request, Response, SutProgram, TestWindowDto,
};
use vmtest_runner::{
    Clock, DisplayController, GuestExecutor, Hypervisor, Observer, WorkloadController,
};

use crate::glazewm::{self, WindowIdentity};

/// How to reach the guest, and the files to deploy to it.
#[derive(Clone, Debug)]
pub struct GuestConfig {
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_key: PathBuf,
    pub user: String,
    /// The address the runner connects to the guest daemon on.
    pub daemon_address: String,
    /// The address the guest daemon listens on inside the guest, when the runner reaches it through an SSH port
    /// forward from `daemon_address`; `None` when `daemon_address` reaches the daemon directly.
    pub daemon_forward: Option<String>,
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

/// The runner's ports for one Windows guest, restored by `H`.
pub struct WindowsGuest<H> {
    config: GuestConfig,
    hypervisor: H,
    daemon: Option<GuestDaemonClient<TcpStream>>,
    daemon_forward: Option<Child>,
}

const VIRTUAL_DISPLAY: &str = "Get-PnpDevice -FriendlyName 'Virtual Display Driver'";

/// How long one connect, read, or write to the guest daemon may take.
const DAEMON_TIMEOUT: Duration = Duration::from_secs(5);

/// The longest IPC limit whose timed-out answer still reaches the runner within `DAEMON_TIMEOUT`.
const LONGEST_IPC_LIMIT: Duration = Duration::from_secs(4);

impl<H> WindowsGuest<H> {
    pub fn with_hypervisor(config: GuestConfig, hypervisor: H) -> Self {
        Self {
            config,
            hypervisor,
            daemon: None,
            daemon_forward: None,
        }
    }

    fn ssh(&self, command: &str) -> Result<String, String> {
        let output = Command::new("ssh")
            .args(self.ssh_options())
            .arg("-p")
            .arg(self.config.ssh_port.to_string())
            .arg(self.ssh_destination())
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
            .arg(format!("{}:{destination}", self.ssh_destination())))
    }

    /// The ssh command that forwards `daemon_address` to the guest daemon, if the daemon is reached that way.
    fn daemon_forward_command(&self) -> Option<Command> {
        let guest_address = self.config.daemon_forward.as_ref()?;
        let mut command = Command::new("ssh");
        command
            .args(self.ssh_options())
            .args(["-v", "-N", "-o", "ExitOnForwardFailure=yes", "-L"])
            .arg(format!("{}:{guest_address}", self.config.daemon_address))
            .arg("-p")
            .arg(self.config.ssh_port.to_string())
            .arg(self.ssh_destination());
        Some(command)
    }

    /// Replaces the SSH port forward to the guest daemon, if the daemon is reached that way.
    fn open_daemon_forward(&mut self) -> Result<(), String> {
        self.close_daemon_forward();
        if let Some(mut command) = self.daemon_forward_command() {
            let mut forward = command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| format!("cannot start the SSH port forward: {error}"))?;
            let stderr = forward.stderr.take().map(BufReader::new);
            self.daemon_forward = Some(forward);
            wait_until_forwarding(stderr.expect("stderr is piped"))?;
        }
        Ok(())
    }

    fn close_daemon_forward(&mut self) {
        self.daemon = None;
        if let Some(mut forward) = self.daemon_forward.take() {
            let _ = forward.kill();
            let _ = forward.wait();
        }
    }

    fn ssh_destination(&self) -> String {
        format!("{}@{}", self.config.user, self.config.ssh_host)
    }

    /// Options shared by ssh and scp.
    ///
    /// The guest's host key is trusted the first time it is seen and must match afterwards; a restore keeps it, and a
    /// reinstall changes it, after which its line in `known_hosts` beside the key has to be removed.
    fn ssh_options(&self) -> Vec<String> {
        let known_hosts = self.config.ssh_key.with_file_name("known_hosts");
        vec![
            "-q".into(),
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
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

impl<H> Drop for WindowsGuest<H> {
    fn drop(&mut self) {
        self.close_daemon_forward();
    }
}

impl<H: Hypervisor> Hypervisor for WindowsGuest<H> {
    fn restore(&mut self) -> Result<(), String> {
        self.close_daemon_forward();
        self.hypervisor.restore()
    }
}

impl<H> GuestExecutor for WindowsGuest<H> {
    fn wait_until_ready(&mut self) -> Result<(), String> {
        poll(Duration::from_secs(300), "SSH to answer", || {
            self.ssh("exit 0").map(|_| ())
        })?;
        self.scp(
            &[
                &self.config.daemon_executable.clone(),
                &self.config.test_window_executable.clone(),
                &self.config.load_executable.clone(),
            ],
            "C:/vmtest/",
        )?;
        if let Some(limits) = self.config.daemon_limits.clone() {
            self.scp(&[&limits], "C:/vmtest/guest-daemon.json")?;
        }
        self.ssh("Start-ScheduledTask -TaskName vmtest-guest-daemon")?;
        self.open_daemon_forward()?;
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

impl<H> DisplayController for WindowsGuest<H> {
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

impl<H> WorkloadController for WindowsGuest<H> {
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

    fn start_load(&mut self, load: &Load) -> Result<(), String> {
        let kind = load_kind(load.kind);
        let request = Request::StartLoad {
            kind,
            percent: load.percent,
        };
        match self.request(&request)? {
            Response::LoadStarted { kind: started } if started == kind => Ok(()),
            other => Err(format!("unexpected answer to starting {load:?}: {other:?}")),
        }
    }

    fn stop_load(&mut self, kind: LoadKind) -> Result<(), String> {
        let dto = load_kind(kind);
        match self.request(&Request::StopLoad { kind: dto })? {
            Response::LoadStopped { kind: stopped } if stopped == dto => Ok(()),
            other => Err(format!(
                "unexpected answer to stopping the {kind:?} load: {other:?}"
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

impl<H> Observer for WindowsGuest<H> {
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

fn load_kind(kind: LoadKind) -> LoadKindDto {
    match kind {
        LoadKind::Cpu => LoadKindDto::Cpu,
        LoadKind::Memory => LoadKindDto::Memory,
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

pub(crate) fn run(command: &mut Command) -> Result<(), String> {
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

/// Reads the `-v` output of an ssh started with `ExitOnForwardFailure` until it reports listening on the forwarded
/// address, so that no other process holds that address when the runner connects to it, then keeps draining it.
fn wait_until_forwarding(stderr: impl BufRead + Send + 'static) -> Result<(), String> {
    let mut lines = stderr.lines();
    let mut last = String::new();
    for line in lines.by_ref() {
        let line = line.map_err(|error| format!("cannot read the SSH port forward: {error}"))?;
        if line.contains("Local forwarding listening on") {
            std::thread::spawn(move || lines.for_each(drop));
            return Ok(());
        }
        last = line;
    }
    Err(format!(
        "the SSH port forward to the guest daemon exited without listening: {}",
        last.trim()
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn guest(daemon_forward: Option<&str>) -> WindowsGuest<()> {
        WindowsGuest::with_hypervisor(
            GuestConfig {
                ssh_host: "192.0.2.7".into(),
                ssh_port: 22,
                ssh_key: PathBuf::from("keys/id_ed25519"),
                user: "fixture".into(),
                daemon_address: "127.0.0.1:17041".into(),
                daemon_forward: daemon_forward.map(String::from),
                daemon_executable: PathBuf::new(),
                test_window_executable: PathBuf::new(),
                load_executable: PathBuf::new(),
                daemon_limits: None,
                glazewm_build: PathBuf::new(),
                glazewm_config: PathBuf::new(),
                start_glazewm_script: PathBuf::new(),
            },
            (),
        )
    }

    fn arguments(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn trusts_the_guests_host_key_on_first_use_and_refuses_a_different_one_later() {
        let options = guest(None).ssh_options();

        assert!(
            options
                .windows(2)
                .any(|pair| pair == ["-o", "StrictHostKeyChecking=accept-new"]),
            "{options:?}"
        );
    }

    #[test]
    fn the_forward_is_ready_once_ssh_reports_listening_on_the_daemon_address() {
        let stderr = b"debug1: Connection established.\r\n\
            debug1: Local connections to 127.0.0.1:17041 forwarded to remote address 127.0.0.1:7041\r\n\
            debug1: Local forwarding listening on 127.0.0.1 port 17041.\r\n\
            debug1: channel 0: new port-listener\r\n";

        assert_eq!(wait_until_forwarding(&stderr[..]), Ok(()));
    }

    #[test]
    fn a_forward_whose_ssh_exits_without_listening_is_refused_with_its_last_words() {
        let stderr = b"debug1: Local connections to 127.0.0.1:17041 forwarded to remote address 127.0.0.1:7041\r\n\
            bind [127.0.0.1]:17041: Address already in use\r\n\
            Could not request local forwarding.\r\n";

        let error = wait_until_forwarding(&stderr[..]).unwrap_err();

        assert!(
            error.contains("Could not request local forwarding."),
            "{error}"
        );
    }

    #[test]
    fn a_daemon_reached_directly_needs_no_forward() {
        assert!(guest(None).daemon_forward_command().is_none());
    }

    #[test]
    fn forwards_the_daemon_address_to_the_daemon_inside_the_guest_without_a_shell() {
        let command = guest(Some("127.0.0.1:7041"))
            .daemon_forward_command()
            .unwrap();
        let arguments = arguments(&command);

        assert_eq!(command.get_program(), "ssh");
        assert!(
            arguments.ends_with(&[
                "-v".into(),
                "-N".into(),
                "-o".into(),
                "ExitOnForwardFailure=yes".into(),
                "-L".into(),
                "127.0.0.1:17041:127.0.0.1:7041".into(),
                "-p".into(),
                "22".into(),
                "fixture@192.0.2.7".into(),
            ]),
            "{arguments:?}"
        );
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["-i", "keys/id_ed25519"]),
            "{arguments:?}"
        );
    }
}
