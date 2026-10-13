//! Conditions that must hold on a settled guest whatever the SUT is meant to do.

use crate::scenario::GlazewmIpc;

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

/// Checks that glazewm answered the IPC query within the scenario's time limit.
pub fn glazewm_answers_ipc(answer: &GlazewmIpc) -> Result<(), Violation> {
    match answer {
        GlazewmIpc::Monitors(_) => Ok(()),
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

    #[test]
    fn holds_when_glazewm_answers_ipc() {
        assert_eq!(glazewm_answers_ipc(&GlazewmIpc::Monitors(2)), Ok(()));
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
