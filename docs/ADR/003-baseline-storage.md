# 003: How baselines are stored and compared

- Status: Accepted

## Context

A baseline pins what the SUT currently does where nobody has specified what it should do (docs/PRD.md, FR-15). A later run reports where it deviates, apart from invariant violations, and a person decides whether the change is accepted; an accepted change is recorded again.

The forces:

- A deviation is judged by a person, so it has to be readable as a difference between two recorded states, and a re-recording has to be reviewable like any other change to the repository.
- The baseline has to name the SUT commit it was recorded from, since the SUT checkout and the SUT build can differ.
- Deviations and invariant violations are reported separately, so the comparison belongs to the domain model beside the invariants, not to the test framework.
- What is observed grows with the harness: windows join when test windows exist (FR-13).

## Decision

Each scenario with a baseline has one JSON file, baselines/<scenario>.json, committed to the repository. It holds the SUT commit and, for start-up and each step, the settled observation in canonical form, or `null` for a step that did not settle.

The observation recorded is the guest OS's display bounds and glazewm's monitors, each with its bounds, working area, DPI and focus, and the workspaces on it with their name, whether displayed, focus and tiling direction. Displays and monitors are ordered by position; workspaces keep glazewm's order. Container ids, window handles and device names are not recorded, because they change between identical runs.

The domain model compares a run with its baseline step by step and returns the steps that differ. The file format is defined in the adapters, apart from the domain types. A file with a field the reader does not know, or with a different number of steps than the scenario, is not compared; it is recorded again.

A run records the file instead of comparing when `VMTEST_RECORD=1` is set, and then requires the SUT commit to be named in `VMTEST_SUT_COMMIT`.

## Options not taken

| Option | Reason |
|---|---|
| insta (snapshot-testing crate) | Its comparison is a test assertion, so deviations cannot sit beside invariant violations in the domain model's results; it also adds a crate and a command-line tool |
| YAML or another format of our own | Same structure as JSON with one more dependency; the common Rust YAML crate is no longer maintained |
| The whole of glazewm's answer, minus the values that change between runs | Any field glazewm adds becomes a deviation, and the list of values to drop has to follow glazewm |

## Consequences

- A scenario whose steps change makes its baseline stale, and the baseline is recorded again.
- Behaviour outside the recorded fields, such as a change in DPI handling that leaves the DPI value alone, is not caught until a field that shows it is added; adding a field makes every baseline stale.
- Recording needs the SUT commit named by hand.

## History

| Date | Replaces version | Change |
|---|---|---|
| 2026-10-14 | — | Decided |
