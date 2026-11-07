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
- `Step`: add monitor, remove monitor, change settings, RDP connect / disconnect, VNC connect / disconnect, sleep / resume, display off / wake, lock / unlock, start / stop load, spawn windows, hang / resume window, replace and reload SUT config, wait.
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
| `GuestExecutor` | Transfer files to the guest and send requests to the guest daemon, which listens on TCP inside the guest and speaks JSON, one object per line. The daemon has no authentication and can make glazewm run commands, so its port is never reachable from a network: it is reached through an SSH port forward, or through a port published on the host's loopback |
| `Observer` | Capture topology, window bounds, glazewm state, zebar window placement, process liveness, IPC responsiveness, and screenshots |

### Adapters

Each virtualization platform implements `Hypervisor`, `DisplayController`, and `SessionController`, and the screenshot part of `Observer`.
The Hyper-V `Hypervisor` turns the guest off, restores a checkpoint taken with the guest off, and starts it, through WMI alone. It runs PowerShell on the Hyper-V host, either on the runner's machine or through SSH to the host, so the runner runs on the Hyper-V host or on any machine that can reach it and the guest. A checkpoint of a running guest would restore in about 4 seconds instead of about 13, but it would hold the guest daemon that was running when it was taken, and a guest daemon older than the runner's would go unnoticed; a checkpoint of a stopped guest takes the guest daemon delivered on each run. The guest's SSH is reached at its address on the network, and the guest daemon, which listens on the guest's loopback, through an SSH port forward.
`GuestExecutor`, `WorkloadController`, and the rest of `Observer` go through the guest daemon, so they are shared by every Windows guest. Load comes from a separate load generator process the daemon starts and kills, one per kind of load, so that the daemon itself stays as responsive as any other process; it is delivered beside the daemon. CPU load keeps every logical processor busy for its percent of each 100 ms; memory load holds its percent of the physical memory and writes to every page of it each second. GPU load waits on the spike on GPU load without a physical GPU.

The guest daemon runs in the interactive session of the user that logs on automatically. After every snapshot restore the runner delivers the current daemon binary and starts it through a scheduled task registered in the guest image (docs/GUEST-IMAGE.md), so the image does not have to be rebuilt when the daemon changes. It starts the SUT and test windows, generates load, and reads the topology and window bounds from inside that session. glazewm and test windows run in that session at the user's integrity level as children of the daemon, whose scheduled task runs with `RunLevel Limited`, and so does zebar, which glazewm starts through its startup commands as glazewm's sample configuration does, or which the daemon starts itself when a scenario needs zebar's standard error kept; elevated test windows reach it through a scheduled task with an interactive logon type and `RunLevel Highest`. The guest image has UAC enabled, as a user's machine does; without it every process runs elevated and the privilege axis of the window kinds cannot be told apart.
Test windows come from test-window applications, one per UI framework, each written in that framework's own language. The runner delivers each application with the guest daemon after every restore; a runtime an application needs is installed in the guest image. Every test-window application accepts the same control protocol (create, hang, resume, close, with the window kind's other axes as parameters) over a loopback connection to the daemon (docs/ADR/004-test-window-application-transport.md), so adding a framework means adding one application and no change to the runner. The daemon refuses a request that asks for more test windows, or a longer span, than its limits allow, so that no request keeps it busy without end; the limits come from a file the runner delivers beside the daemon, never from a request, and default to 500 windows and 60 seconds. The same file sets how long a started application may take to connect back, 3 seconds by default; one that does not connect in time fails the request. An application answers a create once its window exists and is the foreground window, so that windows created one after another are placed the same way on every run.

When an operation cannot be performed on a platform, or in the current session state (for example a per-monitor change while an RDP session owns the display), the adapter reports it as unsupported and the step is reported as skipped. A scenario whose steps were all skipped is reported as not run.

## Design intent

### Verdicts come from invariants and baselines, on settled states

The oracle evaluates observations, not reference screenshots.
Screenshots vary with resolution, fonts, theme, and timing, and a pixel difference cannot point at a cause.
Screenshots are kept only as artifacts for investigating failures.

Observations are taken only once the guest has settled. The settle window is at least zebar's monitor polling interval, because zebar detects monitor changes by polling every 4 seconds rather than by window messages. A fixed sleep in place of settling would hide races, so a step that does not settle within its limit is reported as unsettled.

An invariant states what must hold whatever the SUT is meant to do; a violation is a bug. The invariants:

- Once settled, glazewm's monitor count equals the guest OS display count, except that it stays at 1 while all displays are gone.
- Once settled, every monitor glazewm tracks displays exactly one workspace. A monitor without one is a target that commands moving or focusing windows onto it fail on.
- After every step the glazewm and zebar processes are alive, and glazewm answers an IPC query within the time limit. The guest daemon measures this inside the guest, and the limit applies only while no load step is active. The limit is set per scenario and defaults to 2 seconds. A process is alive when one runs from the executable the harness deployed, compared by full path. A glazewm that is not running or does not answer is a violation of this invariant alone, since it reports no monitors to compare, and the scenario goes on to its next step. A glazewm that exits during the IPC query counts as not running, not as unanswered.
- A window that has stopped processing messages does not stop glazewm from managing the other windows. This is judged by the other invariants holding while such a window exists: glazewm answers IPC in time, and every other tiled window lies within its working area. The stopped window itself is not compared, since nothing can move a window that does not process messages.
- Once settled, glazewm manages no window that no longer exists, and every window it manages is a top-level window. This covers every window glazewm reports, not only test windows: the guest daemon asks the OS about each handle, and a window with the `WS_CHILD` style counts as a child.
- Once settled, the actual bounds of every tiled window, read from the OS, lie within the working area of its monitor. The bounds are the visible frame, without the invisible resize borders. Only test windows are compared, since the harness reads the bounds of no other window, and only those on displayed workspaces, since glazewm's hide method decides where the others are.
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
