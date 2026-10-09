# codex-fork-tree

A terminal UI for browsing a Codex conversation's **fork tree**: the conversation you are in, plus every session that shares a nonempty history prefix with it. That includes forks created before this tool was installed. Subagent sessions are ignored.

```
 codex-fork-tree · kvstore · focus bf7e5086 Explain fsync ordering                             7 shown · 8 in project
 [archived off] [inferred on] [empty off]                                                            indexed just now
 anchor ⌂ “We have a log-structured key-value store prototype; the storage engine is half done and the WAL is missing…
──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
╭─ ▣ Design the storage engine ───────────────────────── +38 ·  1w ─╮ │ Explain fsync ordering                ▣ focus
│ ◆ turn 3 “List what the engine needs from the file form… +3 turns │ │ bf7e5086-9daf-7076-91e8-afc0e2… cli · 0.161.0
│ ├─○ Earlier attempt: direct port                        +84 ·  1w │ │ created 2026-10-03 03:06 · 6d ago · 50 turns
│ ◆ turn 9 “Agreed on the format. Now plan the WAL work i… +6 turns │ │
│ └─╭─ ▣ Implement WAL replay ──────────────────────── +55 ·  7d ─╮ │ │ FORK
│   │ ◆ turn 11 “Stage 1 passes. Before stage 2, review… +2 turns │ │ │   from   7b3a1c42 Implement WAL replay native
│   │ ├─○ Rewrite compaction ▸ +1                       +40 ·  7d │ │ │   after  turn 14  “Run the crash-recovery te…
│   │ ◆ turn 14 “Run the crash-recovery tests and repor… +3 turns │ │ │   shared 14 turns · own 36 turns
│   │ ├─○ Benchmark write stalls                        +16 ·  6d │ │ │   children 1 · siblings 2
│   │ └─┏━ ▣ Explain fsync ordering ━━━━━━━━━━━━━━━━ +36 ·  6d ━┓ │ │ │
│   │   ┃ ◆ turn 19 “So the directory entry is not sy… +5 turns ┃ │ │ │ FIRST OWN PROMPT
│   │   ┃ └─○ fsync: batch alternative                 +4 · now ┃ │ │ │   you    Explain in detail why fsync ordering
│   │   ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ends at turn 50 ━┛ │ │ │          loses the last write after a crash.
│   ╰─────────────────────────────────────────── ends at turn 64 ─╯ │ │
╰───────────────────────────────────────────────── ends at turn 38 ─╯ │ LAST EXCHANGE
                                                                      │   you    so the fix is one extra fsync on th…
                                                                      │   codex  Yes, and it only runs when the WAL
                                                                      │          segment rotates.
                                                                      │
                                                                      │ r resume  n fork  t transcript  f focus  y c…
──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
 j/k move  ⏎ open  r resume  n fork  f focus  ? help
```

(Example data: a made-up key-value store project.)

Each frame is a session that has forks (`▣`). Inside it, `◆ turn N` rows are the points where history split, labeled with the last shared prompt, and the branches hang off them. `●` marks the conversation in focus.

## Install

```sh
cargo install --path .
```

Requires Rust 1.85+. The binary reads `~/.codex` (or `$CODEX_HOME`) and calls the `codex` CLI for resume, fork, archive and delete (`$CODEX_BIN` overrides which binary).

## Use

```sh
codex-fork-tree                    # newest session in the current project
codex-fork-tree bf7e5086           # by id prefix, full uuid, thread name, or rollout path
codex-fork-tree --theme latte      # auto | mocha | latte | ansi
codex-fork-tree tree bf7e5086 --width 100   # print the tree once, no colors
codex-fork-tree index --rebuild    # rebuild the project cache
```

| Keys | |
|---|---|
| `j` `k` / arrows, `gg` `G`, `[` `]`, `p`, `F` | move · first/last · siblings · parent · back to focus |
| `h` `l` | fold or unfold a frame, or go to parent / first child |
| `Enter` / `t` | transcript of the selected session (narrow terminals: details first) |
| `r` / `n` | resume / fork in Codex (the TUI suspends and comes back refreshed) |
| `f` | make the selected session the focus |
| `a` `i` `e` | show archived · inferred · empty sessions |
| `x` / `X` | confirm / reject an inferred fork (`≈`) |
| `A` / `D` | archive (asks `y`) / delete (type the first 4 characters of the id) |
| `y` / `Y` | copy id / rollout path (OSC 52) |
| `z` `Z`, `<` `>` | fold off-path frames, expand all · resize the tree pane |
| `Ctrl-r`, `?`, `q` | re-index now · help · quit |

The layout follows the terminal size: side-by-side panes from 100 columns, stacked from 60, tree only from 40, and a notice below 40×12.

## How forks are found

Fork links come from Codex's own metadata (`forked_from_id`, with the cut taken from the ordinal sequence), from forks made through this tool, and, for very old Codex versions, from history copied in one burst at creation. Sessions that merely start with the same prompt are never treated as forks. Full details and validation on real data are in [`docs/06-algorithms.md`](docs/06-algorithms.md). [`prototype/forktree.py`](prototype/forktree.py) is the Python reference implementation; `codex-fork-tree debug-edges --project /` produces the same edge list.

## Files

| Path | What |
|---|---|
| `$XDG_CACHE_HOME/codex-fork-tree/<hash>.json` | per-project cache of parsed rollouts; safe to delete |
| `$XDG_DATA_HOME/codex-fork-tree/ledger.json` | forks created through the TUI and your confirm/reject verdicts |

Rollout files are never written; changes to Codex state go through the `codex` CLI.
