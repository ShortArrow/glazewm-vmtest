//! Runs a scenario against a guest through ports the adapters implement.

use std::time::Duration;

use vmtest_domain::invariant::{
    every_monitor_displays_one_workspace, glazewm_answers_ipc, glazewm_is_running,
    glazewm_shows_no_dialog, manages_only_existing_top_level_windows, monitor_count_matches,
    tiled_windows_within_working_area, zebar_is_running,
};
use vmtest_domain::scenario::{
    ActiveLoads, GlazewmIpc, Load, LoadKind, Observation, Projection, Scenario, Step, StepOutcome,
    StepVerdict, TestWindowBatch,
};
use vmtest_domain::settle::{SettleState, SettleTracker};

/// Brings the guest back to its snapshot.
pub trait Hypervisor {
    fn restore(&mut self) -> Result<(), String>;
}

/// Reaches into the guest: the guest daemon and the SUT.
pub trait GuestExecutor {
    /// Returns once the guest daemon answers.
    fn wait_until_ready(&mut self) -> Result<(), String>;
    /// Deploys and starts the SUT in the interactive session.
    ///
    /// Returns once glazewm has taken an IPC query, whether or not it answered within `ipc_limit`,
    /// so that a glazewm stalled from start-up is judged rather than waited for.
    fn start_sut(&mut self, ipc_limit: Duration) -> Result<(), String>;
}

/// Changes the guest's monitors.
pub trait DisplayController {
    /// Sets how many virtual monitors the guest has and returns once the guest OS shows that many.
    fn set_virtual_monitor_count(&mut self, count: u32) -> Result<(), String>;
    fn remove_virtual_monitors(&mut self) -> Result<(), String>;
    fn restore_virtual_monitors(&mut self) -> Result<(), String>;
    /// Turns every display off and returns once the guest OS has been asked to.
    fn turn_displays_off(&mut self) -> Result<(), String>;
    /// Wakes the displays with input and returns once the guest OS has been given it.
    fn wake_displays(&mut self) -> Result<(), String>;
    /// Switches to `projection`, as Win+P does, and returns once the guest OS has applied it.
    fn set_projection(&mut self, projection: Projection) -> Result<(), String>;
}

/// Starts and stops what the guest runs besides the SUT.
pub trait WorkloadController {
    /// Opens `count` more test windows and returns once each exists.
    fn open_test_windows(&mut self, batch: &TestWindowBatch) -> Result<(), String>;
    /// Stops test window `id` from processing messages and returns once it has stopped.
    fn hang_test_window(&mut self, id: u32) -> Result<(), String>;
    /// Lets test window `id` process messages again.
    fn resume_test_window(&mut self, id: u32) -> Result<(), String>;
    /// Starts `load`, replacing a running load of the same kind, and returns once it is running.
    fn start_load(&mut self, load: &Load) -> Result<(), String>;
    /// Stops the running load of `kind`, if any, and returns once it has stopped.
    fn stop_load(&mut self, kind: LoadKind) -> Result<(), String>;
}

/// Gives the guest the input a user gives.
pub trait InputController {
    /// Clicks the middle of test window `id` and returns once the guest OS has taken the input.
    fn click_test_window(&mut self, id: u32) -> Result<(), String>;
    /// Presses the keys of `shortcut` and returns once the guest OS has taken the input.
    fn press_shortcut(&mut self, shortcut: &str) -> Result<(), String>;
}

/// Reads the guest's current state.
pub trait Observer {
    /// Observes the guest, giving glazewm `ipc_limit`, measured in the guest, to answer.
    fn observe(&mut self, ipc_limit: Duration) -> Result<Observation, String>;
}

/// Elapsed time, injectable so that settling can be tested without waiting.
pub trait Clock {
    fn now(&self) -> Duration;
    fn sleep(&mut self, duration: Duration);
}

/// How long the guest must stay unchanged, how long a step may take to settle, and how often to look.
#[derive(Clone, Copy, Debug)]
pub struct SettleSettings {
    pub window: Duration,
    pub limit: Duration,
    pub poll: Duration,
}

