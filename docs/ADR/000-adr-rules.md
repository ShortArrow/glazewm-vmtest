# 000: How ADRs are written

- Status: Proposed

## What an ADR is for

An ADR covers a decision that chose one option among several: the current choice, the reasons for it, and the reasons the other options are not taken.
ADRs are living documents, one file per decision. When the forces change, the same file is rewritten.
The body states only the current decision. Earlier decisions are reached through the History table, which points at the commits that hold them.

How the system is arranged — what is where and how it works — belongs in docs/ARCHITECTURE.md. ADRs carry the reasons behind choices.
The reason for a single change goes in its commit message in one or two sentences; a commit that rewrites an ADR names the ADR by its title.

## When to write one

- Write one for a choice with realistic alternatives that someone is likely to propose again later (virtualization platform, implementation language, scenario format, and so on).
- Do not write one when the requirements or constraints leave only one option, or for a convention with no alternative.

## Files

- `docs/ADR/NNN-kebab-case-title.md`, where `NNN` is a three-digit sequence number. Name the file after the question, not the answer (e.g. `NNN-scenario-format.md`).
- To reverse a decision, rewrite the "Decision" and the reasons in the same file, and add a row to "History". Do not create a new one.
- When the question itself no longer exists, delete the file. Numbers are never reused.
- A Japanese translation sits beside it with the same name and a `.jp.md` extension, using the Japanese headings shown in the translation of this file. Where the two differ, the English file applies.

## Body

```markdown
# NNN: Question

- Status: Proposed | Accepted

## Context
The forces acting on this question and the trade-offs between them.

## Decision
The current choice.

## Options not taken
| Option | Reason |
|---|---|

## Consequences
Constraints this decision creates and risks it accepts.

## History
| Date | Replaces version | Change |
|---|---|---|
| YYYY-MM-DD | — | Decided |
```

## History

Every change to the Decision adds one row to History:

- Date: the day of the change.
- Replaces version: the short hash of the last commit on `main` that contains the previous decision, taken before editing with `git log -1 --format=%h -- <this file>`, written as a link to this file at that commit: `[<hash>](../../../../blob/<hash>/docs/ADR/<this file>)`. GitHub does not turn a bare hash in a repository file into a link, and the relative path keeps the link independent of the repository's name and default branch. A commit cannot contain its own hash, so the row points at the version it replaces; the commit that made the change is the first one after that hash to touch this file: `git log --reverse --format=%h <hash>..main -- <this file> | head -1`.
- Change: one line saying what changed in the decision. The reason belongs in the body and the commit message.

Edits that leave the decision unchanged (wording, links, translation) add no row. Hashes refer to commits on `main`; a history rewrite on `main` would break them, so `main` is never rewritten.

## Rules

- Write the body in the present tense. Do not write "this used to be X" or "changed from Y" outside History.
- When a premise breaks and the decision needs revisiting, set the status back to `Proposed`; set it to `Accepted` once the revision is done.
- Do not use names that only made sense in a conversation or during the work ("option B", "the thing we discussed"); say what is meant.
