//! What a scenario does to the guest and what each step concluded.

use std::time::Duration;

use crate::invariant::Violation;
use crate::topology::Rect;

/// One operation of a scenario's When.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Sets how many virtual monitors exist besides the platform's own display: every virtual monitor leaves, the
    /// SUT settles without them, then the new count arrives. The virtual display driver can change its count only by
    /// letting every monitor go, and a SUT that missed that would see a different sequence from run to run.
    SetVirtualMonitorCount(u32),
    /// Sets the virtual monitor count to each of `counts` in turn, `apart` after the guest OS shows the one before,
    /// as a dock does when it drops its monitors and brings them back; the SUT is not given time to settle in between.
    SetVirtualMonitorCounts {
        counts: Vec<u32>,
        apart: Duration,
    },
    /// Removes every virtual monitor at once.
    RemoveVirtualMonitors,
    /// Brings back the virtual monitors removed by `RemoveVirtualMonitors`.
    RestoreVirtualMonitors,
    /// Turns every display off, as the OS does when the user is away; the monitors stay connected.
    TurnDisplaysOff,
    /// Wakes the displays with input, as the user does on returning.
    WakeDisplays,
    /// Switches which displays the OS uses, as Win+P does; the monitors stay connected.
    SwitchProjection(Projection),
    OpenTestWindows(TestWindowBatch),
    /// Stops the message processing of the test window with this id.
    HangTestWindow(u32),
    /// Lets the test window with this id process messages again.
    ResumeTestWindow(u32),
    /// Clicks the middle of the test window with this id, as the user does, whether or not it processes messages.
    ClickTestWindow(u32),
    /// Starts a load in the guest, replacing a running load of the same kind.
    StartLoad(Load),
    /// Stops the running load of this kind, if any.
    StopLoad(LoadKind),
}

/// The display modes Win+P offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection {
    /// Only the primary display.
    PcScreenOnly,
    /// Every display shows the same picture.
    Duplicate,
    /// Every display, side by side.
    Extend,
    /// Every display but the primary one.
    SecondScreenOnly,
}

/// What a load uses up in the guest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoadKind {
    /// Every logical processor, busy for `percent` of the time.
    Cpu,
    /// `percent` of the physical memory, allocated and kept in use.
    Memory,
}

/// A load and its intensity, from 1 to 100 percent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Load {
    pub kind: LoadKind,
    pub percent: u8,
}

/// The loads that the steps applied so far have left running.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveLoads(std::collections::BTreeSet<LoadKind>);

impl ActiveLoads {
    /// Follows `step`; steps that start or stop no load change nothing.
    pub fn apply(&mut self, step: &Step) {
        match step {
            Step::StartLoad(load) => {
                self.0.insert(load.kind);
            }
            Step::StopLoad(kind) => {
                self.0.remove(kind);
            }
            _ => {}
        }
    }

    /// Whether any load is running.
    pub fn any(&self) -> bool {
        !self.0.is_empty()
    }
}

/// Test windows opened by one step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestWindowBatch {
    pub count: u32,
    /// `None` opens each window once the one before is the foreground window, so that every run places
    /// them alike; a span opens them at even intervals within it, without waiting for anything.
    pub within: Option<Duration>,
    pub lifetime: WindowLifetime,
}

impl TestWindowBatch {
    /// `count` persistent windows, each opened once the one before is the foreground window.
    pub fn persistent(count: u32) -> Self {
        Self {
            count,
            within: None,
            lifetime: WindowLifetime::Persistent,
        }
    }
}

/// How long a test window exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowLifetime {
    Persistent,
    /// Destroyed by its own thread this long after it was created.
    DestroyedAfter(Duration),
}

/// Given an initial number of virtual monitors, when the steps run in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    pub name: String,
    pub virtual_monitors: u32,
    pub steps: Vec<Step>,
    /// How long glazewm may take to answer one IPC query, measured in the guest.
    pub ipc_limit: Duration,
}

/// The IPC time limit a scenario uses unless it needs another.
///
/// A responsive glazewm answers `query monitors` in well under a second, so two seconds tells a stalled
/// glazewm from a slow one without making every observation of a stalled one wait long.
pub const DEFAULT_IPC_LIMIT: Duration = Duration::from_secs(2);

/// One workspace as glazewm reports it, without the identifiers that change between runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceLayout {
    pub name: String,
    pub is_displayed: bool,
    pub has_focus: bool,
    pub tiling_direction: String,
    /// The windows on this workspace, in glazewm's order with split containers flattened.
    pub windows: Vec<ManagedWindow>,
}

/// One window glazewm manages, without the handle that changes between runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedWindow {
    /// The test window it is, or `None` for a window the harness did not open.
    pub test_window: Option<u32>,
    /// glazewm's window state: `tiling`, `floating`, `minimized` or `fullscreen`.
    pub state: String,
    /// What the OS says of the window glazewm reports.
    pub presence: WindowPresence,
}

/// Whether a window exists, as the OS reports it, and whether it is a top-level window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowPresence {
    TopLevel,
    /// It exists but has a parent window.
    Child,
    Gone,
}

/// A window the harness opened, identified by the number the guest daemon gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestWindow {
    pub id: u32,
    /// The visible frame as the OS reports it, or `None` when the window no longer exists.
    pub frame: Option<Rect>,
    /// Whether the harness has stopped the window's message processing.
    pub hung: bool,
}

