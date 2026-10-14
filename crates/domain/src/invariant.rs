//! Conditions that must hold on a settled guest whatever the SUT is meant to do.

use crate::scenario::{GlazewmIpc, MonitorLayout, TestWindow};

/// A violated invariant, with what was expected and what was observed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub invariant: &'static str,
    pub expected: String,
    pub observed: String,
}

/// Checks that glazewm tracks exactly the displays the guest OS reports.
///
/// While the guest OS reports no display, glazewm keeps its last monitor, so one monitor is expected.
pub fn monitor_count_matches(os_displays: usize, glazewm_monitors: usize) -> Result<(), Violation> {
    let expected = os_displays.max(1);
    if glazewm_monitors == expected {
        return Ok(());
    }
    Err(Violation {
        invariant: "monitor_count_matches",
        expected: format!("{expected} monitors"),
        observed: format!("{glazewm_monitors} monitors"),
    })
}

/// Checks that every tiled test window on a displayed workspace lies within its monitor's working area.
///
/// The frame compared is the one the OS reports, not the one glazewm meant to set. Windows on hidden workspaces
/// are where glazewm's hide method leaves them, and a window that does not process messages cannot be moved,
/// so neither is compared; that the others still hold is what a hung window must not change.
pub fn tiled_windows_within_working_area(
    monitors: &[MonitorLayout],
    test_windows: &[TestWindow],
) -> Result<(), Violation> {
    let mut expected = Vec::new();
    let mut observed = Vec::new();
    for monitor in monitors {
        let tiled = monitor
            .workspaces
            .iter()
            .filter(|workspace| workspace.is_displayed)
            .flat_map(|workspace| &workspace.windows)
            .filter(|window| window.state == "tiling")
            .filter_map(|window| window.test_window);
        for id in tiled {
            let frame = test_windows
                .iter()
                .find(|window| window.id == id && !window.hung)
                .and_then(|window| window.frame);
            if let Some(frame) = frame.filter(|frame| !monitor.working_area.contains(frame)) {
                expected.push(format!("test window {id} within {}", monitor.working_area));
                observed.push(format!("test window {id} at {frame}"));
            }
        }
    }
    if expected.is_empty() {
        return Ok(());
    }
    Err(Violation {
        invariant: "tiled_windows_within_working_area",
        expected: expected.join("; "),
        observed: observed.join("; "),
    })
}

/// Checks that a glazewm process was running when the guest was observed.
pub fn glazewm_is_running(answer: &GlazewmIpc) -> Result<(), Violation> {
    match answer {
        GlazewmIpc::NotRunning => Err(Violation {
            invariant: "glazewm_is_running",
            expected: "a running glazewm process".into(),
            observed: "no glazewm process".into(),
        }),
        _ => Ok(()),
    }
}

