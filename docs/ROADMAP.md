# Roadmap

## Phases

| Phase | Scope | Done when |
|---|---|---|
| 0 | Technical spikes | Feasibility and limits of every spike below are known |
| 1 | Record baselines for monitor add/remove and RDP disconnect/reconnect scenarios on Hyper-V (docs/ADR/005-windows-guest-platform.md), using Win32 test windows only | The acceptance criteria in docs/PRD.md are met on Hyper-V |
| 2 | Run the same scenarios on dockur/windows on a Linux host with KVM | Phase 1 scenarios run unchanged |
| 3 | Random operation-sequence generation, repeated runs, and test windows for the other UI frameworks, with a JSON Schema generated from the protocol crate and a conformance test that every test-window application passes | A failed generated sequence can be re-run from its record |
| 4 | Test glazewm on a macOS guest on a Mac host (Virtualization.framework; Tart or similar) | On hold until an Apple Silicon Mac host is available. Starts with a spike on whether a macOS guest can present multiple displays and their hot-plug |

## Phase 1 scenario: RDP disconnect with audio redirection

Given glazewm and zebar running in the interactive session, when an RDP client logs on as the same user with audio redirection and then disconnects, then the zebar process is alive.
Audio redirection gives the guest a playback device that exists only while the client is connected, so the disconnect removes the default playback device. zebar 3.3.1 fails this scenario; a zebar with a null check on the default device ID passes it.

| Order | Work | Done when |
|---|---|---|
| 1 | The runner judges zebar's liveness. The invariant that glazewm and zebar are alive after every step is judged for glazewm only | A scenario that deploys zebar reports a violation when zebar has exited |
| 2 | The guest image accepts Remote Desktop connections without a firewall rule for them, and has a scheduled task that runs `tscon` as SYSTEM to return the session to the console. On Hyper-V the RDP client reaches the guest's loopback through an SSH port forward; dockur/windows publishes the port on the host's loopback | An RDP client on the runner's machine logs on to the guest, and no other machine can reach the guest's RDP port |
| 3 | An RDP client adapter: FreeRDP 3 under Xvfb in a container on the runner's machine, with the password given on standard input, the resolution, and audio redirection (`/sound:sys:fake`) as options | The adapter connects and disconnects, and the guest shows a playback device while it is connected |
| 4 | Steps that connect the RDP client, disconnect it, and return the session to the console, through `SessionController` | The runner's unit tests pass the steps to the port in order |
| 5 | The scenario itself | It fails with zebar 3.3.1 and passes with zebar built from `../zebar` |

What the other invariants observe while the session is disconnected is the RDP takeover spike's question; until it is answered, the scenario judges only liveness after the disconnect.

## Spikes

### Guest session

| Spike | What to find out |
|---|---|
| Interactive session | Does a user that logs on automatically, with the guest daemon started at logon, give a daemon that can start the SUT and test windows, read the topology, and capture the screen in that session? Can it start elevated windows without a UAC prompt? |
| Snapshot type | Which Hyper-V checkpoint type (standard or production) restores to a logged-on session with the daemon running, and what restoring does to the guest clock and display state |

### Topology changes

For each mechanism: can it present a monitor add/remove to the guest as a hot-plug, and are the window messages glazewm receives (`WM_DISPLAYCHANGE`, `WM_SETTINGCHANGE` with `SPI_SETWORKAREA`, `WM_DEVICECHANGE` with `DBT_DEVNODES_CHANGED`) the same as on real hardware?

| Mechanism | Platform | What to find out |
|---|---|---|
| In-guest virtual display driver (IddCx) | All | Is there a driver, usable under its license, that adds or removes one monitor while the others stay? The driver in the guest image changes its monitor count only by restarting, so every virtual monitor leaves and the new count arrives. On Hyper-V, whose synthetic video has a single output, this is the only per-monitor mechanism |
| Multi-head virtual GPU (virtio-gpu, QXL) | QEMU/KVM | Does the Windows guest driver reflect enabling and disabling individual outputs? |
| Resolution change inside the guest | All | Does a change made by the guest daemon raise the same messages as a user's change? `Set-VMVideo` only sets which resolutions a Hyper-V guest supports, so it does not serve here |
| RDP client monitor layout (`/multimon`, `/selectedmonitors`, resolution) | All | Can a reconnect present a different topology? Is a headless-capable RDP client (FreeRDP or similar) sufficient? |
| VNC client connect/disconnect | QEMU/KVM | Can it be confirmed that the guest topology stays unchanged? |

### RDP, power, and session state

| Spike | What to find out |
|---|---|
| RDP takeover | What happens to the interactive session when an RDP client logs on as the same user: topology while RDP is active, topology of the disconnected session, and how the session returns to the console afterwards (`tscon` as SYSTEM or otherwise) |
| Sleep | Which platforms let a guest enter and resume from sleep and deliver `PBT_APMSUSPEND` / `PBT_APMRESUMEAUTOMATIC` to it |
| Lock and unlock | Can the guest daemon lock the session, and how is it unlocked without host input? |

### Observation

| Spike | What to find out |
|---|---|
| Settle window | How long the settle window must be, given zebar's 4-second monitor polling, and the IPC time limit at each load level |
| Guest screen capture | Does the WMI thumbnail API (Hyper-V) or the monitor command `screendump` (QEMU) capture the guest display, including while an RDP session owns it? |
| Hyper-V control through WMI | Does WMI (`Msvm_*` classes) cover start, stop, snapshot create / restore, and which operations need the PowerShell module? |

### Workload

| Spike | What to find out |
|---|---|
| GPU load without a physical GPU | Without GPU passthrough the guest renders in software on the CPU. Does GPU load then differ from CPU load at all, or is GPU partitioning (GPU-P on Hyper-V) needed? |
| Harness under CPU load | With every vCPU of a 4-core dockur/windows guest loaded to 100 percent, a connection to the guest daemon timed out once in three runs, and the TCP connect itself did not complete within 5 seconds. Does the guest's kernel or the host's forwarding path starve, and does a higher priority for the guest daemon, or leaving one vCPU unloaded, avoid it? |
| Hung test window | Can a test-window application reliably stop its message processing and resume it later? |
| Window kinds | For each UI framework, can a small test-window application implement the control protocol, and what does the guest image need (runtimes, sideloading certificates)? For kinds that cannot be built (UWP system apps and the like), which preinstalled application stands in, and does its license allow it in a guest image? |

## Open questions

| Question | Decide by |
|---|---|
| Control protocol between the guest daemon and test-window applications (command-line arguments, named pipe, stdin JSON) | Before phase 1 |
| How the SUT is specified (sibling directory path, git submodule, commit hash) | Before phase 1 |
| Scenario format (YAML, in-code DSL) | Before phase 1 |
| Whether the SUT is built with glazewm's `ui_access` feature, as the official release is; it takes effect only when signed and installed under a secure location, and it lets glazewm manage elevated windows | Before the privilege axis is tested |
| Windows guest edition and licensing | Before phase 1 |

A decided question with realistic alternatives becomes an ADR under docs/ADR/; one settled by the requirements or constraints goes into docs/PRD.md or docs/ARCHITECTURE.md.
