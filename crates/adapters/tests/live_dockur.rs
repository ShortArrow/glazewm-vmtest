//! Runs a scenario against the dockur/windows guest in guests/dockur-windows.
//!
//! Needs `cargo build --release -p vmtest-guest-daemon -p vmtest-test-window-win32 -p vmtest-load`, glazewm built into target/sut/glazewm/release
//! (or the directory named by `VMTEST_GLAZEWM_BUILD`), and the guest image of docs/GUEST-IMAGE.md.
//!
//! A scenario with a baseline compares its observations with baselines/<name>.json. With
//! `VMTEST_RECORD=1` it records that file instead, from the glazewm commit named by `VMTEST_SUT_COMMIT`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vmtest_adapters::baseline_file;
use vmtest_adapters::dockur::{DockurConfig, DockurGuest, SystemClock};
use vmtest_adapters::glazewm::config_without_startup_commands;
use vmtest_domain::baseline::{Baseline, Deviation};
use vmtest_domain::scenario::{
    DEFAULT_IPC_LIMIT, Load, LoadKind, Observation, Scenario, Step, StepVerdict, TestWindowBatch,
    WindowLifetime,
};
use vmtest_runner::{SettleSettings, run};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// glazewm's sample configuration without its startup and shutdown commands.
fn quiet_glazewm_config() -> String {
    let sample = std::fs::read_to_string(
        repository().join("../glazewm/resources/assets/sample-config.yaml"),
    )
    .expect("glazewm is checked out beside this repository");
    config_without_startup_commands(&sample)
}

/// A guest whose glazewm starts with `glazewm_config`, written under `name` in target/vmtest.
fn config(name: &str, glazewm_config: &str) -> DockurConfig {
    let repository = repository();
    let guest = repository.join("guests/dockur-windows");
    let glazewm_config_path = repository.join("target/vmtest").join(name);
    std::fs::create_dir_all(glazewm_config_path.parent().unwrap()).unwrap();
    std::fs::write(&glazewm_config_path, glazewm_config).unwrap();
    let glazewm_config = glazewm_config_path;
    DockurConfig {
        compose_dir: guest.clone(),
        service: "windows".into(),
        volume: "glazewm-vmtest-win11-storage".into(),
        image: "dockurr/windows:6.06".into(),
        snapshot: "daemon-task".into(),
        ssh_key: guest.join(".secrets/id_ed25519"),
        ssh_port: 2222,
        user: "fixture".into(),
        daemon_address: "127.0.0.1:7041".into(),
        daemon_executable: repository.join("target/release/vmtest-guest-daemon.exe"),
        test_window_executable: repository.join("target/release/vmtest-test-window-win32.exe"),
        load_executable: repository.join("target/release/vmtest-load.exe"),
        daemon_limits: None,
        glazewm_build: std::env::var_os("VMTEST_GLAZEWM_BUILD")
            .map(PathBuf::from)
            .unwrap_or_else(|| repository.join("target/sut/glazewm/release")),
        glazewm_config,
        start_glazewm_script: guest.join("scripts/start-glazewm.ps1"),
    }
}

const SETTLE: SettleSettings = SettleSettings {
    window: Duration::from_secs(5),
    limit: Duration::from_secs(60),
    poll: Duration::from_secs(1),
};

#[test]
#[ignore = "needs the dockur/windows guest and prebuilt binaries"]
fn glazewm_tracks_virtual_monitor_count_changes() {
    let scenario = Scenario {
        name: "virtual monitor count changes".into(),
        virtual_monitors: 1,
        steps: vec![
            Step::SetVirtualMonitorCount(3),
            Step::SetVirtualMonitorCount(1),
            Step::RemoveVirtualMonitors,
            Step::RestoreVirtualMonitors,
        ],
        ipc_limit: DEFAULT_IPC_LIMIT,
    };
    let guest = config("glazewm-config.yaml", &quiet_glazewm_config());

    let outcomes = run(
        &scenario,
        &mut DockurGuest::new(guest),
        &mut SystemClock::start(),
        SETTLE,
    )
    .unwrap();

    let observations: Vec<_> = outcomes.iter().map(|o| o.observation.clone()).collect();
    let verdicts: Vec<_> = outcomes.into_iter().map(|o| o.verdict).collect();
    let deviations = compare_or_record("virtual-monitor-count-changes", &observations);
    assert_eq!(
        (verdicts, deviations),
        (vec![StepVerdict::Passed; 5], vec![])
    );
}

