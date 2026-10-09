# 02 — Interaction Design

## Principles

1. **Built around the focus session.** The screen always answers one question: "what is related to *this* conversation?" It never turns into a global session browser. `codex resume` already provides that.
2. **Show the tree first, refine it later.** Show what the cache already knows within ~100 ms, then verify candidates in the background and update the tree in place. Don't block on a full scan.
3. **Mark heuristics as heuristics.** Inferred edges look different and can be confirmed or rejected. Nothing inferred is treated as fact without the user's say.
4. **The keyboard covers everything; the mouse is optional.** Bindings follow vim/less habits, with arrow-key equivalents.
5. **Hand off to Codex, don't replace it.** Resume and fork suspend the TUI and run `codex`. When Codex exits, the TUI comes back with the tree refreshed.

## Launch and focus resolution

```
codex-fork-tree [SESSION]            # SESSION = uuid, uuid prefix, thread name, or rollout path
codex-fork-tree --last               # newest session in the current project
codex-fork-tree --project PATH       # override project root
codex-fork-tree index [--rebuild]    # warm/rebuild the project cache, no UI
codex-fork-tree tree SESSION --plain # print the tree once (ASCII, for pipes/scripts)
```

The focus session is resolved in this order. The first match wins:

1. the explicit `SESSION` argument
2. the thread id from the environment when launched from inside Codex (to verify: the env var Codex exports to tool shells)
3. the newest session whose `cwd` is inside the current project
4. otherwise, open the **focus picker**: a fuzzy list of the project's sessions, newest first

An ambiguous uuid prefix or name also opens the focus picker, prefiltered with that text.

## Screens

There is one main screen with two panes, plus overlays. There is no screen stack beyond one overlay at a time.

| Screen | Purpose | Opened by |
|---|---|---|
| **Tree** (main) | Fork tree on the left, details of the selected node on the right | launch |
| **Transcript** overlay | Full-height scrollable transcript of the selected session; the fork point is marked | `Enter` / `t` |
| **Compare** overlay | Two sessions side by side, starting at their fork point | `c` on a second node after `m` (mark) |
| **Focus picker** | Fuzzy-find any session in the project and make it the focus | `f` / `/` with empty tree |
| **Confirm** dialog | Destructive or outward actions | archive, delete |
| **Help** | Keymap reference, generated from the binding table | `?` |

## Tree screen

**Selection model**

- The cursor starts on the focus node, and that row is centered.
- `j`/`k` (or `↓`/`↑`) move through selectable rows: frame tops, `◆` fork points and leaf sessions. Bottom borders are skipped. `h`/`l` (or `←`/`→`) fold or unfold a frame, or jump to its parent or first child. The parent of a branch is its `◆`, and the parent of a `◆` is its frame.
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

## Mouse (optional)

Click to select, double-click to open the transcript, and scroll the wheel inside either pane. There is no drag interaction.
