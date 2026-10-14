//! What a scenario observed in a reference run, and how a later run deviates from it.

use crate::scenario::Observation;

/// The settled observations of a reference run, one per step with start-up as step 0.
///
/// `None` is a step that did not settle within its limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Baseline {
    pub sut_commit: String,
    pub steps: Vec<Option<Observation>>,
}

/// A step whose settled observation differs from the baseline's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deviation {
    pub step: usize,
    pub expected: Option<Observation>,
    pub observed: Option<Observation>,
}

/// A baseline that was recorded for a different sequence of steps than the run compared with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleBaseline {
    pub recorded_steps: usize,
    pub run_steps: usize,
}

impl Baseline {
    /// Returns every step of `observed` that differs from the baseline, in step order.
    pub fn deviations(
        &self,
        observed: &[Option<Observation>],
    ) -> Result<Vec<Deviation>, StaleBaseline> {
        if self.steps.len() != observed.len() {
            return Err(StaleBaseline {
                recorded_steps: self.steps.len(),
                run_steps: observed.len(),
            });
        }
        Ok(self
            .steps
            .iter()
            .zip(observed)
            .enumerate()
            .filter(|(_, (expected, observed))| expected != observed)
            .map(|(step, (expected, observed))| Deviation {
                step,
                expected: expected.clone(),
                observed: observed.clone(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::GlazewmIpc;
    use crate::topology::Rect;

    fn observed(displays: i32) -> Option<Observation> {
        let rects = (0..displays)
            .map(|i| Rect {
                x: i * 800,
                y: 0,
                width: 800,
                height: 600,
            })
            .collect();
        Some(Observation::new(rects, vec![], GlazewmIpc::NoAnswer))
    }

    fn baseline(steps: Vec<Option<Observation>>) -> Baseline {
        Baseline {
            sut_commit: "5709ad0a".into(),
            steps,
        }
    }

    #[test]
    fn a_run_that_observes_what_was_recorded_has_no_deviation() {
        let steps = vec![observed(1), observed(3)];

        assert_eq!(baseline(steps.clone()).deviations(&steps), Ok(vec![]));
    }

    #[test]
    fn reports_each_step_whose_observation_differs() {
        let recorded = baseline(vec![observed(1), observed(3), observed(1)]);

        let deviations = recorded.deviations(&[observed(1), observed(2), observed(1)]);

        assert_eq!(
            deviations,
            Ok(vec![Deviation {
                step: 1,
                expected: observed(3),
                observed: observed(2),
            }])
        );
    }

    #[test]
    fn a_step_that_no_longer_settles_is_a_deviation() {
        let recorded = baseline(vec![observed(1), observed(3)]);

        let deviations = recorded.deviations(&[observed(1), None]);

        assert_eq!(
            deviations,
            Ok(vec![Deviation {
                step: 1,
                expected: observed(3),
                observed: None,
            }])
        );
    }

    #[test]
    fn a_baseline_for_a_different_number_of_steps_is_stale() {
        let recorded = baseline(vec![observed(1), observed(3)]);

        assert_eq!(
            recorded.deviations(&[observed(1)]),
            Err(StaleBaseline {
                recorded_steps: 2,
                run_steps: 1,
            })
        );
    }
}
