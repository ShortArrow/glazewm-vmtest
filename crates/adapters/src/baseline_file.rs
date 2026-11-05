//! The JSON file a baseline is stored in, kept apart from the domain types so that renaming one
//! of them does not change the files already recorded.

use serde::{Deserialize, Serialize};
use vmtest_domain::baseline::Baseline;
use vmtest_domain::scenario::{
    GlazewmIpc, ManagedWindow, MonitorLayout, Observation, TestWindow, WindowPresence,
    WorkspaceLayout,
};
use vmtest_domain::topology::Rect;

/// Writes `baseline` as pretty-printed JSON ending with a newline, so that a re-recording reads as a diff.
pub fn to_json(baseline: &Baseline) -> String {
    let file = BaselineFile {
        sut_commit: baseline.sut_commit.clone(),
        steps: baseline
            .steps
            .iter()
            .map(|step| step.as_ref().map(ObservationFile::from))
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&file)
        .expect("baseline files hold no maps with non-string keys");
    text.push('\n');
    text
}

/// Reads a baseline written by `to_json`.
///
/// A field it does not know is an error: a baseline that cannot be compared in full is recorded again.
pub fn from_json(text: &str) -> Result<Baseline, String> {
    let file: BaselineFile =
        serde_json::from_str(text).map_err(|error| format!("invalid baseline file: {error}"))?;
    Ok(Baseline {
        sut_commit: file.sut_commit,
        steps: file
            .steps
            .into_iter()
            .map(|step| step.map(Observation::from))
            .collect(),
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineFile {
    sut_commit: String,
    steps: Vec<Option<ObservationFile>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationFile {
    os_displays: Vec<RectFile>,
    test_windows: Vec<TestWindowFile>,
    glazewm: GlazewmFile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    zebar_running: Option<bool>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestWindowFile {
    id: u32,
    frame: Option<RectFile>,
    hung: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowFile {
    test_window: Option<u32>,
    state: String,
    presence: PresenceFile,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PresenceFile {
    TopLevel,
    Child,
    Gone,
}

impl From<WindowPresence> for PresenceFile {
    fn from(presence: WindowPresence) -> Self {
        match presence {
            WindowPresence::TopLevel => Self::TopLevel,
            WindowPresence::Child => Self::Child,
            WindowPresence::Gone => Self::Gone,
        }
    }
}

impl From<PresenceFile> for WindowPresence {
    fn from(file: PresenceFile) -> Self {
        match file {
            PresenceFile::TopLevel => Self::TopLevel,
            PresenceFile::Child => Self::Child,
            PresenceFile::Gone => Self::Gone,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case", deny_unknown_fields)]
enum GlazewmFile {
    Answered { monitors: Vec<MonitorFile> },
    NoAnswer,
    NotRunning,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MonitorFile {
    bounds: RectFile,
    working_area: RectFile,
    dpi: u32,
    has_focus: bool,
    workspaces: Vec<WorkspaceFile>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceFile {
    name: String,
    is_displayed: bool,
    has_focus: bool,
    tiling_direction: String,
    windows: Vec<WindowFile>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RectFile {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl From<&Observation> for ObservationFile {
    fn from(observation: &Observation) -> Self {
        Self {
            os_displays: observation
                .os_displays()
                .iter()
                .map(RectFile::from)
                .collect(),
            test_windows: observation
                .test_windows()
                .iter()
                .map(|window| TestWindowFile {
                    id: window.id,
                    frame: window.frame.as_ref().map(RectFile::from),
                    hung: window.hung,
                })
                .collect(),
            glazewm: match observation.glazewm() {
                GlazewmIpc::Answered(monitors) => GlazewmFile::Answered {
                    monitors: monitors.iter().map(MonitorFile::from).collect(),
                },
                GlazewmIpc::NoAnswer => GlazewmFile::NoAnswer,
                GlazewmIpc::NotRunning => GlazewmFile::NotRunning,
            },
            zebar_running: observation.zebar_running(),
        }
    }
}

impl From<ObservationFile> for Observation {
    fn from(file: ObservationFile) -> Self {
        let glazewm = match file.glazewm {
            GlazewmFile::Answered { monitors } => {
                GlazewmIpc::Answered(monitors.into_iter().map(MonitorLayout::from).collect())
            }
            GlazewmFile::NoAnswer => GlazewmIpc::NoAnswer,
            GlazewmFile::NotRunning => GlazewmIpc::NotRunning,
        };
        let observation = Observation::new(
            file.os_displays.into_iter().map(Rect::from).collect(),
            file.test_windows
                .into_iter()
                .map(|window| TestWindow {
                    id: window.id,
                    frame: window.frame.map(Rect::from),
                    hung: window.hung,
                })
                .collect(),
            glazewm,
        );
        match file.zebar_running {
            Some(running) => observation.with_zebar_running(running),
            None => observation,
        }
    }
}

impl From<&MonitorLayout> for MonitorFile {
    fn from(monitor: &MonitorLayout) -> Self {
        Self {
            bounds: RectFile::from(&monitor.bounds),
            working_area: RectFile::from(&monitor.working_area),
            dpi: monitor.dpi,
            has_focus: monitor.has_focus,
            workspaces: monitor.workspaces.iter().map(WorkspaceFile::from).collect(),
        }
    }
}

impl From<MonitorFile> for MonitorLayout {
    fn from(file: MonitorFile) -> Self {
        Self {
            bounds: Rect::from(file.bounds),
            working_area: Rect::from(file.working_area),
            dpi: file.dpi,
            has_focus: file.has_focus,
            workspaces: file
                .workspaces
                .into_iter()
                .map(WorkspaceLayout::from)
                .collect(),
        }
    }
}

impl From<&WorkspaceLayout> for WorkspaceFile {
    fn from(workspace: &WorkspaceLayout) -> Self {
        Self {
            name: workspace.name.clone(),
            is_displayed: workspace.is_displayed,
            has_focus: workspace.has_focus,
            tiling_direction: workspace.tiling_direction.clone(),
            windows: workspace
                .windows
                .iter()
                .map(|window| WindowFile {
                    test_window: window.test_window,
                    state: window.state.clone(),
                    presence: window.presence.into(),
                })
                .collect(),
        }
    }
}

impl From<WorkspaceFile> for WorkspaceLayout {
    fn from(file: WorkspaceFile) -> Self {
        Self {
            name: file.name,
            is_displayed: file.is_displayed,
            has_focus: file.has_focus,
            tiling_direction: file.tiling_direction,
            windows: file
                .windows
                .into_iter()
                .map(|window| ManagedWindow {
                    test_window: window.test_window,
                    state: window.state,
                    presence: window.presence.into(),
                })
                .collect(),
        }
    }
}

impl From<&Rect> for RectFile {
    fn from(rect: &Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

impl From<RectFile> for Rect {
    fn from(file: RectFile) -> Self {
        Self {
            x: file.x,
            y: file.y,
            width: file.width,
            height: file.height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmtest_domain::scenario::{
        GlazewmIpc, ManagedWindow, MonitorLayout, Observation, TestWindow, WindowPresence,
        WorkspaceLayout,
    };
    use vmtest_domain::topology::Rect;

    const RECT: Rect = Rect {
        x: 0,
        y: 0,
        width: 1280,
        height: 800,
    };

    fn answered() -> Observation {
        Observation::new(
            vec![RECT],
            vec![
                TestWindow {
                    id: 0,
                    frame: Some(RECT),
                    hung: false,
                },
                TestWindow {
                    id: 1,
                    frame: None,
                    hung: true,
                },
            ],
            GlazewmIpc::Answered(vec![MonitorLayout {
                bounds: RECT,
                working_area: RECT,
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
                            state: "floating".into(),
                            presence: WindowPresence::Gone,
                        },
                    ],
                }],
            }]),
        )
    }

    #[test]
    fn a_baseline_survives_a_round_trip() {
        let baseline = Baseline {
            sut_commit: "5709ad0a".into(),
            steps: vec![
                Some(answered()),
                None,
                Some(Observation::new(vec![], vec![], GlazewmIpc::NoAnswer)),
                Some(Observation::new(vec![RECT], vec![], GlazewmIpc::NotRunning)),
                Some(
                    Observation::new(vec![RECT], vec![], GlazewmIpc::NoAnswer)
                        .with_zebar_running(false),
                ),
            ],
        };

        assert_eq!(from_json(&to_json(&baseline)), Ok(baseline));
    }

    #[test]
    fn writes_one_entry_per_step_with_unsettled_steps_as_null() {
        let baseline = Baseline {
            sut_commit: "5709ad0a".into(),
            steps: vec![
                None,
                Some(Observation::new(vec![RECT], vec![], GlazewmIpc::NoAnswer)),
            ],
        };

        assert_eq!(
            to_json(&baseline),
            r#"{
  "sut_commit": "5709ad0a",
  "steps": [
    null,
    {
      "os_displays": [
        {
          "x": 0,
          "y": 0,
          "width": 1280,
          "height": 800
        }
      ],
      "test_windows": [],
      "glazewm": {
        "answer": "no_answer"
      }
    }
  ]
}
"#
        );
    }

    #[test]
    fn a_file_with_a_field_it_does_not_know_is_an_error() {
        let text = r#"{"sut_commit":"5709ad0a","steps":[],"extra":1}"#;

        assert!(from_json(text).is_err());
    }
}
