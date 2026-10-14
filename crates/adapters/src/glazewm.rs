//! Reading what `glazewm-cli` prints, and preparing glazewm's configuration.
//!
//! The JSON is read without glazewm's own types, so that SUT commits with a different IPC schema can still be observed.

use serde_json::Value;
use vmtest_domain::scenario::{
    GlazewmIpc, ManagedWindow, MonitorLayout, WindowPresence, WorkspaceLayout,
};
use vmtest_domain::topology::Rect;
use vmtest_protocol::Response;

/// What the harness knows of a window glazewm reports, found by its handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowIdentity {
    pub test_window: Option<u32>,
    pub presence: WindowPresence,
}

/// Reads the guest daemon's answer to a `query monitors` as what glazewm answered.
///
/// `identify` tells, for each window handle glazewm reports, which test window it is and what the OS says
/// of it. A daemon-side error says nothing about glazewm, so it is an error of the harness, not `NoAnswer`.
pub fn monitors_answer(
    response: Response,
    identify: &dyn Fn(i64) -> WindowIdentity,
) -> Result<GlazewmIpc, String> {
    match response {
        Response::Output { stdout, .. } => {
            monitor_layouts(&stdout, identify).map(GlazewmIpc::Answered)
        }
        Response::TimedOut { .. } => Ok(GlazewmIpc::NoAnswer),
        other => Err(format!("unexpected answer to a glazewm query: {other:?}")),
    }
}

/// The handle of every window in the output of `glazewm-cli query monitors`, or none if it cannot be read.
pub fn window_handles(stdout: &str) -> Vec<i64> {
    fn collect(container: &Value, handles: &mut Vec<i64>) {
        if container["type"] == "window" {
            handles.extend(container["handle"].as_i64());
        }
        for child in container["children"].as_array().into_iter().flatten() {
            collect(child, handles);
        }
    }
    let mut handles = Vec::new();
    if let Ok(answer) = serde_json::from_str::<Value>(stdout.trim()) {
        for monitor in answer["data"]["monitors"].as_array().into_iter().flatten() {
            collect(monitor, &mut handles);
        }
    }
    handles
}

/// Reads the monitors, the workspaces on each and the windows on those, from the output of
/// `glazewm-cli query monitors`.
///
/// Identifiers that change between runs (container ids, handles, device names) are not kept; a window's
/// handle only serves `identify`.
pub fn monitor_layouts(
    stdout: &str,
    identify: &dyn Fn(i64) -> WindowIdentity,
) -> Result<Vec<MonitorLayout>, String> {
    let answer: Value = serde_json::from_str(stdout.trim())
        .map_err(|error| format!("glazewm-cli printed something other than JSON: {error}"))?;
    if answer["success"] != Value::Bool(true) {
        let error = answer["error"].as_str().unwrap_or("no error message");
        return Err(format!("glazewm answered with an error: {error}"));
    }
    answer["data"]["monitors"]
        .as_array()
        .ok_or("glazewm's answer has no data.monitors array")?
        .iter()
        .map(|monitor| monitor_layout(monitor, identify))
        .collect()
}

fn monitor_layout(
    monitor: &Value,
    identify: &dyn Fn(i64) -> WindowIdentity,
) -> Result<MonitorLayout, String> {
    let working = &monitor["workingRect"];
    let left = int(working, "left")?;
    let top = int(working, "top")?;
    Ok(MonitorLayout {
        bounds: Rect {
            x: int(monitor, "x")?,
            y: int(monitor, "y")?,
            width: int(monitor, "width")?.unsigned_abs(),
            height: int(monitor, "height")?.unsigned_abs(),
        },
        working_area: Rect {
            x: left,
            y: top,
            width: (int(working, "right")? - left).unsigned_abs(),
            height: (int(working, "bottom")? - top).unsigned_abs(),
        },
        dpi: int(monitor, "dpi")?.unsigned_abs(),
        has_focus: boolean(monitor, "hasFocus")?,
        workspaces: monitor["children"]
            .as_array()
            .ok_or("a monitor in glazewm's answer has no children array")?
            .iter()
            .filter(|child| child["type"] == "workspace")
            .map(|workspace| workspace_layout(workspace, identify))
            .collect::<Result<_, _>>()?,
    })
}

