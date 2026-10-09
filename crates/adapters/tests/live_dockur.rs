//! Runs a scenario against the dockur/windows guest in guests/dockur-windows.
//!
//! Needs `cargo build --release -p vmtest-guest-daemon`, glazewm built into target/sut/glazewm/release,
//! and the guest image of docs/GUEST-IMAGE.md.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vmtest_adapters::dockur::{DockurConfig, DockurGuest, SystemClock};
use vmtest_adapters::glazewm::config_without_startup_commands;
use vmtest_domain::scenario::{Scenario, Step, StepVerdict};
use vmtest_runner::{SettleSettings, run};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn config() -> DockurConfig {
    let repository = repository();
    let guest = repository.join("guests/dockur-windows");
    let sample =
        std::fs::read_to_string(repository.join("../glazewm/resources/assets/sample-config.yaml"))
            .expect("glazewm is checked out beside this repository");
    let glazewm_config = repository.join("target/vmtest/glazewm-config.yaml");
    std::fs::create_dir_all(glazewm_config.parent().unwrap()).unwrap();
    std::fs::write(&glazewm_config, config_without_startup_commands(&sample)).unwrap();
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
    };
    let settle = SettleSettings {
        window: Duration::from_secs(5),
        limit: Duration::from_secs(60),
        poll: Duration::from_secs(1),
    };

    let verdicts = run(
        &scenario,
        &mut DockurGuest::new(config()),
        &mut SystemClock::start(),
        settle,
    )
    .unwrap();

    assert_eq!(verdicts, vec![StepVerdict::Passed; 5]);
}
