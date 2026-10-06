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
- [docs/ADR/](docs/ADR/) — decisions between alternatives, with their reasons

Every `*.md` has a Japanese translation beside it as `*.jp.md` ([README.jp.md](README.jp.md)). Where the two differ, the English file applies.

## Status

Documentation only. Implementation follows the phases in docs/ROADMAP.md.
