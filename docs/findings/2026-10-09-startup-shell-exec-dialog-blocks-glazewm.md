# Finding: a failing startup `shell-exec` blocks glazewm until two dialogs are dismissed

- Date: 2026-10-09
- Status: candidate; not reported upstream
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

`shell_execute_ex` in packages/wm-platform/src/dispatcher.rs calls `ShellExecuteExW` with `fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC` and without `SEE_MASK_FLAG_NO_UI`, so the shell reports a missing program with a modal dialog on the calling thread. The second dialog is glazewm's own report of the resulting error. The thread that ran the command is the one that serves IPC, which is consistent with both dialogs blocking it; this has not been confirmed in a debugger.

## Effect on a user

A user whose configuration starts a program that has been uninstalled gets a window manager that ignores IPC (and, if the same thread handles window events, does not manage windows) until they find and dismiss both dialogs.

## Open

- Whether window management, and not only IPC, stops while the dialogs are open.
- Whether `shell-exec` from a keybinding blocks the same way.
- Whether upstream considers the dialog intended.
