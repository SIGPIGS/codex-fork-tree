# 05 — UI Demos

`demos/fork-tree-demo.html` is a clickable mock terminal that runs the interaction model from [02-interaction.md](02-interaction.md) on invented example sessions. It shows three candidate layouts for the tree pane. The details pane, transcript overlay, header and footer are the same in all three.

Published copy (private to the owner): https://claude.ai/artifact/Wa73qdMHBjMVrRWwJzh8tP

## Chosen direction: E · Outline with fork info

Later feedback: frames felt tight, prompts outshone session names, and a frame's title looked more indented than its own contents. Layout E drops the frames (see [03-ui-style.md](03-ui-style.md#tree-layout-outline-with-fork-info)): one row per session, the fork point on an indented line under each forked session, `+N · age` in one aligned column, and the focus shown by an accent marker, title and path. D (framed) is kept in the demo for comparison (`v`). Comfortable/compact density is toggled with `=`. E is what the Rust TUI implements.

Earlier, D replaced A–C: it combined the turn trie with the outline in framed boxes.

## First-round layouts

| | A · Outline | B · Timeline | C · Turn trie |
|---|---|---|---|
| Node | session | session, drawn as a bar | fork point (◆) or session end (○) |
| Fork position | `⑂ t14` label | column on a square-root turn axis | ◆ row with the last shared prompt |
| Branch length | not shown | bar length | `+N` own turns |
| Minimum width | 56 | 88 | 56 |
| Best at | scanning and selecting | seeing where branches split and how far they went | "what did I ask differently?" |

Suggested: **A as the default, with `v` switching to C.** B stays a prototype unless it turns out to be useful in practice.

## What the demo also covers

- Responsive modes: 120 columns (side by side), 88 (stacked), 56 (tree only, with details as an overlay)
- Dark and light terminal palettes, and ASCII fallback glyphs
- Progressive verification: two inferred forks appear after the spinner finishes, with a `+` gutter
- Toggles (`s`, `a`, `i`), confirming and rejecting inferred edges (`x`/`X`), re-focusing (`f`), folding (`h`/`l`/`z`/`Z`)
- Resume and fork are toasts only (`would run: codex resume …`)

The compare overlay, rename, archive, delete and search are not in the demo.

## Open questions

1. Default layout, and whether `v` cycles through all three layouts or switches between two.
2. Selection bar: a tinted row (the demo) or reverse video (the 16-color fallback).
3. Show inferred forks by default, or keep them hidden until confirmed.
4. Default visibility of archived sessions. (Subagents were dropped from scope.)
5. At narrow widths, `Enter` opens details first, or the transcript directly.