/// Compares `observations` with baselines/`name`.json, or records that file when `VMTEST_RECORD=1`.
#[test]
#[ignore = "needs the dockur/windows guest and prebuilt binaries"]
fn tiled_test_windows_stay_within_their_working_area_across_monitor_count_changes() {
    let scenario = Scenario {
        name: "test windows across monitor count changes".into(),
        virtual_monitors: 1,
        steps: vec![
            Step::OpenTestWindows(TestWindowBatch::persistent(3)),
            Step::SetVirtualMonitorCount(3),
            Step::SetVirtualMonitorCount(1),
            Step::RemoveVirtualMonitors,
            Step::RestoreVirtualMonitors,
        ],
        ipc_limit: DEFAULT_IPC_LIMIT,
    };
    let guest = config("glazewm-config.yaml", &quiet_glazewm_config());

    let outcomes = run(
        &scenario,
        &mut DockurGuest::new(guest),
        &mut SystemClock::start(),
        SETTLE,
    )
    .unwrap();

    let observations: Vec<_> = outcomes.iter().map(|o| o.observation.clone()).collect();
    let verdicts: Vec<_> = outcomes.into_iter().map(|o| o.verdict).collect();
    let deviations = compare_or_record("test-windows-across-monitor-count-changes", &observations);
    assert_eq!(
        (verdicts, deviations),
        (vec![StepVerdict::Passed; 6], vec![])
    );
}

#[test]
#[ignore = "needs the dockur/windows guest and prebuilt binaries"]
fn a_hung_test_window_does_not_stop_glazewm_managing_the_others() {
    let scenario = Scenario {
        name: "hung test window across losing every display".into(),
        virtual_monitors: 1,
        steps: vec![
            Step::OpenTestWindows(TestWindowBatch::persistent(3)),
            Step::HangTestWindow(1),
            Step::RemoveVirtualMonitors,
            Step::RestoreVirtualMonitors,
            Step::ResumeTestWindow(1),
        ],
        ipc_limit: DEFAULT_IPC_LIMIT,
    };
    let guest = config("glazewm-config.yaml", &quiet_glazewm_config());

    let outcomes = run(
        &scenario,
        &mut DockurGuest::new(guest),
        &mut SystemClock::start(),
        SETTLE,
    )
    .unwrap();

    let observations: Vec<_> = outcomes.iter().map(|o| o.observation.clone()).collect();
    let verdicts: Vec<_> = outcomes.into_iter().map(|o| o.verdict).collect();
    let deviations = compare_or_record(
        "hung-test-window-across-losing-every-display",
        &observations,
    );
    assert_eq!(
        (verdicts, deviations),
        (vec![StepVerdict::Passed; 6], vec![])
    );
}

/// Windows opened in a burst land in an order that depends on timing, so this scenario has no baseline
/// and is judged by its invariants alone.
#[test]
#[ignore = "needs the dockur/windows guest and prebuilt binaries"]
fn glazewm_keeps_no_short_lived_window_after_a_burst() {
    let burst = |count, within_ms, lifetime| {
        Step::OpenTestWindows(TestWindowBatch {
            count,
            within: Some(Duration::from_millis(within_ms)),
            lifetime,
        })
    };
    let scenario = Scenario {
        name: "bursts of short-lived and persistent test windows".into(),
        virtual_monitors: 1,
        steps: vec![
            Step::OpenTestWindows(TestWindowBatch::persistent(2)),
            burst(20, 500, WindowLifetime::DestroyedAfter(Duration::ZERO)),
            burst(
                10,
                200,
                WindowLifetime::DestroyedAfter(Duration::from_millis(30)),
            ),
            burst(5, 100, WindowLifetime::Persistent),
            Step::RemoveVirtualMonitors,
            Step::RestoreVirtualMonitors,
        ],
        ipc_limit: DEFAULT_IPC_LIMIT,
    };
    let guest = config("glazewm-config.yaml", &quiet_glazewm_config());

    let outcomes = run(
        &scenario,
        &mut DockurGuest::new(guest),
        &mut SystemClock::start(),
        SETTLE,
    )
    .unwrap();

    let verdicts: Vec<_> = outcomes.into_iter().map(|o| o.verdict).collect();
    assert_eq!(verdicts, vec![StepVerdict::Passed; 7]);
}

/// Under load glazewm may miss an IPC answer that the scenario does not hold it to, so this scenario has
/// no baseline and is judged by its invariants alone.
#[test]
#[ignore = "needs the dockur/windows guest and prebuilt binaries"]
fn glazewm_tracks_losing_every_display_under_cpu_and_memory_load() {
    let cpu = Load {
        kind: LoadKind::Cpu,
        percent: 100,
    };
    let memory = Load {
        kind: LoadKind::Memory,
        percent: 70,
    };
    let scenario = Scenario {
        name: "losing every display under load".into(),
        virtual_monitors: 1,
        steps: vec![
            Step::OpenTestWindows(TestWindowBatch::persistent(3)),
            Step::StartLoad(cpu),
            Step::RemoveVirtualMonitors,
            Step::RestoreVirtualMonitors,
            Step::StopLoad(LoadKind::Cpu),
            Step::StartLoad(memory),
            Step::RemoveVirtualMonitors,
            Step::RestoreVirtualMonitors,
            Step::StopLoad(LoadKind::Memory),
        ],
        ipc_limit: DEFAULT_IPC_LIMIT,
    };
    let guest = config("glazewm-config.yaml", &quiet_glazewm_config());

    let outcomes = run(
        &scenario,
        &mut DockurGuest::new(guest),
        &mut SystemClock::start(),
        SETTLE,
    )
    .unwrap();

    let verdicts: Vec<_> = outcomes.into_iter().map(|o| o.verdict).collect();
    assert_eq!(verdicts, vec![StepVerdict::Passed; 10]);
}

