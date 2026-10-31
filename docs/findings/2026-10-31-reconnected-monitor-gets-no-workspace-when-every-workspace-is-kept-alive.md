# Finding: a reconnected monitor gets no workspace when every configured workspace is kept alive

- Date: 2026-10-31
- Status: fixed in ../glazewm on `fix/no-displayed-workspace-on-monitor-reconnect` (`207f9b5a`) and merged into `vmtest/all-fixes` (`f8dc1eba`); not reported upstream
- SUT: glazewm `5709ad0a`, and `vmtest/all-fixes` at `b801904d`
- Guest: checkpoint `daemon-task` of docs/GUEST-IMAGE.md on Hyper-V, whose displays are the Hyper-V synthetic display (1024×768) and the Virtual Display Driver

## Reproduction

The scenario `every_monitor_gets_a_workspace_back_when_every_workspace_is_kept_alive` in crates/adapters/tests/live.rs:

1. Start glazewm with ten workspaces, `0` to `9`, all with `keep_alive: true`; `0` to `5` and `9` have `bind_to_monitor: 1`. The guest has the synthetic display and three virtual displays.
2. Disable the virtual displays, then enable them again.

## Observed

| SUT | After the displays return |
|---|---|
| `5709ad0a` | A "Non-fatal error" dialog reads "No workspace config available to activate workspace." glazewm answers no IPC request while it is open |
| `vmtest/all-fixes` at `b801904d`, where that dialog does not stop the event loop | glazewm answers, but two of the three virtual displays (800×600 at x = 1824 and x = 2624) have no workspace |
| `vmtest/all-fixes` at `f8dc1eba` | Every display has one displayed workspace: `6` on the synthetic display, `0` to `5` and `9` on the first virtual display, `7` and `8` on the other two |

## Mechanism, as far as the source shows

When a display is removed, its workspaces move to the remaining one, so all ten stay active. When a display is added, `move_bounded_workspaces_to_new_monitor` in packages/wm/src/commands/monitor/add_monitor.rs moves the workspaces bound to its index back, and gives a display left with none a workspace through `activate_workspace(None, …)`. That creates a workspace from an inactive workspace configuration, and there is none, so it fails with "No workspace config available to activate workspace." and the display stays without a workspace. A command that later looks up the displayed workspace of that monitor fails with "No displayed workspace.".

The fix moves a workspace that another monitor is not displaying, and that is not bound to that monitor, onto the new display when no inactive workspace configuration is left.

## Effect on a user

A user who keeps every workspace alive and has displays disconnected and reconnected gets a monitor that glazewm tracks but shows no workspace on, and commands that look up its displayed workspace fail with "No displayed workspace.". On upstream glazewm the error dialog also stops the window manager until it is dismissed.

## Open

- Whether upstream would rather create a workspace than move one.
- Which workspace a user expects on the returned display; the fix takes the first it finds, going through the monitors in order and through the workspaces of each.