/// Runs `scenario` and returns the outcome of start-up (step 0) and of each step.
///
/// The Given monitors are in place before the SUT starts. Invariants are judged only on settled states.
pub fn run<G>(
    scenario: &Scenario,
    guest: &mut G,
    clock: &mut dyn Clock,
    settle: SettleSettings,
) -> Result<Vec<StepOutcome>, String>
where
    G: Hypervisor
        + GuestExecutor
        + DisplayController
        + WorkloadController
        + InputController
        + Observer,
{
    guest.restore()?;
    guest.wait_until_ready()?;
    guest.set_virtual_monitor_count(scenario.virtual_monitors)?;
    guest.start_sut(scenario.ipc_limit)?;
    let mut loads = ActiveLoads::default();
    let mut outcomes = vec![settled_outcome(scenario, &loads, guest, clock, settle)?];
    for step in &scenario.steps {
        apply(step, scenario, guest, clock, settle)?;
        loads.apply(step);
        outcomes.push(settled_outcome(scenario, &loads, guest, clock, settle)?);
    }
    Ok(outcomes)
}

fn apply(
    step: &Step,
    scenario: &Scenario,
    guest: &mut (impl DisplayController + WorkloadController + InputController + Observer),
    clock: &mut dyn Clock,
    settle: SettleSettings,
) -> Result<(), String> {
    match step {
        Step::SetVirtualMonitorCount(count) => {
            guest.remove_virtual_monitors()?;
            settled_observation(|| guest.observe(scenario.ipc_limit), clock, settle)?;
            guest.set_virtual_monitor_count(*count)
        }
        Step::SetVirtualMonitorCounts { counts, apart } => {
            for (index, count) in counts.iter().enumerate() {
                if index > 0 {
                    clock.sleep(*apart);
                }
                guest.set_virtual_monitor_count(*count)?;
            }
            Ok(())
        }
        Step::RemoveVirtualMonitors => guest.remove_virtual_monitors(),
        Step::RestoreVirtualMonitors => guest.restore_virtual_monitors(),
        Step::TurnDisplaysOff => guest.turn_displays_off(),
        Step::WakeDisplays => guest.wake_displays(),
        Step::SwitchProjection(projection) => guest.set_projection(*projection),
        Step::OpenTestWindows(batch) => guest.open_test_windows(batch),
        Step::HangTestWindow(id) => guest.hang_test_window(*id),
        Step::ResumeTestWindow(id) => guest.resume_test_window(*id),
        Step::ClickTestWindow(id) => guest.click_test_window(*id),
        Step::PressShortcut(shortcut) => guest.press_shortcut(shortcut),
        Step::StartLoad(load) => guest.start_load(load),
        Step::StopLoad(kind) => guest.stop_load(*kind),
    }
}

fn settled_outcome(
    scenario: &Scenario,
    loads: &ActiveLoads,
    guest: &mut impl Observer,
    clock: &mut dyn Clock,
    settle: SettleSettings,
) -> Result<StepOutcome, String> {
    let observation = settled_observation(|| guest.observe(scenario.ipc_limit), clock, settle)?;
    let verdict = judge(observation.as_ref(), loads, &scenario.allowed_dialogs);
    Ok(StepOutcome {
        observation,
        verdict,
    })
}

/// Polls until nothing has changed for the settle window; `None` when the limit is reached first.
fn settled_observation(
    mut observe: impl FnMut() -> Result<Observation, String>,
    clock: &mut dyn Clock,
    settle: SettleSettings,
) -> Result<Option<Observation>, String> {
    let start = clock.now();
    let mut tracker = SettleTracker::new(settle.window, settle.limit);
    let mut last = observe()?;
    loop {
        match tracker.state(clock.now() - start) {
            SettleState::Settled { .. } => return Ok(Some(last)),
            SettleState::Unsettled => return Ok(None),
            SettleState::Waiting => {}
        }
        clock.sleep(settle.poll);
        let current = observe()?;
        if current != last {
            tracker.record_change(clock.now() - start);
            last = current;
        }
    }
}

