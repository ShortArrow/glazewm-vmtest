# 004: Transport between the guest daemon and the test-window applications

- Status: Proposed

## Context

The guest daemon tells test-window applications to open windows, stop and resume their message processing, and close them (docs/ARCHITECTURE.md, "Adapters"). There is one application per UI framework, each written in that framework's own language.

The forces:

- Every framework's language has to speak the transport: Rust, C#, JavaScript, Dart, Java, C++ and others.
- An elevated test window has to be started through a scheduled task with `RunLevel Highest`, because the daemon runs at the user's integrity level. No pipe then exists between the daemon and the application. A process started that way, at high integrity, connects to a loopback listener at medium integrity; this was checked in the guest image on 2026-10-16.
- The harness already talks JSON, one object per line, between the runner and the daemon (ADR 002).

## Decision

The daemon listens on a loopback TCP port it picks for each application and starts the application with that address as its only argument. The application connects to it and reads commands, JSON, one object per line, answering each with one reply line. The command and reply types are part of the protocol crate. An application exits when the connection closes.

## Options not taken

| Option | Reason |
|---|---|
| Standard input and output of a child process | An application started through a scheduled task, as an elevated one is, has no pipe to the daemon, so the privilege axis would need a second transport |
| Named pipes | The standard libraries of some frameworks' languages, Dart among them, have no named-pipe client, so each such application would need its own workaround |
| FlatBuffers instead of JSON | Its one gain here is a schema from which every language's types are generated, and a JSON Schema generated from the protocol crate gives the same while keeping messages readable in logs and writable with each language's standard library. Its speed does not matter at a few messages per step, and it defines no framing on a stream, which would have to be added |

## Consequences

- The port is reachable by any process in the guest, without authentication. This holds on the same premises as ADR 002: the commands are typed and start nothing, and the guest holds no secrets.
- An application that does not connect within a limit set in the daemon's configuration is reported as a failed request, not waited for.

## History

| Date | Replaces version | Change |
|---|---|---|
| 2026-10-14 | — | Decided |
