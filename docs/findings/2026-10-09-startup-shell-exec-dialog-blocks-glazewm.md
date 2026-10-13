# Finding: a failing startup `shell-exec` blocks glazewm until two dialogs are dismissed

- Date: 2026-10-09
- Status: fixed on branch `fix/failed-command-blocks-wm` of ../glazewm (`8c63768a`, `b801904d`); not reported upstream. Upstream issue glzr-io/glazewm#705 covers the second dialog as a warning, not as a stall
- SUT: glazewm `5709ad0a`, built as in docs/spikes/2026-10-09-first-glazewm-run.md
- Guest: snapshot `daemon-task` of docs/GUEST-IMAGE.md, which has no zebar

## Reproduction

1. Start glazewm with its own `resources/assets/sample-config.yaml`, whose `general.startup_commands` is `['shell-exec zebar']`, on a machine where `zebar` cannot be found.
2. Run `glazewm-cli query monitors`.

## Observed

| Moment | Screen | `glazewm-cli query monitors` |
|---|---|---|
| After start-up | Shell dialog "Windows cannot find 'zebar'" | No answer within 8 s, also from a second session |
| After dismissing it with Enter | glazewm dialog "Non-fatal error — Shell exec failed for 'zebar' … (0x800704C7)" | No answer within 8 s |
| After dismissing that one too | No dialog | Answered in 280 ms; the queries that were waiting completed |

While either dialog is open, glazewm answers no IPC request.

## Mechanism, as far as the source shows

`shell_execute_ex` in packages/wm-platform/src/dispatcher.rs calls `ShellExecuteExW` with `fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC` and without `SEE_MASK_FLAG_NO_UI`, so the shell reports a missing program with a modal dialog on the calling thread. The second dialog is glazewm's own report of the resulting error: `start_wm` in packages/wm/src/main.rs shows every non-fatal error with `show_error_dialog`, a `MessageBoxW` with `MB_SYSTEMMODAL` that returns only when dismissed, on the thread that runs the event loop. That loop also takes IPC messages and window events, so the second dialog stops both, after any failing command and not only `shell-exec`.

The scenario `glazewm_answers_ipc_after_a_startup_command_fails` in crates/adapters/tests/live_dockur.rs reports `glazewm_answers_ipc` as violated on `5709ad0a` and passes on the fix branch, which adds `SEE_MASK_FLAG_NO_UI` and shows the non-fatal error dialog on its own thread.

## Effect on a user

A user whose configuration starts a program that is not installed gets a window manager that ignores IPC and window events until they find and dismiss both dialogs. glazewm leaves the choice of status bar to the user, and the sample configuration starts zebar, so this includes everyone who uses another bar or none and keeps the sample's startup command.

## Open

- Whether window management stops while the dialogs are open has been read from the source, not observed.
- Whether `shell-exec` from a keybinding blocks the same way.
- Whether upstream considers the dialog intended.
