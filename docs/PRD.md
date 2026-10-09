# PRD: glazewm-vmtest

## Goal

Make display-configuration bugs in glazewm and zebar reproducible and detectable without physical hardware.

These bugs are normally triggered by hand — unplugging a monitor, dropping an RDP session — so the reproduction steps depend on a particular person and a particular desk of equipment.
The harness replaces that manual work with virtualization-platform operations, so the same steps run from the same initial state as many times as needed.

## Users

- Contributors to glazewm and zebar. They write a scenario that reproduces a bug and confirm it fails before the fix and passes after it.
- AI coding agents doing the same, without taking over the developer's screen and keyboard.

## Terms

| Term | Meaning |
|---|---|
| SUT | System under test: glazewm and zebar built from specific commits |
| Guest | The virtual machine (or the VM inside a container) that runs the SUT |
| Guest daemon | A program resident in the guest's interactive desktop session that starts the SUT and test windows and observes the guest on the harness's behalf |
| Snapshot | A saved guest state that every run starts from (a checkpoint, in Hyper-V's terms) |
| Topology | The set of displays the guest OS sees. Per display: identifier, bounds, working area, DPI, and whether it is primary |
| Settled | The guest is settled when, for the settle window, the topology, the set of windows, and glazewm's IPC event stream have all been quiet |
| Scenario | Given (initial topology and SUT configuration), When (a sequence of operations that change topology, session, load, or windows), Then (invariants and a baseline) |
| Invariant | A condition that must hold whatever the SUT is meant to do; a violation is a bug |
| Baseline | The SUT's observed behaviour, recorded from a reference run where no expected behaviour is specified; a deviation is a change in behaviour for a person to judge |

## Use cases

| ID | Trigger | Example |
|---|---|---|
| UC-1 | Monitor added or removed | 1 → 3 → 2 monitors; removing only the middle one |
| UC-2 | Display settings changed | Resolution, scaling, arrangement, primary monitor |
| UC-3 | RDP session changes | Connect, disconnect, reconnect from a client with a different monitor count or resolution |
| UC-4 | VNC client changes | Connect and disconnect; that the guest topology does *not* change is also under test |
| UC-5 | Power and session state changes | Resume from sleep, lock and unlock |
| UC-6 | All displays lost, then restored | Remove every monitor, then add one |
| UC-7 | Configuration assumes a monitor layout that differs from the actual one | Start-up, or config reload, on 1 monitor while glazewm `bind_to_monitor` names a third monitor; a zebar `monitorSelection` of type `index` or `name` that matches no monitor |
| UC-8 | Resource exhaustion in the guest | CPU, memory, or GPU saturated before or during a topology change |
| UC-9 | Unresponsive windows | A window stops processing messages while glazewm moves, resizes, or hides it, including across a topology change |
| UC-10 | Bursts and short-lived windows | Many windows created and destroyed within a short span; a window or dialog destroyed or hidden within milliseconds of being created, before glazewm finishes handling it; alone or overlapping a topology change |
| UC-11 | Window kinds | Each kind under "Window kinds" below, managed across topology changes |
| UC-12 | Combinations and repetition | Randomly generated operation sequences, long-running loops |

## Functional requirements

| ID | Requirement | Use cases |
|---|---|---|
| FR-1 | Scenarios are written declaratively as Given / When / Then | All |
| FR-2 | Every run restores the guest from the same snapshot | All |
| FR-3 | The SUT is built on the host from a specified commit, deployed to the guest, and started in the guest's interactive session after the Given topology and configuration are in place | All |
| FR-4 | The guest topology can be changed per monitor: add, remove, modify | UC-1, UC-2, UC-6 |
| FR-5 | RDP and VNC clients can be connected and disconnected from the host | UC-3, UC-4 |
| FR-6 | Sleep, resume, lock, and unlock can be triggered from the host | UC-5 |
| FR-7 | After every step settles, the harness captures, from the guest's interactive session, the topology, the actual bounds of every window, glazewm state (output of `glazewm query`), and zebar window placement | All |
| FR-8 | The oracle evaluates invariants and reports the failing step and the difference | All |
| FR-9 | Each run stores SUT logs, state dumps, and screenshots as artifacts | All |
| FR-10 | A failed scenario can be re-run verbatim from its recorded operation sequence | UC-12 |
| FR-11 | Given sets the SUT configuration (glazewm config, zebar widget packs) independently of the initial topology, and a step can replace and reload it | UC-7 |
| FR-12 | CPU, memory, and GPU load in the guest can be started and stopped as steps, at a specified intensity | UC-8 |
| FR-13 | Test windows can be created in the guest as steps: of a specified kind, a specified number within a specified span, and windows that stop and resume processing messages | UC-9, UC-10, UC-11 |
| FR-14 | Every observation records whether the glazewm and zebar processes are alive and whether glazewm answers an IPC query within a time limit measured inside the guest | UC-8, UC-9, UC-10 |
| FR-15 | A scenario can record its observations as its baseline; later runs report deviations from the baseline separately from invariant violations | All |
| FR-16 | Operation sequences can be generated at random from a seed, and a scenario can be repeated for a specified count or duration | UC-12 |
| FR-17 | A step that does not settle within a time limit is reported as unsettled, not as passed | All |

## Window kinds

Windows differ along independent axes, and a scenario names one value per axis. Coverage is judged per axis, and pairwise across axes.

| Axis | Values |
|---|---|
| UI framework | Win32; .NET (WinForms, WPF, WinUI 3, Avalonia); UWP (hosted by `ApplicationFrameHost`); web-based (Electron, Tauri / WebView2); Rust-native (winit, egui, gpui); Qt; GTK; Flutter; Java (Swing); game-style (SDL, GLFW); console (conhost, Windows Terminal) |
| DPI awareness | Unaware, system aware, per-monitor, per-monitor v2 |
| Privilege | Same integrity level as glazewm, elevated (administrator) |
| Relationship | Unowned top-level; owned modeless window (tool palette, find dialog); owned modal dialog (including message boxes and common dialogs); popup (menu, tooltip, notification); child window (`WS_CHILD`, MDI child) |
| Lifetime | Persistent; short-lived (destroyed within milliseconds of creation); shown then hidden within milliseconds; repeatedly shown and hidden |
| Style | Normal, borderless, tool window, layered / transparent, topmost, minimum / maximum size constraints |
| State | Normal, minimized, maximized, fullscreen (borderless), exclusive fullscreen (DirectX) |
| Responsiveness | Responsive, not processing messages |

## Order of work

Regression coverage comes first: the harness pins the current behaviour of glazewm and zebar as baselines before any behaviour is specified, so a change in behaviour is noticed whether or not it was intended. Specifying expected behaviour and driving fixes test-first follows, one use case at a time, once that use case has a baseline.

## Non-functional requirements

- Isolation: the harness never sends input to the developer's desktop session. Inside the guest, the guest daemon acts within the guest's own interactive session.
- Headless: runs without opening a host window onto the guest display.
- Reproducibility: the same scenario, SUT, and snapshot execute the same operation sequence. A verdict that varies between runs is reported as flaky, without attributing the cause to either the SUT or the harness.

## Non-goals

- Replacing glazewm or zebar unit tests. Regression tests for bugs found by the harness are written as unit tests in the respective repository wherever possible.
- Fixing the bugs. Fixes are made in the respective repository.
- Performance measurement.
- Reproducing bugs specific to real GPU drivers or real monitors.

## Constraints

- Windows guests run on a Windows 11 Pro host, using Hyper-V and Docker Desktop (WSL2 backend).
- dockur/windows requires KVM. On a Windows 11 host this goes through nested virtualization in WSL2.
- In the guest, the SUT runs in the interactive desktop session of a user that logs on automatically. Processes started through SSH or PowerShell Direct run in a non-interactive session without that desktop.
- macOS guests run only on an Apple Silicon Mac host, through Virtualization.framework, and no such host is available. The macOS software license agreement permits macOS virtual machines only on Apple-branded hardware already running macOS, at most two additional copies per machine, for purposes including software development and testing; Virtualization.framework enforces the same limit of two.
- Windows guests use an evaluation edition or a license the user owns.

## Acceptance criteria

- At least one known display-related bug in glazewm or zebar is expressed as a scenario that fails on the commit before the fix and passes on the commit after it.
- Running the same scenario 10 times in a row against the same SUT yields the same verdict every time, and no run is reported as unsettled.
- For each baseline scenario, running it against a SUT commit that changes behaviour in that scenario's area reports a deviation.