fn compare_or_record(name: &str, observations: &[Option<Observation>]) -> Vec<Deviation> {
    let path = repository().join("baselines").join(format!("{name}.json"));
    if std::env::var("VMTEST_RECORD").as_deref() == Ok("1") {
        let sut_commit = std::env::var("VMTEST_SUT_COMMIT")
            .expect("VMTEST_SUT_COMMIT names the glazewm commit a baseline is recorded from");
        let baseline = Baseline {
            sut_commit,
            steps: observations.to_vec(),
        };
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, baseline_file::to_json(&baseline)).unwrap();
        return vec![];
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    baseline_file::from_json(&text)
        .unwrap()
        .deviations(observations)
        .unwrap_or_else(|stale| panic!("{} is stale: {stale:?}", path.display()))
}

/// A startup command naming a program that is not installed, such as a status bar the user never installed.
#[test]
#[ignore = "needs the dockur/windows guest and prebuilt binaries"]
fn glazewm_answers_ipc_after_a_startup_command_fails() {
    let scenario = Scenario {
        name: "startup command names a missing program".into(),
        virtual_monitors: 1,
        steps: vec![],
        ipc_limit: DEFAULT_IPC_LIMIT,
    };
    let glazewm_config = quiet_glazewm_config().replacen(
        "startup_commands: []",
        "startup_commands: ['shell-exec vmtest-missing-program']",
        1,
    );
    let guest = config(
        "glazewm-config-missing-startup-program.yaml",
        &glazewm_config,
    );

    let outcomes = run(
        &scenario,
        &mut DockurGuest::new(guest),
        &mut SystemClock::start(),
        SETTLE,
    )
    .unwrap();

    let verdicts: Vec<_> = outcomes.into_iter().map(|o| o.verdict).collect();
    assert_eq!(verdicts, vec![StepVerdict::Passed]);
}

#[test]
#[ignore = "needs the dockur/windows guest and prebuilt binaries"]
fn reports_a_glazewm_that_exits_as_not_running() {
    let scenario = Scenario {
        name: "glazewm exits when a third monitor appears".into(),
        virtual_monitors: 1,
        steps: vec![Step::SetVirtualMonitorCount(2)],
        ipc_limit: DEFAULT_IPC_LIMIT,
    };
    let kill_glazewm_at_three_monitors = encoded_powershell(
        "Add-Type -Name M -Namespace W -MemberDefinition '[DllImport(\"user32.dll\")] public static extern int GetSystemMetrics(int i);';          while ([W.M]::GetSystemMetrics(80) -lt 3) { Start-Sleep -Milliseconds 200 };          Stop-Process -Name glazewm -Force",
    );
    let glazewm_config = quiet_glazewm_config().replacen(
        "startup_commands: []",
        &format!(
            "startup_commands: ['shell-exec --hide-window powershell -NoProfile -EncodedCommand {kill_glazewm_at_three_monitors}']"
        ),
        1,
    );
    let guest = config(
        "glazewm-config-exits-at-three-monitors.yaml",
        &glazewm_config,
    );

    let outcomes = run(
        &scenario,
        &mut DockurGuest::new(guest),
        &mut SystemClock::start(),
        SETTLE,
    )
    .unwrap();

    let verdicts: Vec<_> = outcomes.into_iter().map(|o| o.verdict).collect();
    assert_eq!(verdicts[0], StepVerdict::Passed);
    assert!(
        matches!(&verdicts[1], StepVerdict::Violated(v) if v.len() == 1 && v[0].invariant == "glazewm_is_running"),
        "{verdicts:?}"
    );
}

/// Encodes `script` for `powershell -EncodedCommand`: Base64 of its UTF-16LE bytes.
///
/// glazewm's `shell-exec` splits its command on whitespace and drops quotes, which an encoded script has none of.
fn encoded_powershell(script: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let group = chunk.iter().enumerate().fold(0u32, |group, (i, &byte)| {
            group | u32::from(byte) << (16 - 8 * i)
        });
        for i in 0..4 {
            if i <= chunk.len() {
                encoded.push(ALPHABET[(group >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

#[test]
fn encodes_a_powershell_script_as_base64_of_utf16le() {
    assert_eq!(encoded_powershell("dir"), "ZABpAHIA");
    assert_eq!(encoded_powershell("ab"), "YQBiAA==");
}
