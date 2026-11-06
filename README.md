# glazewm-vmtest

A test harness that reproduces display configuration changes inside virtual machines to verify how [GlazeWM](https://github.com/glzr-io/glazewm) and [Zebar](https://github.com/glzr-io/zebar) respond to them.

The changes under test:

- Adding and removing monitors
- Changing resolution, scaling, arrangement, and the primary monitor
- Connecting, disconnecting, and reconnecting an RDP session
- Connecting and disconnecting a VNC client
- Resuming from sleep, locking, and unlocking

No physical monitors or real hardware are involved. The same scenarios run repeatedly on Windows guests hosted by Hyper-V and by QEMU/KVM (dockur/windows). macOS guests need an Apple Silicon Mac host; see docs/ROADMAP.md.

## Targets

| Target | Default checkout | Platforms |
|---|---|---|
| glazewm | `../glazewm` | Windows, macOS |
| zebar | `../zebar` | Windows |

The harness builds the targets on the host and deploys only the build output to the guest. Nothing is built inside the guest.

## Documentation

- [docs/PRD.md](docs/PRD.md) — goals, use cases, requirements, non-goals
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — structure, dependency direction, design intent
- [docs/ROADMAP.md](docs/ROADMAP.md) — phases, spikes, open questions
- [docs/GUEST-IMAGE.md](docs/GUEST-IMAGE.md) — how the Windows guest and its snapshots are built
- [docs/CI.md](docs/CI.md) — the CI workflows and the self-hosted runner the live scenarios run on
- [docs/ADR/](docs/ADR/) — decisions between alternatives, with their reasons

Every `*.md` has a Japanese translation beside it as `*.jp.md` ([README.jp.md](README.jp.md)). Where the two differ, the English file applies.

## Status

Phase 0 of docs/ROADMAP.md is in progress. Scenarios run end to end against the Hyper-V guest or the dockur/windows guest: `cargo test -p vmtest-adapters --test live -- --ignored`, after building the guest daemon and glazewm and setting the variables that test's header describes. Spike results are in docs/spikes/, glazewm behaviour found along the way in docs/findings/.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
glazewm and zebar, which the harness builds from sibling directories and runs as separate programs, keep their own license, GPL-3.0; nothing of theirs is in this repository.
