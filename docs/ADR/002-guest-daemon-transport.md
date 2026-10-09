# 002: Transport between the runner and the guest daemon

- Status: Proposed

## Context

The runner on the host asks the guest daemon, resident in the guest's interactive session, to start processes, change load, and report the topology, window bounds, and process liveness (docs/ARCHITECTURE.md, "Adapters").

The forces:

- Settle detection polls the guest repeatedly after every step, so the cost of one request bounds how finely settling can be observed.
- Work has to happen in the interactive session; a process started through SSH runs in session 0 and cannot see that desktop.
- The same transport has to work on every platform: dockur/windows forwards container ports to `127.0.0.1` on the host, and Hyper-V guests are reachable over the VM network.
- glazewm's own IPC is a TCP server in the guest, and the harness already reaches it across sessions.

## Decision

The guest daemon listens on a TCP port inside the guest. The runner connects to it through the platform's forward to `127.0.0.1` on the host. Requests and responses are JSON, one object per line. SSH and PowerShell Direct only deliver files and register the daemon to start at logon.

## Options not taken

| Option | Reason |
|---|---|
| A command over SSH per request | Starts SSH and PowerShell for every request, too slow for settle polling, and every request that must run in session 1 needs a scheduled task |
| Request and response files in a shared folder | High latency and no prompt way to notice a new file |

## Consequences

- The daemon has to be running before a scenario starts; the runner waits for it after every snapshot restore.
- The port is published only on `127.0.0.1`, since the daemon starts processes and has no authentication.
- Request and response types are shared between the runner and the daemon as one Rust crate.

## History

| Date | Replaces version | Change |
|---|---|---|
| 2026-10-09 | — | Decided |
