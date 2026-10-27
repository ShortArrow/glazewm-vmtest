# 005: Platform that runs the Windows guests

- Status: Accepted

## Context

The harness restores a Windows guest to a snapshot, runs a scenario in it and observes it, many times over (docs/PRD.md). The forces:

- **Automation and maintenance cost.** The harness is worth its upkeep only if it keeps running unattended. A platform that can stop working without a change to the harness is a recurring cost.
- **dockur/windows needs KVM.** Its containers run Windows under QEMU with KVM. On a Windows host, a Linux container runs inside a WSL2 virtual machine, so KVM there is nested virtualization inside Hyper-V.
- **Nested virtualization went away with no change to the harness.**
  - Situation on 2026-10-27, on two Windows 11 hosts running build 26300.9711:
    - The WSL2 kernel logged `Hyper-V: Nested features: 0x0`.
    - No `/dev/kvm` and no `svm` flag appeared.
  - What it held across:
    - with `nestedVirtualization=true` in `.wslconfig`;
    - on WSL 2.6.3 and on WSL 3.0.3;
    - with memory integrity (HVCI) on and off.
  - On one of the two hosts the same setup ran the dockur guest on 2026-10-16. WSL gave no warning.
- **Hyper-V runs the guest as a first-level virtual machine.** It needs no nested virtualization. Every Windows 11 Pro host has it.
- **Monitor changes do not depend on the hypervisor.** They go through an IddCx virtual display driver inside the guest (docs/spikes/2026-10-07-iddcx-virtual-display-driver.md). On Hyper-V, whose synthetic video has a single output, this driver is the only per-monitor mechanism (docs/ROADMAP.md).
- **The dockur adapter and its scenarios exist; a Hyper-V adapter does not.** Three spikes in docs/ROADMAP.md are open for Hyper-V: "Snapshot type", "Guest screen capture" and "Hyper-V control through WMI".

## Decision

Windows guests run on Hyper-V, on a Windows 11 Pro host dedicated to the harness. A Hyper-V adapter implements the same ports as the dockur adapter, so the scenarios stay unchanged. The dockur adapter is kept for a Linux host with KVM.

## Options not taken

| Option | Reason |
|---|---|
| dockur/windows on a Windows host | Depends on nested virtualization in the WSL2 VM, which an update to the host can remove without warning, as it did on both hosts |
| dockur/windows on a Linux host with KVM | Needs no nested virtualization and no change to the harness, but no Linux host with enough memory and disk for a Windows 11 guest is available |
| QEMU with the Windows Hypervisor Platform (WHPX) on the Windows host | Needs no nested virtualization and keeps qcow2 snapshots and `screendump`. But whether a Windows 11 guest runs stably on it is unknown, and the unattended install that dockur provides would have to be rebuilt |
| Memory integrity (HVCI) turned off on the host | Did not bring nested virtualization back |

## Consequences

- The three Hyper-V spikes listed in Context block phase 1 on this platform. In docs/ROADMAP.md, Hyper-V and dockur/windows swap places between phases 1 and 2.
- The guest image is built without dockur's unattended install. Its steps in docs/GUEST-IMAGE.md are rewritten for Hyper-V.
- Where the runner runs, on the Hyper-V host or on another machine controlling it remotely, is decided with the Hyper-V adapter.
- The constraint in docs/PRD.md that names Docker Desktop and nested virtualization changes to name Hyper-V.

## History

| Date | Replaces version | Change |
|---|---|---|
| 2026-10-27 | — | Decided |
