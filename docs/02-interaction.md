# 02 — Interaction Design

## Principles

1. **Built around the focus session.** The screen always answers one question: "what is related to *this* conversation?" It never turns into a global session browser. `codex resume` already provides that.
2. **Show the tree first, refine it later.** Show what the cache already knows within ~100 ms, then verify candidates in the background and update the tree in place. Don't block on a full scan.
3. **Mark heuristics as heuristics.** Inferred edges look different and can be confirmed or rejected. Nothing inferred is treated as fact without the user's say.
4. **The keyboard covers everything; the mouse is optional.** Bindings follow vim/less habits, with arrow-key equivalents.
5. **Hand off to Codex, don't replace it.** Resume and fork suspend the TUI and run `codex`. When Codex exits, the TUI comes back with the tree refreshed.

## Launch and focus resolution

```
codex-fork-tree                      # project overview: every fork tree in the current project
codex-fork-tree SESSION              # SESSION = uuid, uuid prefix, thread name, or rollout path
codex-fork-tree --last [N]           # only the N most recently active sessions (default 1: open its tree)
codex-fork-tree --project PATH       # override project root
codex-fork-tree list [--all]         # print the project's trees (and standalone sessions)
codex-fork-tree tree SESSION         # print one tree once, no colors
codex-fork-tree index [--rebuild]    # warm/rebuild the project cache, no UI
```

- **With a `SESSION`**, the tool opens that session's tree. That session is the "current conversation".
- **Without one, it opens the project overview.** A project usually has many roots: several fork trees plus many standalone sessions. Guessing "the newest session" often lands on a session without forks, which is a one-node tree. So the tool lets you pick.
- `--last [N]` narrows by recent activity (file modification time, so a resumed old session counts). `--last` alone opens the most recently active session's tree. `--last N` opens the overview limited to the trees that hold one of the N most recently active sessions, and opens the tree directly when they all share one. A header chip shows the limit, and `Esc` widens to the whole project.
- An ambiguous id prefix or thread name is an error that lists the count. (Opening the overview prefiltered with that text is a possible improvement.)
- Reading the thread id from the environment when launched inside Codex is not implemented. No such variable has been confirmed yet.

## Project overview

```
 codex-fork-tree · kvstore · overview                                 2 fork trees · 3 standalone
 [standalone on] [archived off]
──────────────────────────────────────────────────────────────────────────────────────────────────
 FORK TREES  2                                 │  ⏎ opens at c08f6197 fsync: batch alternative
 ▣ Design the storage engine  8 sessions · now │
 ▣ Port the CLI to clap       2 sessions ·  2d │ ╭─ ▣ Design the storage engine ── +38 ·  1w ─╮
                                               │ │ ◆ turn 3 “List what the engine needs… +3 │
 STANDALONE SESSIONS  3                        │ │ …                                         │
 ○ Fix typo in README         1 turn ·  4h     │
```

- **Left list:** every root in the project. Fork trees come first, then standalone sessions, each group ordered by most recent activity anywhere in the tree. A session's `cwd` is shown when it is a subdirectory of the project.
- **Right preview:** the selected tree, focused on its **most recently active session**, which is where `Enter` will open it. The tree scrolls so that session is visible.
- **Keys:** `j`/`k`, `g`/`G`, PageUp/PageDown move. `Enter`/`l` opens the tree. `/` filters by title, prompt or id (space-separated words must all match). `s` hides or shows standalone sessions. `a` shows archived trees. `q` quits.
- **Coming back:** in the tree view, `Esc` (or `o`/`Tab`, or a click on `‹ all trees` in the header) returns to the overview with that tree selected; only `q` quits. `Esc` in the overview goes back to the open tree, or quits when none was opened.
- **Layout:** side by side at ≥ 100 columns, stacked (list above preview) at 60–99, list only below 60.
- **Empty tree:** a tree view of a session without relatives says so and points to `o`.

## Screens

There is one main screen with two panes, plus overlays. There is no screen stack beyond one overlay at a time.

| Screen | Purpose | Opened by |
|---|---|---|
| **Tree** (main) | Fork tree on the left, details of the selected node on the right | launch |
| **Transcript** overlay | Full-height scrollable transcript of the selected session; the fork point is marked | `Enter` / `t` |
| **Compare** overlay | Two sessions side by side, starting at their fork point | `c` on a second node after `m` (mark) |
| **Project overview** | Every fork tree and standalone session in the project, with a live preview; pick one to open | launch without `SESSION`, or `o` / `Tab` |
| **Confirm** dialog | Destructive or outward actions | archive, delete |
| **Help** | Keymap reference, generated from the binding table | `?` |

## Tree screen

**Selection model**

- The cursor starts on the focus node, and that row is centered.
- `j`/`k` (or `↓`/`↑`) move between sessions; fork info lines and spacer rows are skipped. `h`/`l` (or `←`/`→`) fold or unfold a session's forks, or jump to its parent or first child. `=` switches between comfortable and compact spacing, and `?` shows every key and symbol.
- `gg`/`G` go to the first or last node. `p` goes to the parent and `F` goes back to the focus node.
- `[`/`]` move to the previous or next sibling: the alternative branches at the same fork point.
- The details pane follows the cursor. It is debounced, so holding `j` stays smooth.

