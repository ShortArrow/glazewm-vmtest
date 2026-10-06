# 001: Implementation language

- Status: Accepted

## Context

This decision covers the host-side runner and the guest agent. Test windows come from separate test-window applications, each written in its UI framework's own language (docs/ARCHITECTURE.md, "Adapters"), so no choice here has to cover every window kind.

The runner holds the domain model (`Topology`, `Scenario`, `Invariant`, `Baseline`), executes scenarios, and contains the adapters for Hyper-V, Docker (dockur/windows), and RDP / VNC clients. It needs a type system strong enough to model the domain and a unit-test framework for test-first work.
The guest agent runs in the guest's interactive session; it generates CPU / memory / GPU load, starts the SUT and test-window applications, and reads the topology, window bounds, and process liveness.

The forces:

- glazewm and zebar are written in Rust, and their contributors work in Rust.
- glazewm's IPC messages are defined in its `wm-common` crate.
- Hyper-V is controlled through PowerShell cmdlets or WMI (`Msvm_*` classes). Rust reaches WMI through COM; .NET can also host the Hyper-V PowerShell module in-process.

## Decision

Rust for both the runner and the guest agent. Hyper-V is driven through WMI from Rust; operations WMI does not cover go through the Hyper-V PowerShell module in a child process, inside the Hyper-V adapter only.

## Options not taken

| Option | Reason |
|---|---|
| C# | Easiest Hyper-V access, but adds a second toolchain for glazewm and zebar contributors working on the runner. Its ability to create WinForms, WPF, and WinUI 3 windows does not bear on this decision, because those windows come from separate test-window applications |
| PowerShell | Most direct Hyper-V access, but weak for a typed domain model and test-first work; remains usable for one-off scripts |

## Consequences

- The runner and the guest agent share one toolchain with glazewm and zebar. Test-window applications bring their own toolchains; they are built once into the guest image, not on every run.
- The runner parses glazewm's IPC JSON with its own types that tolerate unknown fields, instead of depending on `wm-common`, so it can observe SUT commits whose IPC schema differs from the newest one.
- Whether WMI covers the Hyper-V operations is a phase 0 spike; anything missing is handled inside the Hyper-V adapter only.

## History

| Date | Replaces version | Change |
|---|---|---|
| 2026-10-07 | — | Decided |
