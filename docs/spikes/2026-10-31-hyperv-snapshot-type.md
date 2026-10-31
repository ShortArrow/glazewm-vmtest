# Spike: which Hyper-V checkpoint type restores a running guest

- Date: 2026-10-31
- Spike: "Snapshot type" in docs/ROADMAP.md
- Host and guest: as in docs/spikes/2026-10-31-hyperv-wmi-control.md; the time synchronization integration service on

## Method

For each checkpoint type, standard and production (`-CheckpointType ProductionOnly`):

1. Restore `daemon-task`, start the guest, wait for `explorer.exe` in session 1, copy the guest daemon in and start the task `vmtest-guest-daemon`.
2. Take a checkpoint of the running guest.
3. Wait 90 s, stop the guest daemon, and power the guest off.
4. Restore the checkpoint, start the guest, and look through SSH at the guest daemon, `explorer.exe` in session 1, the active monitors (`WmiMonitorBasicDisplayParams`), the boot time, and the guest clock.

The guest clock was compared with the clock of the machine running SSH, taking the midpoint of each SSH call; the duration of the calls was not measured. Before each checkpoint the difference was +5.5 s, an offset between the two machines that the restore did not cause.

## Observations

| | Standard | Production |
|---|---|---|
| State after the restore | Saved | Off |
| From start to SSH answering | 3.9 s | 12.1 s |
| Guest daemon | Running in session 1, listening on `127.0.0.1:7041`, though it had been stopped before the power-off | Not running |
| `explorer.exe` in session 1 | Running | Running (automatic logon) |
| Boot time | Unchanged: the guest resumed | New: the guest booted |
| Active monitors | 2, as before | 2, as before |
| Clock difference from 3.9 s / 12.1 s to 60 s after start | +3.2 to +4.1 s | +5.6 to +5.9 s |

The standard checkpoint held the guest as it was 90 s before the restore, yet by the time SSH answered the clock difference was within 2.3 s of its value before the checkpoint. No 90 s lag was seen.

## Consequences for the design

- Only a standard checkpoint of a running guest restores to a logged-on session with the guest daemon running. A production checkpoint restores to a powered-off guest, which boots, logs on automatically, and has no guest daemon until the task is started again.
- Restoring a running standard checkpoint saves the boot: SSH answered after 3.9 s instead of 11–12 s. But the guest daemon in such a checkpoint is the executable that was running when it was taken. A checkpoint that holds a running guest daemon has to be retaken whenever the guest daemon changes, while a powered-off checkpoint takes the guest daemon as it is copied in on each run.
- By the time SSH answered, the guest clock no longer showed the 90 s it had missed; whether time synchronization is what corrects it was not tested.

## Not measured

- The clock to better than a few seconds, and with time synchronization off.
- Display state other than the number of active monitors: their arrangement, resolution, and whether a restore raises display-change messages in the guest.
- Restoring a running checkpoint while an RDP session owns the console.