**Collapsing**

- By default, branches that don't contain the focus node fold to depth 1, so the path to the focus stays visible.
- `z` toggles folding for every branch off the path to the focus node. `Z` expands everything.

**Filters and toggles.** These are visible as chips in the header, and their state persists per project.

| Key | Toggle | Default |
|---|---|---|
| `a` | show archived sessions | off |
| `i` | show inferred edges (off hides the sessions that only inferred edges connect) | on |
| `e` | show empty sessions (no assistant reply after the fork point) | off |

**Actions on the selected node**

| Key | Action | Notes |
|---|---|---|
| `Enter` / `t` | open transcript overlay | |
| `r` | resume in Codex | suspends the TUI and runs `codex resume <id>`; refreshes on return |
| `n` | new fork from this node's tip | runs `codex fork <id>`, records a `recorded` edge, and focuses the new session on return |
| `f` | make this node the focus | rebuilds scope; a root-level re-focus can change which sessions are shown |
| `m` then `c` | mark, then compare with the cursor node | |
| `R` | rename | inline text field in the details pane |
| `A` | archive / unarchive | asks to confirm |
| `D` | delete | typed confirmation (the first 4 characters of the id) |
| `y` | copy session id | `Y` copies the rollout path |
| `x` | on an inferred edge: confirm (`x`) or reject (`X`) | stored in the ledger; a rejected session leaves the tree, and `i` shows it again |
| `/` | incremental search within the tree (title, first prompt, id) | `n`/`N` jump between matches while searching |
| `Ctrl-r` | re-verify candidates now | |
| `q` / `Esc` | close the overlay, or quit from the main screen | |

When Codex is running a session (the rollout was written in the last few seconds, or its writer lock is held), that session is marked **live**. `r` then warns instead of resuming it a second time.

## Transcript overlay

- It shows normalized history only. Injected context collapses to one dim line: `▸ 3 context items hidden`. Press `.` to expand it.
- The fork point is a full-width rule: `── forked here · 3 sibling branches ──`. The text before the rule is dimmed when the overlay was opened from a non-root node, because that part is shared history.
- Tool calls collapse to one line by default. `o` toggles the item under the cursor and `O` toggles all of them.
- Movement: `j`/`k`, `Ctrl-d`/`Ctrl-u`, `gg`/`G`, `{`/`}` (previous or next user turn).
- `r` and `n` work here too.

## Compare overlay

- Two columns starting at the shared fork point, with the shared prefix summarized as one line: `12 shared turns`.
- Rows align by user turn. Turns that exist on only one side show a blank on the other side.
- `Tab` swaps which column scrolls. By default the columns scroll together.

## Background work and feedback

| State | What the user sees |
|---|---|
| Cache warm, nothing to verify | The tree shows immediately; the status bar reads `indexed 3s ago` |
| New or changed rollouts in the project | The tree shows from the cache, the status bar shows `⟳ verifying 4/11`, and verified nodes appear one by one with no cursor jump |
| First run in a project (cold cache) | The tree shows the focus node alone, with a progress line `indexing project · 120/317 rollouts`; relatives appear as they are found |
| Rollout unreadable or truncated | The node appears with a `!` marker; the details pane shows the error; the rest of the tree is unaffected |
| Focus session has no relatives | An empty state under the focus node: `No forks share history with this conversation.` with the hints `n fork it · f pick another` |

Rules:
- The cursor never moves on its own. Inserted rows go above or below it, and the view keeps the selected row in place.
- The tree never removes the node under the cursor. If verification rejects it, the node fades out after the cursor leaves it.

## Resume and fork handoff

```
[Tree] --r--> suspend TUI, restore terminal --> codex resume <id> --(exit)--> resume TUI
                                                                     └─> rescan focus project, keep cursor on <id>
[Tree] --n--> snapshot session ids --> codex fork <id> --(exit)--> diff ids → new session N
                                                              └─> ledger: recorded(N ← id), focus N
```

If the new session can't be determined after a fork (for example, the user quit Codex before sending anything), show a one-line notice and keep the old focus.

## Mouse

The mouse is always captured. Most terminals still allow native text selection with Shift held.

| Where | Wheel | Click | Double-click |
|---|---|---|---|
| Tree | moves the selection one session per notch; the view follows | selects the session (its fork-info line counts too); `‹ all trees` in the header goes back to the overview | makes it the focus (`f`) |
| Overview list | moves the selection; the preview follows | selects the tree | opens it (`Enter`) |
| Transcript, keys & symbols | scrolls | — | — |
| Narrow details overlay | — | closes it | — |

While `codex` runs (resume or fork), the mouse is handed back to the terminal. It is captured again when the TUI returns. There is no drag interaction.
