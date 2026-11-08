//! A Windows guest reached over SSH and the guest daemon, whatever the platform it runs on.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use vmtest_domain::scenario::{
    GlazewmIpc, Load, LoadKind, Observation, Projection, TestWindow, TestWindowBatch,
    WindowLifetime, WindowPresence,
};
use vmtest_domain::topology::Rect;
use vmtest_guest_client::GuestDaemonClient;
use vmtest_protocol::{
    DisplayDto, GlazewmQuery, LoadKindDto, PresenceDto, ProjectionDto, RectDto, Request, Response,
    SutProgram, TestWindowDto,
};
use vmtest_runner::{
    Clock, DisplayController, GuestExecutor, Hypervisor, InputController, Observer,
    WorkloadController,
};

use crate::glazewm::{self, WindowIdentity};

/// How to reach the guest, and the files to deploy to it.
#[derive(Clone, Debug)]
pub struct GuestConfig {
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_key: PathBuf,
    pub user: String,
    /// The `ssh` destination that `ssh_host` is reached through, such as the Docker host on whose loopback a
    /// dockur/windows container publishes the guest's SSH port; `None` connects to `ssh_host` directly.
    pub ssh_jump: Option<String>,
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
    /// The zebar to deploy and start beside glazewm; `None` deploys no zebar.
    pub zebar: Option<ZebarBuild>,
}

/// A zebar built from its repository: the executable, with the settings UI built into it, and the repository's
/// `resources` directory, which it reads its starter widgets and templates from at run time.
///
/// `config`, when set, is a directory whose entries are copied into zebar's configuration directory before zebar
/// starts, such as a `settings.json` and the widget packs it names; without it, zebar's first run installs its
/// starter widgets.
#[derive(Clone, Debug)]
pub struct ZebarBuild {
    pub executable: PathBuf,
    pub resources: PathBuf,
    pub config: Option<PathBuf>,
    pub launch: ZebarLaunch,
}

/// What starts zebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZebarLaunch {
    /// glazewm, through its startup commands, as glazewm's sample configuration does with an installed zebar; glazewm
    /// also stops it when it exits. zebar's output is not kept.
    Glazewm,
    /// The guest daemon, once glazewm answers IPC, keeping zebar's output in `stdout.log` and `stderr.log`.
    GuestDaemon,
}

/// The runner's ports for one Windows guest, restored by `H`.
pub struct WindowsGuest<H> {
    config: GuestConfig,
    hypervisor: H,
    daemon: Option<GuestDaemonClient<TcpStream>>,
    daemon_forward: Option<Child>,
}

/// The name the virtual display driver gives its device and its display adapter.
const VIRTUAL_DISPLAY_DRIVER: &str = "Virtual Display Driver";

