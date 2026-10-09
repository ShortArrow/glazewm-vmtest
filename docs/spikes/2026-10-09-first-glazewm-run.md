# Spike: glazewm under monitor-count changes on dockur/windows

- Date: 2026-10-09
- Guest: snapshot `vdd` (docs/spikes/2026-10-07-iddcx-virtual-display-driver.md), with the Microsoft Visual C++ 2015–2022 x64 redistributable added; saved as snapshot `vcredist`
- SUT: glazewm `5709ad0a` (upstream `main`), built on the host with `cargo +nightly build --release --locked -p wm -p wm-cli -p wm-watcher` (nightly 2026-02-17), no `ui_access` feature
- Configuration: glazewm's own `resources/assets/sample-config.yaml` with `startup_commands` and `shutdown_commands` emptied, passed through `GLAZEWM_CONFIG_PATH`

## Observations

| Question | Observed |
|---|---|
| Does the release build start on a fresh guest? | No: exit code `0xC0000135` (DLL not found). The binaries import `vcruntime140.dll`; glazewm's release workflow builds the same way, so the official binaries need it too. Installing the redistributable fixed it |
| Can glazewm run in session 1 at the user's integrity level? | Yes: started through an interactive scheduled task with `RunLevel Limited`; `glazewm` and `glazewm-watcher` ran in session 1 |
| Can the harness query glazewm from SSH? | Yes: `glazewm-cli query monitors` from session 0 answers, since the IPC server listens on TCP |

Monitor-count scenario, each step observed 6 s after the operation:

| Step | glazewm monitors (workspaces) | OS monitors |
|---|---|---|
| Start | `DISPLAY1` (1), `DISPLAY2` (2) | 2 |
| Virtual count 3 | `DISPLAY1` (1), `DISPLAY3` (2), `DISPLAY4` (3), `DISPLAY5` (4) | 4 |
| Virtual count 1 | `DISPLAY1` (1), `DISPLAY6` (2) | 2 |
| Device disabled | `DISPLAY1` (1) | 1 |
| Device enabled | `DISPLAY1` (1), `DISPLAY7` (2) | 2 |

glazewm's monitor count matched the OS after every step. Empty workspaces on removed monitors disappeared; no windows were open.

## Not measured

- Windows on a monitor that is removed.
- Whether 6 s is a settled state; no settle detection was used.
- The official build's `ui_access` feature, which needs signing and an install under a secure location to take effect.
