# Finding: Windows minimizes a tiled window when the virtual monitors return, and glazewm keeps it minimized

- Date: 2026-10-31
- Status: fixed in ../glazewm on `fix/restore-windows-minimized-by-display-change` (`d43f1d1c`) and merged into `vmtest/all-fixes` (`2f4ff441`); not reported upstream
- SUT: glazewm `5709ad0a`, with its sample configuration less the startup and shutdown commands
- Guest: checkpoint `daemon-task` of docs/GUEST-IMAGE.md on Hyper-V, whose displays are the Hyper-V synthetic display (1280×800) and the Virtual Display Driver; Windows 11 build 26300

## Reproduction

The scenario `tiled_test_windows_stay_within_their_working_area_across_monitor_count_changes` in crates/adapters/tests/live.rs:

1. Start with the synthetic display and one virtual monitor to its right, and start glazewm.
2. Open three test windows. They tile on workspace 2, on the virtual monitor.
3. Set the virtual monitor count to 3: every virtual monitor leaves, glazewm settles with workspace 2 and its windows on the synthetic display, then three virtual monitors arrive.

## Observed

In 2 of 7 runs, after step 3 test window 0 was minimized: the OS placed it at (-31993, -32000), and glazewm reported it with the state `minimized` while the other two windows tiled across the whole workspace. It stayed minimized through the remaining steps of the scenario in the one run whose later steps were compared. No invariant was violated; the difference showed only as a baseline deviation.

glazewm's log of one such run shows no minimize issued by glazewm. When the virtual monitors left, glazewm moved workspace 2 to the synthetic display and the window stayed tiled. When the driver's device was enabled again, 0.6 seconds before glazewm handled the change that added the three monitors, glazewm logged `Window minimized` for test window 0 and `Updating window state: Minimized`.

## Trigger

Windows 11 minimizes the windows of a display that is disconnected, unless `MonitorRemovalRecalcBehavior` (DWORD, `HKCU\Control Panel\Desktop`) is 1. The value is undocumented by Microsoft; third-party guides such as winaero.com, "Windows 11: Don't minimize windows when monitor is disconnected", read on 2026-10-31, give its name and meaning. The guest image does not set it, so the guest minimizes.

A copy of the guest with the value set to 1 ran the same scenario 15 times: every run matched the baseline and glazewm logged no minimize. With 2 of 7 runs minimizing, 15 runs without one would happen by chance less than 1% of the time.

## Fix

`fix/restore-windows-minimized-by-display-change` takes a minimize within 5 seconds of a display change, in either order, as the OS's and restores the window to its previous state. A minimize the user makes within those 5 seconds is undone as well.

The first version looked only at minimizes after a display change, and 4 of 15 runs still kept a window minimized: in those runs glazewm learned of the minimize 0.07 to 0.19 seconds before the display change. With both orders, 15 runs on the guest with Windows' default minimized a window in 2 runs, glazewm logged `Window minimized by a display change, restoring` for each, and no window stayed minimized.

## Open when found

- glazewm treats every minimize the OS reports as the user's, and so keeps a window that Windows minimized for a display change out of its tiling. Whether glazewm should tell the two apart, for example by restoring a window the OS minimizes while displays are changing, is not decided.
- Why Windows minimized the window only when the virtual monitors returned, after glazewm had already moved it to the remaining display, and only in some runs.