/// How long the guest OS may take to show the virtual monitors the driver was asked for.
const VIRTUAL_MONITORS_LIMIT: Duration = Duration::from_secs(30);

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

    pub fn with_zebar(mut self, zebar: ZebarBuild) -> Self {
        self.config.zebar = Some(zebar);
        self
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
            .arg("-r")
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
            wait_until_forwarding(
                stderr.expect("stderr is piped"),
                &self.config.daemon_address,
            )?;
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
        let jump = self
            .config
            .ssh_jump
            .iter()
            .flat_map(|jump| ["-J".to_string(), jump.clone()]);
        let mut options = vec![
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
        ];
        options.extend(jump);
        options
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

    /// The displays the guest OS shows in the interactive session.
    fn displays(&mut self) -> Result<Vec<DisplayDto>, String> {
        match self.request(&Request::Topology)? {
            Response::Topology { displays } => Ok(displays),
            other => Err(format!("unexpected answer to topology: {other:?}")),
        }
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

    /// The id of the test window in the foreground, or `None` when the foreground window is no test window.
    pub fn foreground_test_window(&mut self) -> Result<Option<u32>, String> {
        let handle = match self.request(&Request::ForegroundWindow)? {
            Response::ForegroundWindow { handle } => handle,
            other => {
                return Err(format!(
                    "unexpected answer to asking for the foreground window: {other:?}"
                ));
            }
        };
        Ok(self
            .test_windows()?
            .into_iter()
            .find(|window| Some(window.handle) == handle)
            .map(|window| window.id))
    }

    fn start(&mut self, program: SutProgram) -> Result<(), String> {
        match self.request(&Request::StartSut { program })? {
            Response::SutStarted { program: started } if started == program => Ok(()),
            other => Err(format!(
                "unexpected answer to starting {program:?}: {other:?}"
            )),
        }
    }

    /// Copies zebar, its resources to where it resolves them from, and its configuration when given: Tauri spells
    /// the `../../resources` it names as `_up_\_up_\resources` under the executable's directory.
    fn deploy_zebar(&self, zebar: &ZebarBuild) -> Result<(), String> {
        self.ssh(r"New-Item -ItemType Directory -Force C:\vmtest\zebar\config, C:\vmtest\zebar\_up_\_up_ | Out-Null")?;
        self.scp(&[&zebar.executable], "C:/vmtest/zebar/")?;
        self.scp(&[&zebar.resources], "C:/vmtest/zebar/_up_/_up_/")?;
        let Some(config) = &zebar.config else {
            return Ok(());
        };
        let entries = std::fs::read_dir(config)
            .and_then(|entries| {
                entries
                    .map(|entry| entry.map(|e| e.path()))
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|error| {
                format!(
                    "cannot read zebar's configuration at {}: {error}",
                    config.display()
                )
            })?;
        self.scp(
            &entries.iter().collect::<Vec<_>>(),
            "C:/vmtest/zebar/config/",
        )
    }

    /// Writes the glazewm configuration with glazewm starting the deployed zebar, beside the configuration it is made
    /// from, and returns its path.
    fn glazewm_config_starting_zebar(&self) -> Result<PathBuf, String> {
        let source = &self.config.glazewm_config;
        let config = std::fs::read_to_string(source).map_err(|error| {
            format!(
                "cannot read glazewm's configuration at {}: {error}",
                source.display()
            )
        })?;
        let starting_zebar = glazewm::config_starting_zebar(
            &config,
            r"C:\vmtest\zebar\zebar.exe",
            r"C:\vmtest\zebar\config",
        );
        let path = source.with_extension("starting-zebar.yaml");
        std::fs::write(&path, starting_zebar)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        Ok(path)
    }

    fn set_display_power(&mut self, on: bool) -> Result<(), String> {
        match self.request(&Request::SetDisplayPower { on })? {
            Response::DisplayPowerSet { on: set } if set == on => Ok(()),
            other => Err(format!(
                "unexpected answer to setting the display power {on}: {other:?}"
            )),
        }
    }

    fn dialogs(&mut self, program: SutProgram) -> Result<Vec<String>, String> {
        match self.request(&Request::SutDialogs { program })? {
            Response::Dialogs { texts } => Ok(texts),
            other => Err(format!("unexpected answer to sut_dialogs: {other:?}")),
        }
    }

    fn is_running(&mut self, program: SutProgram) -> Result<bool, String> {
        let request = Request::SutRunning { program };
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
        if !self.is_running(SutProgram::Glazewm)? {
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
        let zebar = self.config.zebar.clone();
        if let Some(zebar) = &zebar {
            self.deploy_zebar(zebar)?;
        }
        let glazewm_config = match zebar.as_ref().map(|zebar| zebar.launch) {
            Some(ZebarLaunch::Glazewm) => self.glazewm_config_starting_zebar()?,
            _ => self.config.glazewm_config.clone(),
        };
        self.scp(&[&glazewm_config], "C:/vmtest/glazewm/config.yaml")?;
        self.start(SutProgram::Glazewm)?;
        poll(Duration::from_secs(60), "glazewm to take a query", || {
            self.query_glazewm_monitors(ipc_limit, &[]).map(|_| ())
        })?;
        if zebar.is_some_and(|zebar| zebar.launch == ZebarLaunch::GuestDaemon) {
            self.start(SutProgram::Zebar)?;
        }
        Ok(())
    }
}

impl<H> DisplayController for WindowsGuest<H> {
    fn set_virtual_monitor_count(&mut self, count: u32) -> Result<(), String> {
        settable_virtual_monitor_count(count)?;
        self.ssh(&virtual_monitor_count_command(count))?;
        poll(VIRTUAL_MONITORS_LIMIT, "the virtual monitors", || {
            virtual_monitors_in_place(&self.displays()?, count)
        })
    }

    fn remove_virtual_monitors(&mut self) -> Result<(), String> {
        self.ssh(&format!(
            "Get-PnpDevice -FriendlyName '{VIRTUAL_DISPLAY_DRIVER}' | Disable-PnpDevice -Confirm:$false"
        ))
        .map(|_| ())
    }

    fn restore_virtual_monitors(&mut self) -> Result<(), String> {
        self.ssh(&format!(
            "Get-PnpDevice -FriendlyName '{VIRTUAL_DISPLAY_DRIVER}' | Enable-PnpDevice -Confirm:$false"
        ))
        .map(|_| ())
    }

    fn turn_displays_off(&mut self) -> Result<(), String> {
        self.set_display_power(false)
    }

    fn wake_displays(&mut self) -> Result<(), String> {
        self.set_display_power(true)
    }

    fn set_projection(&mut self, projection: Projection) -> Result<(), String> {
        let dto = match projection {
            Projection::PcScreenOnly => ProjectionDto::PcScreenOnly,
            Projection::Duplicate => ProjectionDto::Duplicate,
            Projection::Extend => ProjectionDto::Extend,
            Projection::SecondScreenOnly => ProjectionDto::SecondScreenOnly,
        };
        match self.request(&Request::SetProjection { projection: dto })? {
            Response::ProjectionSet { projection: set } if set == dto => Ok(()),
            other => Err(format!(
                "unexpected answer to switching to {projection:?}: {other:?}"
            )),
        }
    }
}

/// The middle of test window `id`'s frame, in virtual-screen coordinates and physical pixels.
fn middle_of_test_window(windows: &[TestWindowDto], id: u32) -> Result<(i32, i32), String> {
    let window = windows
        .iter()
        .find(|window| window.id == id)
        .ok_or_else(|| format!("test window {id} was never opened"))?;
    let frame = window
        .frame
        .ok_or_else(|| format!("test window {id} no longer exists"))?;
    let half = |length: u32| i32::try_from(length / 2).unwrap_or(i32::MAX);
    Ok((frame.x + half(frame.width), frame.y + half(frame.height)))
}

impl<H> InputController for WindowsGuest<H> {
    fn click_test_window(&mut self, id: u32) -> Result<(), String> {
        let (x, y) = middle_of_test_window(&self.test_windows()?, id)?;
        match self.request(&Request::Click { x, y })? {
            Response::Clicked { x: cx, y: cy } if (cx, cy) == (x, y) => Ok(()),
            other => Err(format!(
                "unexpected answer to clicking test window {id} at {x}, {y}: {other:?}"
            )),
        }
    }

    fn press_shortcut(&mut self, shortcut: &str) -> Result<(), String> {
        let request = Request::PressShortcut {
            shortcut: shortcut.into(),
        };
        match self.request(&request)? {
            Response::ShortcutPressed { shortcut: pressed } if pressed == shortcut => Ok(()),
            other => Err(format!(
                "unexpected answer to pressing {shortcut}: {other:?}"
            )),
        }
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
        let os_displays = self
            .displays()?
            .iter()
            .map(|display| rect(display.bounds))
            .collect();
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
        let observation = Observation::new(os_displays, test_windows, glazewm)
            .with_glazewm_dialogs(self.dialogs(SutProgram::Glazewm)?);
        if self.config.zebar.is_none() {
            return Ok(observation);
        }
        Ok(observation.with_zebar_running(self.is_running(SutProgram::Zebar)?))
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
///
/// Only ssh's own report for `address` counts, never text the guest sends that resembles it.
fn wait_until_forwarding(
    stderr: impl BufRead + Send + 'static,
    address: &str,
) -> Result<(), String> {
    let (host, port) = address
        .rsplit_once(':')
        .ok_or_else(|| format!("the daemon address {address} has no port"))?;
    let report = format!("debug1: Local forwarding listening on {host} port {port}.");
    let mut lines = stderr.lines();
    let mut last = String::new();
    for line in lines.by_ref() {
        let line = line.map_err(|error| format!("cannot read the SSH port forward: {error}"))?;
        if line.trim_end() == report {
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

/// Succeeds when the OS shows `count` displays driven by the virtual display driver among `displays`.
/// Sets the count in the driver's settings while its device is disabled, since the driver reads the count only when
/// it starts, and its own request for a new count restarts it by crashing, which Windows allows only a few times.
fn virtual_monitor_count_command(count: u32) -> String {
    format!(
        "$d = Get-PnpDevice -FriendlyName '{VIRTUAL_DISPLAY_DRIVER}'; \
         $d | Disable-PnpDevice -Confirm:$false; \
         $s = 'C:\\VirtualDisplayDriver\\vdd_settings.xml'; \
         (Get-Content -Raw $s) -replace '<count>\\d+</count>', '<count>{count}</count>' | Set-Content -NoNewline $s; \
         $d | Enable-PnpDevice -Confirm:$false"
    )
}

/// The virtual display driver stops working when asked for no monitors, until the guest is restored.
fn settable_virtual_monitor_count(count: u32) -> Result<(), String> {
    if count == 0 {
        Err("the virtual display driver fails when asked for no monitors; remove the virtual monitors instead".into())
    } else {
        Ok(())
    }
}

fn virtual_monitors_in_place(displays: &[DisplayDto], count: u32) -> Result<(), String> {
    let shown = displays
        .iter()
        .filter(|display| display.adapter == VIRTUAL_DISPLAY_DRIVER)
        .count();
    if shown == count as usize {
        Ok(())
    } else {
        Err(format!("the OS shows {shown} of {count} virtual monitors"))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_window(id: u32, frame: Option<RectDto>) -> TestWindowDto {
        TestWindowDto {
            id,
            handle: 10 + i64::from(id),
            frame,
            hung: false,
        }
    }

    #[test]
    fn clicks_a_test_window_in_the_middle_of_its_frame() {
        let frame = RectDto {
            x: -1280,
            y: 0,
            width: 640,
            height: 401,
        };
        let windows = [test_window(0, None), test_window(1, Some(frame))];

        assert_eq!(middle_of_test_window(&windows, 1), Ok((-960, 200)));
    }

    #[test]
    fn cannot_click_a_test_window_that_is_closed_or_was_never_opened() {
        let windows = [test_window(0, None)];

        assert!(middle_of_test_window(&windows, 0).is_err());
        assert!(middle_of_test_window(&windows, 1).is_err());
    }

    fn guest(daemon_forward: Option<&str>) -> WindowsGuest<()> {
        WindowsGuest::with_hypervisor(
            GuestConfig {
                ssh_host: "192.0.2.7".into(),
                ssh_port: 22,
                ssh_key: PathBuf::from("keys/id_ed25519"),
                user: "fixture".into(),
                ssh_jump: None,
                daemon_address: "127.0.0.1:17041".into(),
                daemon_forward: daemon_forward.map(String::from),
                daemon_executable: PathBuf::new(),
                test_window_executable: PathBuf::new(),
                load_executable: PathBuf::new(),
                daemon_limits: None,
                glazewm_build: PathBuf::new(),
                glazewm_config: PathBuf::new(),
                zebar: None,
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

    fn display(adapter: &str) -> DisplayDto {
        let rect = RectDto {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        };
        DisplayDto {
            device_name: String::new(),
            adapter: adapter.into(),
            bounds: rect,
            working_area: rect,
            is_primary: false,
        }
    }

    #[test]
    fn the_virtual_monitors_are_in_place_when_the_os_shows_that_many_of_the_drivers_displays() {
        let shown = [
            display("Microsoft Hyper-V Video"),
            display("Virtual Display Driver"),
        ];

        assert_eq!(virtual_monitors_in_place(&shown, 1), Ok(()));
    }

    #[test]
    fn refuses_to_ask_the_driver_for_no_virtual_monitors() {
        assert_eq!(
            settable_virtual_monitor_count(0),
            Err("the virtual display driver fails when asked for no monitors; remove the virtual monitors instead".into())
        );
    }

    #[test]
    fn writes_the_count_while_the_driver_is_stopped_so_that_it_starts_with_that_many_monitors() {
        assert_eq!(
            virtual_monitor_count_command(2),
            "$d = Get-PnpDevice -FriendlyName 'Virtual Display Driver'; \
             $d | Disable-PnpDevice -Confirm:$false; \
             $s = 'C:\\VirtualDisplayDriver\\vdd_settings.xml'; \
             (Get-Content -Raw $s) -replace '<count>\\d+</count>', '<count>2</count>' | Set-Content -NoNewline $s; \
             $d | Enable-PnpDevice -Confirm:$false"
        );
    }

    #[test]
    fn asks_the_driver_for_one_virtual_monitor_or_more() {
        assert_eq!(settable_virtual_monitor_count(1), Ok(()));
    }

    #[test]
    fn the_virtual_monitors_are_not_in_place_while_the_os_shows_fewer_of_the_drivers_displays() {
        let shown = [
            display("Microsoft Hyper-V Video"),
            display("Virtual Display Driver"),
        ];

        assert_eq!(
            virtual_monitors_in_place(&shown, 3),
            Err("the OS shows 1 of 3 virtual monitors".into())
        );
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

        assert_eq!(
            wait_until_forwarding(&stderr[..], "127.0.0.1:17041"),
            Ok(())
        );
    }

    #[test]
    fn text_from_the_guest_that_resembles_the_report_is_not_taken_for_it() {
        let stderr = b"debug1: Remote: Local forwarding listening on 127.0.0.1 port 17041.\r\n\
            Local forwarding listening on 127.0.0.1 port 17041.\r\n\
            debug1: Local forwarding listening on 127.0.0.1 port 27041.\r\n";

        assert!(wait_until_forwarding(&stderr[..], "127.0.0.1:17041").is_err());
    }

    #[test]
    fn a_forward_whose_ssh_exits_without_listening_is_refused_with_its_last_words() {
        let stderr = b"debug1: Local connections to 127.0.0.1:17041 forwarded to remote address 127.0.0.1:7041\r\n\
            bind [127.0.0.1]:17041: Address already in use\r\n\
            Could not request local forwarding.\r\n";

        let error = wait_until_forwarding(&stderr[..], "127.0.0.1:17041").unwrap_err();

        assert!(
            error.contains("Could not request local forwarding."),
            "{error}"
        );
    }

    #[test]
    fn connects_to_the_guest_directly_when_no_jump_is_set() {
        assert!(!guest(None).ssh_options().contains(&"-J".to_string()));
    }

    #[test]
    fn connects_to_the_guest_through_the_jump_when_one_is_set() {
        let mut guest = guest(Some("127.0.0.1:7041"));
        guest.config.ssh_jump = Some("vmtest@192.0.2.8".into());
        let options = guest.ssh_options();
        let forward = arguments(&guest.daemon_forward_command().unwrap());

        assert!(
            options
                .windows(2)
                .any(|pair| pair == ["-J", "vmtest@192.0.2.8"]),
            "{options:?}"
        );
        assert!(
            forward
                .windows(2)
                .any(|pair| pair == ["-J", "vmtest@192.0.2.8"]),
            "{forward:?}"
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