fn workspace_layout(
    workspace: &Value,
    identify: &dyn Fn(i64) -> WindowIdentity,
) -> Result<WorkspaceLayout, String> {
    let mut windows = Vec::new();
    collect_windows(workspace, identify, &mut windows)?;
    Ok(WorkspaceLayout {
        name: text(workspace, "name")?,
        is_displayed: boolean(workspace, "isDisplayed")?,
        has_focus: boolean(workspace, "hasFocus")?,
        tiling_direction: text(workspace, "tilingDirection")?,
        windows,
    })
}

/// Appends the windows among `container`'s descendants in depth-first order, the order glazewm lays them out.
fn collect_windows(
    container: &Value,
    identify: &dyn Fn(i64) -> WindowIdentity,
    windows: &mut Vec<ManagedWindow>,
) -> Result<(), String> {
    let children = container["children"]
        .as_array()
        .ok_or("a container in glazewm's answer has no children array")?;
    for child in children {
        match child["type"].as_str() {
            Some("window") => {
                let handle = child["handle"]
                    .as_i64()
                    .ok_or("a window in glazewm's answer has no integer handle")?;
                let identity = identify(handle);
                windows.push(ManagedWindow {
                    test_window: identity.test_window,
                    state: text(&child["state"], "type")?,
                    presence: identity.presence,
                });
            }
            Some("split") => collect_windows(child, identify, windows)?,
            _ => {}
        }
    }
    Ok(())
}

fn int(object: &Value, field: &str) -> Result<i32, String> {
    object[field]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| format!("glazewm's answer has no integer {field}"))
}

fn boolean(object: &Value, field: &str) -> Result<bool, String> {
    object[field]
        .as_bool()
        .ok_or_else(|| format!("glazewm's answer has no boolean {field}"))
}

fn text(object: &Value, field: &str) -> Result<String, String> {
    object[field]
        .as_str()
        .map(String::from)
        .ok_or_else(|| format!("glazewm's answer has no string {field}"))
}

