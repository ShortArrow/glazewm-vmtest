//! What a scenario does to the guest and what each step concluded.

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
}

/// What the harness saw of the guest at one moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub os_displays: usize,
    pub glazewm_monitors: usize,
}

/// The conclusion for the state after one step (or after start-up, for step 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepVerdict {
    Passed,
    Violated(Vec<Violation>),
    Unsettled,
}
