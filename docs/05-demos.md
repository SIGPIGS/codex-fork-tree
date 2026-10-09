# 05 — UI Demos

`demos/fork-tree-demo.html` is a clickable mock terminal that runs the interaction model from [02-interaction.md](02-interaction.md) on invented example sessions. It shows three candidate layouts for the tree pane. The details pane, transcript overlay, header and footer are the same in all three.

Published copy (private to the owner): https://claude.ai/artifact/Wa73qdMHBjMVrRWwJzh8tP

## Chosen direction: D · Framed trie

Feedback on the first round: keep Unicode glyphs, and combine the turn trie with the outline. Layout D draws each session that has forks as a frame titled with its name. Inside the frame, the session's fork points run down a spine and its branches hang off them. See [03-ui-style.md](03-ui-style.md#tree-layout-framed-trie-default). D is the demo's default; A and C are kept for comparison, and B was dropped from the page.

The demo terminal resizes like a real one: drag its corner, or use the size presets (fit, 120×30, 88×30, 56×24). Columns and rows are measured from the box, and the layout re-flows on every change. That matches how the real TUI reacts to SIGWINCH.

Open questions for D:

1. Full boxes or a top label with a left rail (saves the right margin and the bottom row)
2. Whether off-path frames start folded
3. Bottom border shows the end turn, or the session's last prompt

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
