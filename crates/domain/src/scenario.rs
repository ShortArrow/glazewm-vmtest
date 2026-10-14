//! What a scenario does to the guest and what each step concluded.

use std::time::Duration;

use crate::invariant::Violation;
use crate::topology::Rect;

/// One operation of a scenario's When.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Sets how many virtual monitors exist besides the platform's own display.
    SetVirtualMonitorCount(u32),
    /// Removes every virtual monitor at once.
    RemoveVirtualMonitors,
    /// Brings back the virtual monitors removed by `RemoveVirtualMonitors`.
    RestoreVirtualMonitors,
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
    glazewm: GlazewmIpc,
}

impl Observation {
    /// Puts the guest OS's display bounds and glazewm's answer, in any order, into canonical form.
    pub fn new(mut os_displays: Vec<Rect>, mut glazewm: GlazewmIpc) -> Self {
        os_displays.sort_by_key(Rect::position);
        if let GlazewmIpc::Answered(monitors) = &mut glazewm {
            monitors.sort_by_key(|monitor| monitor.bounds.position());
        }
        Self {
            os_displays,
            glazewm,
        }
    }

    /// The bounds of the displays the guest OS reports, ordered by position.
    pub fn os_displays(&self) -> &[Rect] {
        &self.os_displays
    }

    pub fn glazewm(&self) -> &GlazewmIpc {
        &self.glazewm
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
    fn observations_of_the_same_state_reported_in_different_orders_are_equal() {
        let one = Observation::new(vec![rect(0, 0), rect(800, 0)], GlazewmIpc::NoAnswer);
        let other = Observation::new(vec![rect(800, 0), rect(0, 0)], GlazewmIpc::NoAnswer);

        assert_eq!(one, other);
    }
}