/// Judges a settled observation; a glazewm that is not running or did not answer has no monitors to compare.
///
/// zebar's liveness is judged whatever glazewm did.
///
/// While a load runs, a glazewm that does not answer within the IPC limit is not held to it.
fn judge(
    observation: Option<&Observation>,
    loads: &ActiveLoads,
    allowed_dialogs: &[String],
) -> StepVerdict {
    let Some(observation) = observation else {
        return StepVerdict::Unsettled;
    };
    let mut checked = match observation.glazewm() {
        GlazewmIpc::Answered(monitors) => vec![
            monitor_count_matches(observation.os_displays().len(), monitors.len()),
            tiled_windows_within_working_area(monitors, observation.test_windows()),
            manages_only_existing_top_level_windows(monitors),
            every_monitor_displays_one_workspace(monitors),
        ],
        GlazewmIpc::NoAnswer if loads.any() => vec![],
        GlazewmIpc::NoAnswer => vec![glazewm_answers_ipc(observation.glazewm())],
        GlazewmIpc::NotRunning => vec![glazewm_is_running(observation.glazewm())],
    };
    checked.push(glazewm_shows_no_dialog(
        observation.glazewm_dialogs(),
        allowed_dialogs,
    ));
    checked.push(zebar_is_running(observation.zebar_running()));
    let violations: Vec<_> = checked.into_iter().filter_map(Result::err).collect();
    if violations.is_empty() {
        StepVerdict::Passed
    } else {
        StepVerdict::Violated(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmtest_domain::scenario::{
        Load, LoadKind, ManagedWindow, MonitorLayout, TestWindow, TestWindowBatch, WindowLifetime,
        WindowPresence, WorkspaceLayout,
    };
    use vmtest_domain::topology::Rect;

    const SETTLE: SettleSettings = SettleSettings {
        window: Duration::from_secs(5),
        limit: Duration::from_secs(30),
        poll: Duration::from_secs(1),
    };

    struct FakeClock(Duration);

    impl Clock for FakeClock {
        fn now(&self) -> Duration {
            self.0
        }
        fn sleep(&mut self, duration: Duration) {
            self.0 += duration;
        }
    }

    /// A guest that records every operation and answers observations from a script, repeating the last one.
    #[derive(Default)]
    struct FakeGuest {
        log: Vec<String>,
        observations: Vec<Observation>,
        ipc_limits: Vec<Duration>,
    }

    impl FakeGuest {
        fn observing(observations: Vec<Observation>) -> Self {
            Self {
                log: Vec::new(),
                observations,
                ipc_limits: Vec::new(),
            }
        }
    }

    impl Hypervisor for FakeGuest {
        fn restore(&mut self) -> Result<(), String> {
            self.log.push("restore".into());
            Ok(())
        }
    }

    impl GuestExecutor for FakeGuest {
        fn wait_until_ready(&mut self) -> Result<(), String> {
            self.log.push("wait_until_ready".into());
            Ok(())
        }
        fn start_sut(&mut self, ipc_limit: Duration) -> Result<(), String> {
            self.ipc_limits.push(ipc_limit);
            self.log.push("start_sut".into());
            Ok(())
        }
    }

    impl DisplayController for FakeGuest {
        fn set_virtual_monitor_count(&mut self, count: u32) -> Result<(), String> {
            self.log.push(format!("set_virtual_monitor_count {count}"));
            Ok(())
        }
        fn remove_virtual_monitors(&mut self) -> Result<(), String> {
            self.log.push("remove_virtual_monitors".into());
            Ok(())
        }
        fn restore_virtual_monitors(&mut self) -> Result<(), String> {
            self.log.push("restore_virtual_monitors".into());
            Ok(())
        }
        fn turn_displays_off(&mut self) -> Result<(), String> {
            self.log.push("turn_displays_off".into());
            Ok(())
        }
        fn wake_displays(&mut self) -> Result<(), String> {
            self.log.push("wake_displays".into());
            Ok(())
        }
        fn set_projection(&mut self, projection: Projection) -> Result<(), String> {
            self.log.push(format!("set_projection {projection:?}"));
            Ok(())
        }
    }

    impl WorkloadController for FakeGuest {
        fn open_test_windows(&mut self, batch: &TestWindowBatch) -> Result<(), String> {
            self.log.push(format!("open_test_windows {batch:?}"));
            Ok(())
        }
        fn hang_test_window(&mut self, id: u32) -> Result<(), String> {
            self.log.push(format!("hang_test_window {id}"));
            Ok(())
        }
        fn start_load(&mut self, load: &Load) -> Result<(), String> {
            self.log
                .push(format!("start_load {:?} {}", load.kind, load.percent));
            Ok(())
        }
        fn stop_load(&mut self, kind: LoadKind) -> Result<(), String> {
            self.log.push(format!("stop_load {kind:?}"));
            Ok(())
        }
        fn resume_test_window(&mut self, id: u32) -> Result<(), String> {
            self.log.push(format!("resume_test_window {id}"));
            Ok(())
        }
    }

    impl InputController for FakeGuest {
        fn click_test_window(&mut self, id: u32) -> Result<(), String> {
            self.log.push(format!("click_test_window {id}"));
            Ok(())
        }
        fn press_shortcut(&mut self, shortcut: &str) -> Result<(), String> {
            self.log.push(format!("press_shortcut {shortcut}"));
            Ok(())
        }
    }

    impl Observer for FakeGuest {
        fn observe(&mut self, ipc_limit: Duration) -> Result<Observation, String> {
            self.ipc_limits.push(ipc_limit);
            if self.observations.len() > 1 {
                Ok(self.observations.remove(0))
            } else {
                Ok(self.observations[0].clone())
            }
        }
    }

    fn rects(count: usize) -> Vec<Rect> {
        (0..count)
            .map(|i| Rect {
                x: i as i32 * 800,
                y: 0,
                width: 800,
                height: 600,
            })
            .collect()
    }

    fn observed(os_displays: usize, glazewm_monitors: usize) -> Observation {
        let monitors = rects(glazewm_monitors)
            .into_iter()
            .enumerate()
            .map(|(index, bounds)| MonitorLayout {
                bounds,
                working_area: bounds,
                dpi: 96,
                has_focus: false,
                workspaces: vec![WorkspaceLayout {
                    name: (index + 1).to_string(),
                    is_displayed: true,
                    has_focus: false,
                    tiling_direction: "horizontal".into(),
                    windows: vec![],
                }],
            })
            .collect();
        Observation::new(rects(os_displays), vec![], GlazewmIpc::Answered(monitors))
    }

    fn unanswered(os_displays: usize) -> Observation {
        Observation::new(rects(os_displays), vec![], GlazewmIpc::NoAnswer)
    }

    fn not_running(os_displays: usize) -> Observation {
        Observation::new(rects(os_displays), vec![], GlazewmIpc::NotRunning)
    }

    fn verdicts(outcomes: Vec<StepOutcome>) -> Vec<StepVerdict> {
        outcomes
            .into_iter()
            .map(|outcome| outcome.verdict)
            .collect()
    }

    fn scenario(steps: Vec<Step>) -> Scenario {
        Scenario {
            name: "test".into(),
            virtual_monitors: 1,
            steps,
            ipc_limit: Duration::from_secs(2),
            allowed_dialogs: vec![],
        }
    }

    #[test]
    fn applies_the_given_monitors_before_starting_the_sut() {
        let mut guest = FakeGuest::observing(vec![observed(2, 2)]);

        run(
            &scenario(vec![Step::SetVirtualMonitorCount(3)]),
            &mut guest,
            &mut FakeClock(Duration::ZERO),
            SETTLE,
        )
        .unwrap();

        assert_eq!(
            guest.log,
            [
                "restore",
                "wait_until_ready",
                "set_virtual_monitor_count 1",
                "start_sut",
                "remove_virtual_monitors",
                "set_virtual_monitor_count 3"
            ]
        );
    }

    fn run_one_step(step: Step) -> Duration {
        let mut guest = FakeGuest::observing(vec![observed(2, 2)]);
        let mut clock = FakeClock(Duration::ZERO);

        run(&scenario(vec![step]), &mut guest, &mut clock, SETTLE).unwrap();

        clock.0
    }

    #[test]
    fn lets_the_sut_settle_with_every_virtual_monitor_gone_before_setting_a_count() {
        let removing = run_one_step(Step::RemoveVirtualMonitors);
        let setting = run_one_step(Step::SetVirtualMonitorCount(3));

        assert_eq!(setting - removing, SETTLE.window);
    }

    fn run_quick_succession(apart: Duration) -> (Vec<String>, usize, Duration) {
        let mut guest = FakeGuest::observing(vec![observed(3, 3)]);
        let mut clock = FakeClock(Duration::ZERO);
        let step = Step::SetVirtualMonitorCounts {
            counts: vec![0, 1, 2],
            apart,
        };

        let outcomes = run(&scenario(vec![step]), &mut guest, &mut clock, SETTLE).unwrap();

        (guest.log, outcomes.len(), clock.0)
    }

    #[test]
    fn sets_each_count_in_turn_and_settles_only_after_the_last() {
        let (log, outcomes, _) = run_quick_succession(Duration::ZERO);

        assert_eq!(
            log[4..],
            [
                "set_virtual_monitor_count 0",
                "set_virtual_monitor_count 1",
                "set_virtual_monitor_count 2"
            ]
        );
        assert_eq!(outcomes, 2);
    }

    #[test]
    fn waits_the_given_time_between_counts_set_in_quick_succession() {
        let (_, _, back_to_back) = run_quick_succession(Duration::ZERO);
        let (_, _, spaced) = run_quick_succession(Duration::from_secs(3));

        assert_eq!(spaced - back_to_back, Duration::from_secs(6));
    }

    #[test]
    fn turns_the_displays_off_and_wakes_them_as_steps() {
        let mut guest = FakeGuest::observing(vec![observed(2, 2)]);

        run(
            &scenario(vec![Step::TurnDisplaysOff, Step::WakeDisplays]),
            &mut guest,
            &mut FakeClock(Duration::ZERO),
            SETTLE,
        )
        .unwrap();

        assert_eq!(guest.log[4..], ["turn_displays_off", "wake_displays"]);
    }

    #[test]
    fn opens_test_windows_as_a_step() {
        let mut guest = FakeGuest::observing(vec![observed(2, 2)]);

        run(
            &scenario(vec![Step::OpenTestWindows(TestWindowBatch {
                count: 3,
                within: Some(Duration::from_millis(200)),
                lifetime: WindowLifetime::DestroyedAfter(Duration::ZERO),
            })]),
            &mut guest,
            &mut FakeClock(Duration::ZERO),
            SETTLE,
        )
        .unwrap();

        assert_eq!(
            guest.log.last().unwrap(),
            "open_test_windows TestWindowBatch { count: 3, within: Some(200ms), lifetime: DestroyedAfter(0ns) }"
        );
    }

    #[test]
    fn switches_the_projection_as_a_step() {
        let mut guest = FakeGuest::observing(vec![observed(2, 2)]);

        run(
            &scenario(vec![
                Step::SwitchProjection(Projection::PcScreenOnly),
                Step::SwitchProjection(Projection::Extend),
            ]),
            &mut guest,
            &mut FakeClock(Duration::ZERO),
            SETTLE,
        )
        .unwrap();

        assert_eq!(
            guest.log[guest.log.len() - 2..],
            ["set_projection PcScreenOnly", "set_projection Extend"]
        );
    }

    #[test]
    fn presses_a_shortcut_as_a_step() {
        let mut guest = FakeGuest::observing(vec![observed(2, 2)]);

        run(
            &scenario(vec![Step::PressShortcut("alt+shift+f".into())]),
            &mut guest,
            &mut FakeClock(Duration::ZERO),
            SETTLE,
        )
        .unwrap();

        assert_eq!(guest.log.last().unwrap(), "press_shortcut alt+shift+f");
    }

    #[test]
    fn clicks_a_test_window_as_a_step() {
        let mut guest = FakeGuest::observing(vec![observed(2, 2)]);

        run(
            &scenario(vec![
                Step::OpenTestWindows(TestWindowBatch::persistent(2)),
                Step::ClickTestWindow(0),
            ]),
            &mut guest,
            &mut FakeClock(Duration::ZERO),
            SETTLE,
        )
        .unwrap();

        assert_eq!(guest.log.last().unwrap(), "click_test_window 0");
    }

    #[test]
    fn stops_and_resumes_a_test_window_as_steps() {
        let mut guest = FakeGuest::observing(vec![observed(2, 2)]);

        run(
            &scenario(vec![
                Step::OpenTestWindows(TestWindowBatch::persistent(2)),
                Step::HangTestWindow(1),
                Step::ResumeTestWindow(1),
            ]),
            &mut guest,
            &mut FakeClock(Duration::ZERO),
            SETTLE,
        )
        .unwrap();

        assert_eq!(
            guest.log[guest.log.len() - 3..],
            [
                "open_test_windows TestWindowBatch { count: 2, within: None, lifetime: Persistent }",
                "hang_test_window 1",
                "resume_test_window 1"
            ]
        );
    }

    #[test]
    fn reports_every_invariant_a_settled_state_violates() {
        let reaching_past_its_monitor = Rect {
            x: 700,
            y: 0,
            width: 800,
            height: 600,
        };
        let monitor = MonitorLayout {
            bounds: rects(1)[0],
            working_area: rects(1)[0],
            dpi: 96,
            has_focus: true,
            workspaces: vec![WorkspaceLayout {
                name: "1".into(),
                is_displayed: true,
                has_focus: true,
                tiling_direction: "horizontal".into(),
                windows: vec![
                    ManagedWindow {
                        test_window: Some(0),
                        state: "tiling".into(),
                        presence: WindowPresence::TopLevel,
                    },
                    ManagedWindow {
                        test_window: None,
                        state: "tiling".into(),
                        presence: WindowPresence::Gone,
                    },
                ],
            }],
        };
        let without_workspace = MonitorLayout {
            bounds: rects(2)[1],
            working_area: rects(2)[1],
            dpi: 96,
            has_focus: false,
            workspaces: vec![],
        };
        let observation = Observation::new(
            rects(3),
            vec![TestWindow {
                id: 0,
                frame: Some(reaching_past_its_monitor),
                hung: false,
            }],
            GlazewmIpc::Answered(vec![monitor, without_workspace]),
        );
        let mut guest = FakeGuest::observing(vec![observation]);

        let verdicts = verdicts(
            run(
                &scenario(vec![]),
                &mut guest,
                &mut FakeClock(Duration::ZERO),
                SETTLE,
            )
            .unwrap(),
        );

        let StepVerdict::Violated(violations) = &verdicts[0] else {
            panic!("expected violations, got {verdicts:?}");
        };
        let invariants: Vec<_> = violations.iter().map(|v| v.invariant).collect();
        assert_eq!(
            invariants,
            [
                "monitor_count_matches",
                "tiled_windows_within_working_area",
                "manages_only_existing_top_level_windows",
                "every_monitor_displays_one_workspace"
            ]
        );
    }

    #[test]
    fn passes_start_up_and_every_step_when_glazewm_tracks_the_displays() {
        let mut guest = FakeGuest::observing(vec![observed(2, 2)]);

        let verdicts = verdicts(
            run(
                &scenario(vec![Step::RemoveVirtualMonitors]),
                &mut guest,
                &mut FakeClock(Duration::ZERO),
                SETTLE,
            )
            .unwrap(),
        );

        assert_eq!(verdicts, [StepVerdict::Passed, StepVerdict::Passed]);
    }

    #[test]
    fn reports_a_step_after_which_glazewm_misses_a_display() {
        let mut observations = vec![observed(2, 2); 6];
        observations.push(observed(4, 3));
        let mut guest = FakeGuest::observing(observations);

        let verdicts = verdicts(
            run(
                &scenario(vec![Step::SetVirtualMonitorCount(3)]),
                &mut guest,
                &mut FakeClock(Duration::ZERO),
                SETTLE,
            )
            .unwrap(),
        );

        assert_eq!(verdicts[0], StepVerdict::Passed);
        assert!(
            matches!(&verdicts[1], StepVerdict::Violated(v) if v[0].invariant == "monitor_count_matches")
        );
    }

    #[test]
    fn reports_a_step_that_keeps_changing_until_the_limit_as_unsettled() {
        let mut observations = vec![observed(2, 2); 6];
        observations.extend((0..40).map(|i| observed(2 + i % 2, 2 + i % 2)));
        let mut guest = FakeGuest::observing(observations);

        let verdicts = verdicts(
            run(
                &scenario(vec![Step::RestoreVirtualMonitors]),
                &mut guest,
                &mut FakeClock(Duration::ZERO),
                SETTLE,
            )
            .unwrap(),
        );

        assert_eq!(verdicts, [StepVerdict::Passed, StepVerdict::Unsettled]);
    }

    #[test]
    fn reports_a_glazewm_that_stops_answering_and_runs_the_remaining_steps() {
        let mut observations = vec![observed(2, 2); 6];
        observations.extend(vec![unanswered(4); 6]);
        observations.push(observed(2, 2));
        let mut guest = FakeGuest::observing(observations);

        let verdicts = verdicts(
            run(
                &scenario(vec![
                    Step::RemoveVirtualMonitors,
                    Step::RestoreVirtualMonitors,
                ]),
                &mut guest,
                &mut FakeClock(Duration::ZERO),
                SETTLE,
            )
            .unwrap(),
        );

        assert_eq!(verdicts[0], StepVerdict::Passed);
        assert!(
            matches!(&verdicts[1], StepVerdict::Violated(v) if v.len() == 1 && v[0].invariant == "glazewm_answers_ipc")
        );
        assert_eq!(verdicts[2], StepVerdict::Passed);
    }

    #[test]
    fn reports_a_glazewm_that_exits_as_not_running_alone_and_runs_the_remaining_steps() {
        let mut observations = vec![observed(2, 2); 6];
        observations.extend(vec![not_running(4); 6]);
        let mut guest = FakeGuest::observing(observations);

        let verdicts = verdicts(
            run(
                &scenario(vec![
                    Step::SetVirtualMonitorCount(3),
                    Step::SetVirtualMonitorCount(1),
                ]),
                &mut guest,
                &mut FakeClock(Duration::ZERO),
                SETTLE,
            )
            .unwrap(),
        );

        let not_running_alone = |verdict: &StepVerdict| matches!(verdict, StepVerdict::Violated(v) if v.len() == 1 && v[0].invariant == "glazewm_is_running");
        assert_eq!(verdicts[0], StepVerdict::Passed);
        assert!(not_running_alone(&verdicts[1]));
        assert!(not_running_alone(&verdicts[2]));
    }

    #[test]
    fn reports_a_zebar_that_exits_while_glazewm_keeps_tracking_the_displays() {
        let mut observations = vec![observed(2, 2).with_zebar_running(true); 6];
        observations.extend(vec![observed(2, 2).with_zebar_running(false); 6]);
        let mut guest = FakeGuest::observing(observations);

        let verdicts = verdicts(
            run(
                &scenario(vec![Step::TurnDisplaysOff]),
                &mut guest,
                &mut FakeClock(Duration::ZERO),
                SETTLE,
            )
            .unwrap(),
        );

        assert_eq!(verdicts[0], StepVerdict::Passed);
        assert!(
            matches!(&verdicts[1], StepVerdict::Violated(v) if v.len() == 1 && v[0].invariant == "zebar_is_running")
        );
    }

    #[test]
    fn reports_a_glazewm_dialog_whether_or_not_glazewm_still_answers() {
        let dialog = || vec!["Non-fatal error: No displayed workspace.".to_string()];
        let mut observations = vec![observed(2, 2); 6];
        observations.extend(vec![observed(2, 2).with_glazewm_dialogs(dialog()); 6]);
        observations.extend(vec![unanswered(2).with_glazewm_dialogs(dialog()); 6]);
        let mut guest = FakeGuest::observing(observations);

        let verdicts = verdicts(
            run(
                &scenario(vec![Step::TurnDisplaysOff, Step::WakeDisplays]),
                &mut guest,
                &mut FakeClock(Duration::ZERO),
                SETTLE,
            )
            .unwrap(),
        );

        let invariants = |verdict: &StepVerdict| match verdict {
            StepVerdict::Violated(violations) => violations.iter().map(|v| v.invariant).collect(),
            _ => vec![],
        };
        assert_eq!(
            verdicts.iter().map(invariants).collect::<Vec<_>>(),
            [
                vec![],
                vec!["glazewm_shows_no_dialog"],
                vec!["glazewm_answers_ipc", "glazewm_shows_no_dialog"]
            ]
        );
    }

    #[test]
    fn does_not_report_a_glazewm_dialog_the_scenario_allows() {
        let dialog = "Non-fatal error: Shell exec failed for 'zebar'.".to_string();
        let mut guest =
            FakeGuest::observing(vec![observed(2, 2).with_glazewm_dialogs(vec![dialog])]);
        let scenario = Scenario {
            allowed_dialogs: vec!["Non-fatal error: Shell exec failed".into()],
            ..scenario(vec![])
        };

        let verdicts = verdicts(
            run(
                &scenario,
                &mut guest,
                &mut FakeClock(Duration::ZERO),
                SETTLE,
            )
            .unwrap(),
        );

        assert_eq!(verdicts, [StepVerdict::Passed]);
    }

    #[test]
    fn does_not_hold_glazewm_to_the_ipc_limit_while_a_load_runs() {
        let mut observations = vec![observed(2, 2); 6];
        observations.extend(vec![unanswered(2); 12]);
        let mut guest = FakeGuest::observing(observations);
        let cpu = Load {
            kind: LoadKind::Cpu,
            percent: 100,
        };

        let verdicts = verdicts(
            run(
                &scenario(vec![Step::StartLoad(cpu), Step::StopLoad(LoadKind::Cpu)]),
                &mut guest,
                &mut FakeClock(Duration::ZERO),
                SETTLE,
            )
            .unwrap(),
        );

        assert_eq!(verdicts[..2], [StepVerdict::Passed, StepVerdict::Passed]);
        assert!(
            matches!(&verdicts[2], StepVerdict::Violated(v) if v[0].invariant == "glazewm_answers_ipc")
        );
        assert_eq!(
            guest.log[guest.log.len() - 2..],
            ["start_load Cpu 100", "stop_load Cpu"]
        );
    }

    #[test]
    fn returns_the_settled_observation_of_each_step() {
        let mut observations = vec![observed(2, 2); 6];
        observations.push(observed(4, 4));
        let mut guest = FakeGuest::observing(observations);

        let outcomes = run(
            &scenario(vec![Step::SetVirtualMonitorCount(3)]),
            &mut guest,
            &mut FakeClock(Duration::ZERO),
            SETTLE,
        )
        .unwrap();

        let settled: Vec<_> = outcomes
            .into_iter()
            .map(|outcome| outcome.observation)
            .collect();
        assert_eq!(settled, [Some(observed(2, 2)), Some(observed(4, 4))]);
    }

    #[test]
    fn gives_the_scenario_ipc_limit_to_every_query_of_glazewm() {
        let mut guest = FakeGuest::observing(vec![observed(2, 2)]);
        let mut scenario = scenario(vec![]);
        scenario.ipc_limit = Duration::from_millis(1500);

        run(
            &scenario,
            &mut guest,
            &mut FakeClock(Duration::ZERO),
            SETTLE,
        )
        .unwrap();

        assert!(!guest.ipc_limits.is_empty());
        assert!(
            guest
                .ipc_limits
                .iter()
                .all(|limit| *limit == Duration::from_millis(1500))
        );
    }
}