/// Returns glazewm's sample configuration with its startup and shutdown commands removed.
///
/// The sample starts and stops zebar, which a scenario that does not deploy zebar must not depend on.
pub fn config_without_startup_commands(sample: &str) -> String {
    sample
        .split_inclusive('\n')
        .map(|line| {
            let key = line.trim_start();
            if key.starts_with("startup_commands:") || key.starts_with("shutdown_commands:") {
                let indent = &line[..line.len() - key.len()];
                let name = &key[..key.find(':').unwrap_or(key.len())];
                let ending = if line.ends_with('\n') { "\n" } else { "" };
                format!("{indent}{name}: []{ending}")
            } else {
                line.to_string()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmtest_domain::scenario::{WindowPresence, WorkspaceLayout};
    use vmtest_domain::topology::Rect;

    #[test]
    fn a_timed_out_monitors_query_is_no_answer() {
        assert_eq!(
            monitors_answer(Response::TimedOut { limit_ms: 2000 }, &unknown),
            Ok(GlazewmIpc::NoAnswer)
        );
    }

    #[test]
    fn a_daemon_error_is_not_an_answer_from_glazewm() {
        let response = Response::Error {
            message: "glazewm-cli.exe not found".into(),
        };

        assert!(monitors_answer(response, &unknown).is_err());
    }

    #[test]
    fn empties_only_the_startup_and_shutdown_commands() {
        let sample = "general:
  startup_commands: ['shell-exec zebar']

  shutdown_commands: ['shell-exec taskkill /IM zebar.exe /F']

  config_reload_commands: []
";

        assert_eq!(
            config_without_startup_commands(sample),
            "general:
  startup_commands: []

  shutdown_commands: []

  config_reload_commands: []
"
        );
    }

    const ONE_MONITOR: &str = r#"{"clientMessage":"query monitors","data":{"monitors":[
        {"type":"monitor","id":"7f0c","x":1280,"y":0,"width":800,"height":600,"dpi":144,"hasFocus":true,
         "workingRect":{"left":1280,"top":0,"right":2080,"bottom":560},
         "children":[
           {"type":"workspace","id":"a1","name":"1","isDisplayed":true,"hasFocus":true,"tilingDirection":"horizontal","children":[]},
           {"type":"workspace","id":"b2","name":"2","isDisplayed":false,"hasFocus":false,"tilingDirection":"vertical","children":[]}
         ]}
    ]},"error":null,"success":true}"#;

    fn unknown(_handle: i64) -> WindowIdentity {
        WindowIdentity {
            test_window: None,
            presence: WindowPresence::TopLevel,
        }
    }

    fn one_monitor() -> MonitorLayout {
        MonitorLayout {
            bounds: Rect {
                x: 1280,
                y: 0,
                width: 800,
                height: 600,
            },
            working_area: Rect {
                x: 1280,
                y: 0,
                width: 800,
                height: 560,
            },
            dpi: 144,
            has_focus: true,
            workspaces: vec![
                WorkspaceLayout {
                    name: "1".into(),
                    is_displayed: true,
                    has_focus: true,
                    tiling_direction: "horizontal".into(),
                    windows: vec![],
                },
                WorkspaceLayout {
                    name: "2".into(),
                    is_displayed: false,
                    has_focus: false,
                    tiling_direction: "vertical".into(),
                    windows: vec![],
                },
            ],
        }
    }

    #[test]
    fn reads_the_windows_of_a_workspace_in_glazewm_order_through_split_containers() {
        let stdout = ONE_MONITOR.replace(
            r#""tilingDirection":"horizontal","children":[]"#,
            r#""tilingDirection":"horizontal","children":[
                {"type":"window","handle":200,"state":{"type":"tiling"},"x":0,"y":0,"width":1,"height":1},
                {"type":"split","tilingDirection":"vertical","children":[
                    {"type":"window","handle":300,"state":{"type":"floating","centered":true},"x":0,"y":0,"width":1,"height":1}
                ]}
            ]"#,
        );

        let identify = |handle| match handle {
            300 => WindowIdentity {
                test_window: Some(4),
                presence: WindowPresence::Gone,
            },
            _ => unknown(handle),
        };

        let monitors = monitor_layouts(&stdout, &identify).unwrap();

        assert_eq!(window_handles(&stdout), [200, 300]);
        assert_eq!(
            monitors[0].workspaces[0].windows,
            [
                ManagedWindow {
                    test_window: None,
                    state: "tiling".into(),
                    presence: WindowPresence::TopLevel,
                },
                ManagedWindow {
                    test_window: Some(4),
                    state: "floating".into(),
                    presence: WindowPresence::Gone,
                }
            ]
        );
    }

    #[test]
    fn finds_no_window_handles_in_output_it_cannot_read() {
        assert!(window_handles("not json").is_empty());
    }

    #[test]
    fn reads_each_monitor_with_its_workspaces_in_glazewm_order() {
        assert_eq!(
            monitor_layouts(ONE_MONITOR, &unknown),
            Ok(vec![one_monitor()])
        );
    }

    #[test]
    fn a_finished_monitors_query_is_glazewm_answer() {
        let response = Response::Output {
            exit_code: Some(0),
            stdout: ONE_MONITOR.into(),
            stderr: String::new(),
        };

        assert_eq!(
            monitors_answer(response, &unknown),
            Ok(GlazewmIpc::Answered(vec![one_monitor()]))
        );
    }

    #[test]
    fn ignores_fields_it_does_not_know() {
        let stdout = ONE_MONITOR.replace(r#""dpi":144"#, r#""dpi":144,"newField":[1]"#);

        assert_eq!(monitor_layouts(&stdout, &unknown), Ok(vec![one_monitor()]));
    }

    #[test]
    fn reports_the_error_of_an_unsuccessful_answer() {
        let stdout = r#"{"clientMessage":"query monitors","data":null,"error":"WM is not running","success":false}"#;

        assert_eq!(
            monitor_layouts(stdout, &unknown),
            Err("glazewm answered with an error: WM is not running".into())
        );
    }

    #[test]
    fn a_monitor_without_its_bounds_is_an_error() {
        let stdout = ONE_MONITOR.replace(r#""x":1280,"#, "");

        assert!(monitor_layouts(&stdout, &unknown).is_err());
    }
}