/// Checks that glazewm answered the IPC query within the scenario's time limit.
///
/// A glazewm that is not running was not asked, so `glazewm_is_running` judges it instead.
pub fn glazewm_answers_ipc(answer: &GlazewmIpc) -> Result<(), Violation> {
    match answer {
        GlazewmIpc::Answered(_) | GlazewmIpc::NotRunning => Ok(()),
        GlazewmIpc::NoAnswer => Err(Violation {
            invariant: "glazewm_answers_ipc",
            expected: "an answer within the IPC time limit".into(),
            observed: "no answer".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::scenario::{ManagedWindow, MonitorLayout, TestWindow, WorkspaceLayout};
    use crate::topology::Rect;

    const WORKING_AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 1280,
        height: 760,
    };

    const INSIDE: Rect = Rect {
        x: 20,
        y: 20,
        width: 600,
        height: 720,
    };

    const PAST_RIGHT: Rect = Rect {
        x: 1000,
        y: 20,
        width: 600,
        height: 720,
    };

    fn managed(id: u32, state: &str) -> ManagedWindow {
        ManagedWindow {
            test_window: Some(id),
            state: state.into(),
        }
    }

    fn monitor_showing(windows: Vec<ManagedWindow>, is_displayed: bool) -> MonitorLayout {
        MonitorLayout {
            bounds: Rect {
                height: 800,
                ..WORKING_AREA
            },
            working_area: WORKING_AREA,
            dpi: 96,
            has_focus: true,
            workspaces: vec![WorkspaceLayout {
                name: "1".into(),
                is_displayed,
                has_focus: is_displayed,
                tiling_direction: "horizontal".into(),
                windows,
            }],
        }
    }

    fn test_window(id: u32, frame: Rect) -> TestWindow {
        TestWindow {
            id,
            frame: Some(frame),
            hung: false,
        }
    }

    #[test]
    fn holds_when_every_tiled_test_window_lies_within_its_working_area() {
        let monitors = [monitor_showing(vec![managed(1, "tiling")], true)];

        assert_eq!(
            tiled_windows_within_working_area(&monitors, &[test_window(1, INSIDE)]),
            Ok(())
        );
    }

    #[test]
    fn is_violated_by_a_tiled_test_window_reaching_past_its_working_area() {
        let monitors = [monitor_showing(
            vec![managed(1, "tiling"), managed(2, "tiling")],
            true,
        )];
        let windows = [test_window(1, INSIDE), test_window(2, PAST_RIGHT)];

        assert_eq!(
            tiled_windows_within_working_area(&monitors, &windows),
            Err(Violation {
                invariant: "tiled_windows_within_working_area",
                expected: "test window 2 within 1280x760 at (0, 0)".into(),
                observed: "test window 2 at 600x720 at (1000, 20)".into(),
            })
        );
    }

    #[test]
    fn leaves_floating_windows_and_hidden_workspaces_alone() {
        let monitors = [
            monitor_showing(vec![managed(1, "floating")], true),
            monitor_showing(vec![managed(2, "tiling")], false),
        ];
        let windows = [test_window(1, PAST_RIGHT), test_window(2, PAST_RIGHT)];

        assert_eq!(
            tiled_windows_within_working_area(&monitors, &windows),
            Ok(())
        );
    }

    #[test]
    fn leaves_a_window_that_does_not_process_messages_alone() {
        let monitors = [monitor_showing(vec![managed(1, "tiling")], true)];
        let hung = TestWindow {
            hung: true,
            ..test_window(1, PAST_RIGHT)
        };

        assert_eq!(
            tiled_windows_within_working_area(&monitors, &[hung]),
            Ok(())
        );
    }

    #[test]
    fn leaves_windows_without_a_frame_to_compare_alone() {
        let monitors = [monitor_showing(
            vec![
                ManagedWindow {
                    test_window: None,
                    state: "tiling".into(),
                },
                managed(3, "tiling"),
            ],
            true,
        )];
        let gone = TestWindow {
            id: 3,
            frame: None,
            hung: false,
        };

        assert_eq!(
            tiled_windows_within_working_area(&monitors, &[gone]),
            Ok(())
        );
    }

    #[test]
    fn holds_while_glazewm_is_running() {
        assert_eq!(glazewm_is_running(&GlazewmIpc::NoAnswer), Ok(()));
    }

    #[test]
    fn is_violated_when_glazewm_is_not_running() {
        assert_eq!(
            glazewm_is_running(&GlazewmIpc::NotRunning),
            Err(Violation {
                invariant: "glazewm_is_running",
                expected: "a running glazewm process".into(),
                observed: "no glazewm process".into(),
            })
        );
    }

    #[test]
    fn holds_when_glazewm_answers_ipc() {
        assert_eq!(glazewm_answers_ipc(&GlazewmIpc::Answered(vec![])), Ok(()));
    }

    #[test]
    fn is_violated_when_glazewm_does_not_answer_ipc_in_time() {
        assert_eq!(
            glazewm_answers_ipc(&GlazewmIpc::NoAnswer),
            Err(Violation {
                invariant: "glazewm_answers_ipc",
                expected: "an answer within the IPC time limit".into(),
                observed: "no answer".into(),
            })
        );
    }

    #[test]
    fn holds_when_glazewm_tracks_every_display() {
        assert_eq!(monitor_count_matches(2, 2), Ok(()));
    }

    #[test]
    fn is_violated_when_glazewm_misses_a_display() {
        assert_eq!(
            monitor_count_matches(2, 1),
            Err(Violation {
                invariant: "monitor_count_matches",
                expected: "2 monitors".into(),
                observed: "1 monitors".into(),
            })
        );
    }

    #[test]
    fn expects_one_monitor_while_all_displays_are_gone() {
        assert_eq!(monitor_count_matches(0, 1), Ok(()));
    }
}
