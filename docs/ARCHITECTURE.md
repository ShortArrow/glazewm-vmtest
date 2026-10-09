# Architecture

Terms follow the "Terms" section of docs/PRD.md.

## Structure

```
┌───────────────────────────────────────────────────┐
│ Domain                                            │
│   Topology, WindowKind, Scenario, Step,           │
│   Invariant, Baseline, Verdict                    │
├───────────────────────────────────────────────────┤
│ Application                                       │
│   ScenarioRunner, SutBuilder, ArtifactStore       │
│   Ports: Hypervisor, DisplayController,           │
│          SessionController, WorkloadController,   │
│          GuestExecutor, Observer                  │
├───────────────────────────────────────────────────┤
│ Adapters                                          │
│   hyperv / dockur-windows / rdp-client /          │
│   vnc-client / guest-daemon                        │
└───────────────────────────────────────────────────┘
```

Dependencies point upward only. The domain knows nothing about virtualization platforms or the SUT's implementation.

### Domain

- `Topology`: a value object holding the set of displays; each display has an identifier, bounds, working area, DPI, and a primary flag.
- `WindowKind`: one value per axis of the "Window kinds" section of docs/PRD.md.
- `Scenario`: Given (initial `Topology`, SUT configuration), When (a sequence of `Step`), Then (a set of `Invariant` and, optionally, a `Baseline`).
- `Step`: add monitor, remove monitor, change settings, RDP connect / disconnect, VNC connect / disconnect, sleep / resume, lock / unlock, start / stop load, spawn windows, hang / resume window, replace and reload SUT config, wait.
- `Invariant`: a pure function from an observation to pass/fail plus the difference.
- `Baseline`: the canonical observations recorded from a reference run of a scenario, with the SUT commit they were taken from.
- `Verdict`: the per-step result: invariant violations, deviations from the baseline, unsettled, or skipped, each reported separately.

### Application

`ScenarioRunner` executes in this order:

1. Restore the guest from a snapshot via `Hypervisor`, and wait until the guest daemon reports ready.
2. Apply the Given topology via `DisplayController`, and place the Given SUT configuration via `GuestExecutor`.
3. `SutBuilder` builds the SUT on the host; `GuestExecutor` deploys it, and the guest daemon starts it.
4. Wait until settled, then observe via `Observer`. This is the observation after step 0.
5. For each When `Step`: execute it through its port, wait until settled, observe, evaluate the `Invariant`s, and either record the observation into a new `Baseline` or compare it with the existing one.
6. Store logs, state dumps, screenshots, and the executed operation sequence in `ArtifactStore`.

A recorded operation sequence can be given back to the runner as the When of a scenario, which is how a failed run is replayed. A generator produces When sequences from a seed.

Port responsibilities:

| Port | Responsibility |
|---|---|
| `Hypervisor` | Start, stop, create and restore snapshots |
| `DisplayController` | Change the guest topology per monitor |
| `SessionController` | Connect and disconnect RDP / VNC clients; lock and unlock; sleep and resume |
| `WorkloadController` | Start and stop CPU / memory / GPU load; create, hang, resume, and close test windows |
| `GuestExecutor` | Transfer files to the guest and send requests to the guest daemon, which listens on TCP inside the guest and speaks JSON, one object per line |
| `Observer` | Capture topology, window bounds, glazewm state, zebar window placement, process liveness, IPC responsiveness, and screenshots |

### Adapters

Each virtualization platform implements `Hypervisor`, `DisplayController`, and `SessionController`, and the screenshot part of `Observer`.
`GuestExecutor`, `WorkloadController`, and the rest of `Observer` go through the guest daemon, so they are shared by every Windows guest.

