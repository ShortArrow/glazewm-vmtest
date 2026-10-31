# Spike: controlling a Hyper-V guest through WMI

- Date: 2026-10-31
- Spike: "Hyper-V control through WMI" in docs/ROADMAP.md
- Host: Windows 11 Pro build 26300, Hyper-V, reached over SSH as an administrator; commands run in Windows PowerShell 5.1 with the CIM cmdlets
- Guest: virtual machine `vmtest-win11` with the checkpoints of docs/GUEST-IMAGE.md, all standard checkpoints taken with the guest off

## Method

Every operation used only classes in `root\virtualization\v2`, with the Hyper-V PowerShell module not loaded. A method returning 4096 started a job; the job's `Msvm_ConcreteJob` was polled until `JobState` reached 7 (completed) or above.

| Operation | Class and method |
|---|---|
| Find the guest | `Msvm_ComputerSystem` where `ElementName` is the guest's name |
| Find a checkpoint | `Msvm_VirtualSystemSettingData` associated with the guest through `Msvm_SnapshotOfVirtualSystem`, by `ElementName` |
| Restore a checkpoint | `Msvm_VirtualSystemSnapshotService.ApplySnapshot(Snapshot)` |
| Create / delete a checkpoint | `Msvm_VirtualSystemSnapshotService.CreateSnapshot(AffectedSystem, SnapshotType=2)` / `DestroySnapshot(AffectedSnapshot)` |
| Start / power off | `Msvm_ComputerSystem.RequestStateChange(RequestedState=2 / 3)`, then `EnabledState` polled until 2 / 3 |
| Shut down the guest OS | `Msvm_ShutdownComponent.InitiateShutdown(Force=true, Reason)` associated with the guest, then `EnabledState` polled until 3 |
| Liveness | `Msvm_HeartbeatComponent.OperationalStatus` associated with the guest |

Readiness was measured from another Windows 11 machine on the guest's network: from the start request until `ssh fixture@<guest>` answered, and until `explorer.exe` ran in session 1.

## Observations

| Operation | Observed |
|---|---|
| Restore `daemon-task` with the guest off | 0.3 s; the guest stays off |
| Start | 0.5 s to `EnabledState` 2; heartbeat OK after 5 s |
| Start after restoring `daemon-task` | SSH answered after 11 s and `explorer.exe` ran in session 1 after 13 s, in each of two runs |
| Create a checkpoint while running | 2.0 s; a `Microsoft:Hyper-V:Snapshot:Realized` setting named `vmtest-win11 - (<date> - <time>)` |
| Delete that checkpoint | 0.2 s |
| Shut down the guest OS | 5–10 s to `EnabledState` 3 |
| Power off | 0.3 s |
| Associations from the guest without `-Association` | Returned some checkpoints more than once (through other associations), so passing the result to `ApplySnapshot` failed with `Unable to cast object of type 'System.Object[]'` |

## Consequences for the design

- WMI covers start, stop, shutdown, checkpoint restore, create and delete; none of them needs the Hyper-V PowerShell module.
- A heartbeat of OK does not mean the guest is ready: SSH and the logged-on session came 6–8 s later. Readiness has to be checked through the guest itself.

## Not measured

- Calling WMI from another machine (DCOM or WinRM) instead of over SSH on the Hyper-V host.
- Which checkpoint type to use, and what a restore does to the guest clock and display state; that is the spike on the snapshot type.
