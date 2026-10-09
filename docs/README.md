# codex-fork-tree — Design Docs

A terminal UI for browsing the related Codex conversations of a session as a fork tree. It includes forks created before this package was installed. A project-wide cache finds candidate sessions. The tree itself shows only the current conversation and the sessions that share a nonempty history prefix with it.

| Doc | Covers |
|---|---|
| [01-concepts.md](01-concepts.md) | Terms, the scope rule, fork provenance (native / recorded / inferred), turns and fingerprints, tree construction, what "manage" includes |
| [02-interaction.md](02-interaction.md) | Launch and focus resolution, screens, keymap, background verification, resume/fork handoff |
| [03-ui-style.md](03-ui-style.md) | Layout and breakpoints, row anatomy, glyphs, edge styles, details pane, color tokens |
| [04-cache.md](04-cache.md) | Project cache and ledger, as far as they affect the UX |
| [05-demos.md](05-demos.md) | Clickable layout demos (`../demos/fork-tree-demo.html`) and open decisions |
| [06-algorithms.md](06-algorithms.md) | Turn extraction, fingerprints, fork evidence and edges, scope, framed-trie layout, cache, live updates; validated by `../prototype/forktree.py` |

Status, 2026-10-09: **v1 is implemented** in Rust with ratatui (`../src`, see the top-level README). The Rust core produces the same fork edges as the Python prototype on all 317 local rollouts.

Where v1 differs from these docs:
- The cache and ledger are JSON files, not SQLite. Each rollout is re-parsed whole when its size or mtime changes, instead of resuming from a byte offset. A cold project index takes about 0.4 s, and a warm start about 50 ms.
- Refreshes poll every 2 s; there is no inotify watcher.
- `--theme auto` uses `COLORTERM` and `COLORFGBG` instead of an OSC 11 query.
- Not yet built: rename (`R`), search (`/`), the compare overlay (`m`/`c`), mouse support, and the `+` gutter for new sessions (they are tinted in the `live` color instead).
