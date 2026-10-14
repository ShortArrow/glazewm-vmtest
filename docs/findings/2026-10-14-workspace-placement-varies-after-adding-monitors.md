# Finding: where glazewm puts a workspace after monitors are added varies between identical runs

- Date: 2026-10-14
- Status: open; cause not known
- SUT: glazewm `5709ad0a`, built as in docs/spikes/2026-10-09-first-glazewm-run.md, with its sample configuration less the startup and shutdown commands
- Guest: snapshot `daemon-task` of docs/GUEST-IMAGE.md

## Reproduction

The scenario `tiled_test_windows_stay_within_their_working_area_across_monitor_count_changes` in crates/adapters/tests/live_dockur.rs:

1. Start with the platform's display and one virtual monitor to its right, and start glazewm.
2. Open three test windows. They land on workspace 2, on the virtual monitor.
3. Set the virtual monitor count to 3.

## Observed

After step 3 the guest OS reports the same four displays in every run, and glazewm reports four monitors. Where workspace 2 ends up differs:

| Runs | Workspace 2, with the three windows | Other workspaces |
|---|---|---|
| 4 of 5 | On the platform's display, which hides workspace 1 | 3, 4 and 5 on the three virtual monitors |
| 1 of 5 | Still on the first virtual monitor | 1 on the platform's display; 3 and 4 on the other two virtual monitors |

No invariant was violated in any run; the difference shows only as a baseline deviation.

## Open

- Whether the virtual display driver presents `SETDISPLAYCOUNT` as one addition or as removing and re-adding every virtual monitor, and whether that sequence itself varies between runs.
- Whether glazewm's handling of the same sequence of display events varies, for example with the timing of `WM_DISPLAYCHANGE` against its own monitor polling.
- Whether this is the cause of the one unexplained failure of `glazewm_tracks_virtual_monitor_count_changes`, whose error text was not kept.