/// One monitor as glazewm reports it, with its workspaces in glazewm's order.
///
/// Identified by its bounds: the device name is renumbered whenever virtual monitors are recreated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitorLayout {
    pub bounds: Rect,
    pub working_area: Rect,
    pub dpi: u32,
    pub has_focus: bool,
    pub workspaces: Vec<WorkspaceLayout>,
}

/// What glazewm answered to one IPC query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlazewmIpc {
    /// No glazewm process was running to ask.
    NotRunning,
    /// It answered `query monitors` with these monitors, ordered by position.
    Answered(Vec<MonitorLayout>),
    /// It accepted the query but did not answer within the scenario's IPC time limit.
    NoAnswer,
}

/// What the harness saw of the guest at one moment, in canonical form.
///
/// Displays and monitors are ordered by position, left to right and then top to bottom, so that two
/// observations of the same state are equal whatever order the guest reported them in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    os_displays: Vec<Rect>,
    test_windows: Vec<TestWindow>,
    glazewm: GlazewmIpc,
    zebar_running: Option<bool>,
}

impl Observation {
    /// Puts the guest OS's display bounds, the test windows and glazewm's answer, in any order, into canonical form.
    pub fn new(
        mut os_displays: Vec<Rect>,
        mut test_windows: Vec<TestWindow>,
        mut glazewm: GlazewmIpc,
    ) -> Self {
        test_windows.sort_by_key(|window| window.id);
        os_displays.sort_by_key(Rect::position);
        if let GlazewmIpc::Answered(monitors) = &mut glazewm {
            monitors.sort_by_key(|monitor| monitor.bounds.position());
        }
        Self {
            os_displays,
            test_windows,
            glazewm,
            zebar_running: None,
        }
    }

    /// Adds whether the zebar the harness deployed was running; an observation without it is of a guest with no zebar.
    pub fn with_zebar_running(mut self, running: bool) -> Self {
        self.zebar_running = Some(running);
        self
    }

    /// The windows the harness opened, ordered by id.
    pub fn test_windows(&self) -> &[TestWindow] {
        &self.test_windows
    }

    /// The bounds of the displays the guest OS reports, ordered by position.
    pub fn os_displays(&self) -> &[Rect] {
        &self.os_displays
    }

    pub fn glazewm(&self) -> &GlazewmIpc {
        &self.glazewm
    }

    /// Whether the deployed zebar was running, or `None` when no zebar was deployed.
    pub fn zebar_running(&self) -> Option<bool> {
        self.zebar_running
    }
}

/// The settled observation after one step (or after start-up, for step 0) and the invariants' verdict on it.
///
/// `observation` is `None` when the step did not settle within its limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepOutcome {
    pub observation: Option<Observation>,
    pub verdict: StepVerdict,
}

/// The conclusion for the state after one step (or after start-up, for step 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepVerdict {
    Passed,
    Violated(Vec<Violation>),
    Unsettled,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32) -> Rect {
        Rect {
            x,
            y,
            width: 800,
            height: 600,
        }
    }

    fn monitor(x: i32, y: i32) -> MonitorLayout {
        MonitorLayout {
            bounds: rect(x, y),
            working_area: rect(x, y),
            dpi: 96,
            has_focus: false,
            workspaces: vec![],
        }
    }

    #[test]
    fn orders_displays_and_monitors_left_to_right_then_top_to_bottom() {
        let observation = Observation::new(
            vec![rect(800, 0), rect(0, 600), rect(0, 0)],
            vec![],
            GlazewmIpc::Answered(vec![monitor(800, 0), monitor(0, 0)]),
        );

        assert_eq!(
            observation.os_displays(),
            [rect(0, 0), rect(0, 600), rect(800, 0)]
        );
        assert_eq!(
            observation.glazewm(),
            &GlazewmIpc::Answered(vec![monitor(0, 0), monitor(800, 0)])
        );
    }

    #[test]
    fn tracks_which_loads_the_steps_so_far_have_left_running() {
        let cpu = Load {
            kind: LoadKind::Cpu,
            percent: 100,
        };
        let memory = Load {
            kind: LoadKind::Memory,
            percent: 70,
        };
        let mut loads = ActiveLoads::default();

        loads.apply(&Step::StartLoad(cpu));
        loads.apply(&Step::StartLoad(memory));
        loads.apply(&Step::StopLoad(LoadKind::Cpu));
        let one_left = loads.any();
        loads.apply(&Step::StopLoad(LoadKind::Memory));

        assert!(one_left);
        assert!(!loads.any());
    }

    #[test]
    fn orders_test_windows_by_id() {
        let window = |id| TestWindow {
            id,
            frame: None,
            hung: false,
        };

        let observation = Observation::new(
            vec![],
            vec![window(2), window(0), window(1)],
            GlazewmIpc::NoAnswer,
        );

        assert_eq!(
            observation.test_windows(),
            [window(0), window(1), window(2)]
        );
    }

    #[test]
    fn observations_of_the_same_state_reported_in_different_orders_are_equal() {
        let one = Observation::new(vec![rect(0, 0), rect(800, 0)], vec![], GlazewmIpc::NoAnswer);
        let other = Observation::new(vec![rect(800, 0), rect(0, 0)], vec![], GlazewmIpc::NoAnswer);

        assert_eq!(one, other);
    }
}