The guest daemon runs in the interactive session of the user that logs on automatically. After every snapshot restore the runner delivers the current daemon binary and starts it through a scheduled task registered in the guest image (docs/GUEST-IMAGE.md), so the image does not have to be rebuilt when the daemon changes. It starts the SUT and test windows, generates load, and reads the topology and window bounds from inside that session. Processes reach that session through scheduled tasks with an interactive logon type: `RunLevel Limited` for glazewm, zebar, and test windows at the user's integrity level, `RunLevel Highest` for elevated test windows. The guest image has UAC enabled, as a user's machine does; without it every process runs elevated and the privilege axis of the window kinds cannot be told apart.
Test windows come from test-window applications, one per UI framework, each written in that framework's own language and built once into the guest image. Every test-window application accepts the same control protocol (create, hang, resume, close, with the window kind's other axes as parameters), so adding a framework means adding one application and no change to the runner.

When an operation cannot be performed on a platform, or in the current session state (for example a per-monitor change while an RDP session owns the display), the adapter reports it as unsupported and the step is reported as skipped. A scenario whose steps were all skipped is reported as not run.

## Design intent

### Verdicts come from invariants and baselines, on settled states

The oracle evaluates observations, not reference screenshots.
Screenshots vary with resolution, fonts, theme, and timing, and a pixel difference cannot point at a cause.
Screenshots are kept only as artifacts for investigating failures.

Observations are taken only once the guest has settled. The settle window is at least zebar's monitor polling interval, because zebar detects monitor changes by polling every 4 seconds rather than by window messages. A fixed sleep in place of settling would hide races, so a step that does not settle within its limit is reported as unsettled.

An invariant states what must hold whatever the SUT is meant to do; a violation is a bug. The invariants:

- Once settled, glazewm's monitor count equals the guest OS display count, except that it stays at 1 while all displays are gone.
- After every step the glazewm and zebar processes are alive, and glazewm answers an IPC query within the time limit. The guest daemon measures this inside the guest, and the limit applies only while no load step is active.
- A window that has stopped processing messages does not stop glazewm from managing the other windows.
- Once settled, glazewm manages no window that no longer exists, and every window it manages is a top-level window.
- Once settled, the actual bounds of every tiled window, read from the OS, lie within the working area of its monitor.
- Once settled, for each monitor that the zebar configuration targets, exactly one bar window sits on that monitor.

A baseline pins what the SUT currently does where nobody has specified what it should do; a deviation is a change in behaviour, to be judged by a person.
Baselines compare a canonical form of the observation: monitors identified by position and bounds, workspaces by name, test windows by the identifier the guest daemon gives them. Values that differ between identical runs — container UUIDs, window handles, timestamps — are dropped.
When a behaviour is later specified, the baseline for it is replaced by an invariant or by a unit test in the SUT's repository.

`bind_to_monitor` is the case that drove this split. glazewm documents it as forcing a workspace onto a monitor "if it exists"; what happens otherwise is unspecified, so scenarios cover it with baselines. The index is a position among monitors sorted left to right, then top to bottom, not an identity of a physical monitor, and glazewm applies the binding when a monitor is added, not when one is removed or the monitors are rearranged. Baselines for scenarios that change the topology are therefore recorded per topology.

### The guest daemon acts inside the interactive session

The harness never uses the host's keyboard and mouse; host input would interrupt the developer and rule out parallel runs.
Inside the guest, a process started through SSH or PowerShell Direct gets a session with no interactive desktop: windows it creates are invisible, and display queries it makes describe that session rather than the console. The guest daemon therefore lives in the interactive session and does the guest-side work there. SSH and PowerShell Direct are used only to deliver files and reach the daemon.

### How topology changes is confined to adapters

The mechanism for adding or removing a monitor differs per platform: multi-head virtual GPUs, an in-guest virtual display driver, the RDP client's monitor layout, and so on.
Scenarios declare monitors only; the mechanism lives inside `DisplayController` implementations.
When the same scenario yields different verdicts on different platforms, that difference is itself something to investigate.

### The SUT is built on the host

A toolchain inside the guest would bloat the snapshot and mix build-environment differences into the verdict.
The SUT is built on the host from `../glazewm` and `../zebar` at a specified commit, and only the output is handed to the guest: the glazewm binaries, and the zebar binary together with its frontend build and bundled resources. The guest image provides the WebView2 runtime and the Microsoft Visual C++ redistributable, which glazewm's release build imports (`vcruntime140.dll`). The glazewm configuration is passed through `GLAZEWM_CONFIG_PATH`.
