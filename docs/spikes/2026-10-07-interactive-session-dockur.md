# Spike: interactive session on dockur/windows

- Date: 2026-10-07
- Spike: "Interactive session" in docs/ROADMAP.md
- Guest: `dockurr/windows:6.06`, `VERSION: "11"`, Windows 11 Pro build 26200, 4 cores, 8 GB, qcow2 (guests/dockur-windows/compose.yaml at commit d0d8ae0)
- Host: Windows 11 Pro, Docker Desktop 28.5.1 (WSL2), AMD Ryzen 9 7950X

## Observations

| Question | Method | Observed |
|---|---|---|
| Does the guest log on automatically? | `query user` and the `Winlogon` registry key over SSH, after install and after a cold boot from snapshot | `AutoAdminLogon=1`, `DefaultUserName=fixture`; session `console`, ID 1, Active, with `explorer` running in session 1 |
| Which session do SSH commands run in? | `(Get-Process -Id $PID).SessionId` over SSH | Session 0, High integrity, logon type NETWORK |
| Can a process be started in session 1 from SSH? | Scheduled task with `-LogonType Interactive`, started with `Start-ScheduledTask`; QEMU `screendump` | Notepad ran in session 1 and appeared on the console desktop |
| Can session 0 see that window? | `MainWindowTitle` and `MainWindowHandle` read over SSH | Empty title and handle 0 |
| Is UAC on? | `EnableLUA` registry value | `0` after the dockur install: tasks with `RunLevel Limited` and `Highest` both ran at High integrity |
| With UAC on, are the integrity levels distinct? | Set `EnableLUA=1`, reboot, run the same two tasks writing `whoami /groups` to a file | `Limited`: Medium, session 1. `Highest`: High, session 1, no prompt. Automatic logon still worked |
| Time from `docker compose up` to SSH answering on a fresh install | Poll `ssh hostname` every 60 s | 27 minutes, including the 8.5 GB ISO download |

## Snapshots taken

| Name | Contents |
|---|---|
| `clean` | Fresh install, sshd enabled, UAC off |
| `uac-on` | `clean` with `EnableLUA=1` |

## Not measured

- Time from a snapshot restore to SSH answering: the polling loop treated the exit code 1 of `query user` as failure, so the figure it printed (301 s) is the loop's limit, not the boot time.
- Screen capture of the session from inside the session.
