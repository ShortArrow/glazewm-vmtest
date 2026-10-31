# Finding: where glazewm puts a workspace after monitors are added varies between identical runs

- Date: 2026-10-14
- Status: cause found on 2026-10-31; the harness no longer varies the sequence glazewm sees
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
| 6 of 8 | On the platform's display, which hides workspace 1 | 3, 4 and 5 on the three virtual monitors |
| 2 of 8 | Still on the first virtual monitor | 1 on the platform's display; 3 and 4 on the other two virtual monitors |

No invariant was violated in any run; the difference shows only as a baseline deviation.

The scenario `glazewm_tracks_virtual_monitor_count_changes`, which opens no windows, varies at the same step in another way. In about ten runs, glazewm put the focus on the platform's display after setting the virtual monitor count to 3, except in one run, in which the first virtual monitor and its workspace had the focus. Workspaces were placed the same in every run.

## Cause

Found on 2026-10-31 on the Hyper-V guest, checkpoint `daemon-task`, with the same glazewm and scenario, by reading glazewm's own log of six runs:

- The virtual display driver changes its monitor count only by letting every virtual monitor go and starting again with the new count, so the guest OS shows the platform's display alone for a moment.
- glazewm reads the displays when it handles a display change. In the five runs that matched the baseline, it handled one while the platform's display was alone, logged `Removing monitor` for the first virtual monitor, moved workspace 2 to the platform's display, then added three monitors. In the one run that deviated, it handled the change only after the four displays were back, matched the first virtual monitor to its existing monitor, and added two.

glazewm handles the sequence it reads the same way each time; what varies is whether it reads the moment with no virtual monitor. Since 2026-10-31 the harness lets the SUT settle with every virtual monitor gone before it sets a new count, and in nine runs of this scenario and `glazewm_tracks_virtual_monitor_count_changes` workspace 2 was placed as in the baseline every time.

## Open when found

- Whether the virtual display driver presents `SETDISPLAYCOUNT` as one addition or as removing and re-adding every virtual monitor, and whether that sequence itself varies between runs.
- Whether glazewm's handling of the same sequence of display events varies, for example with the timing of `WM_DISPLAYCHANGE` against its own monitor polling.
- Whether this is the cause of the one unexplained failure of `glazewm_tracks_virtual_monitor_count_changes`, whose error text was not kept.
