//! Runs a scenario against the dockur/windows guest in guests/dockur-windows.
//!
//! Needs `cargo build --release -p vmtest-guest-daemon`, glazewm built into target/sut/glazewm/release,
//! and the guest image of docs/GUEST-IMAGE.md.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vmtest_adapters::dockur::{DockurConfig, DockurGuest, SystemClock};
use vmtest_adapters::glazewm::config_without_startup_commands;
use vmtest_domain::scenario::{DEFAULT_IPC_LIMIT, Scenario, Step, StepVerdict};
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
        glazewm_build: repository.join("target/sut/glazewm/release"),
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

    let verdicts = run(
        &scenario,
        &mut DockurGuest::new(guest),
        &mut SystemClock::start(),
        SETTLE,
    )
    .unwrap();

    assert_eq!(verdicts, vec![StepVerdict::Passed; 5]);
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

    let verdicts = run(
        &scenario,
        &mut DockurGuest::new(guest),
        &mut SystemClock::start(),
        SETTLE,
    )
    .unwrap();

    assert_eq!(verdicts, vec![StepVerdict::Passed]);
}
