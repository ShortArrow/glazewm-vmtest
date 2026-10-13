//! What a scenario does to the guest and what each step concluded.

use std::time::Duration;

use crate::invariant::Violation;

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

/// What glazewm answered to one IPC query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlazewmIpc {
    /// It answered `query monitors` with this many monitors.
    Monitors(usize),
    /// It accepted the query but did not answer within the scenario's IPC time limit.
    NoAnswer,
}

/// What the harness saw of the guest at one moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub os_displays: usize,
    pub glazewm: GlazewmIpc,
}

/// The conclusion for the state after one step (or after start-up, for step 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepVerdict {
    Passed,
    Violated(Vec<Violation>),
    Unsettled,
}
