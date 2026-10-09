# 03 — UI Style

## Tone

The look is quiet: a terminal-native tool that sits comfortably next to Codex's own TUI. It uses the terminal's default background, so it never paints a full-screen color. Color carries meaning (focus, provenance, state) rather than decoration. Without color, the layout must still read correctly: provenance is always shown with a glyph *and* a color.

## Layout

```
┌ header ─────────────────────────────────────────────────────────────────────┐
│ codex-fork-tree · kvstore · focus bf7e5086 "Explain fsync ordering"          │
│ [archived off] [inferred on]                                       ⟳ 4/11   │
├ tree (≈45%) ─────────────────────────┬ details (≈55%) ──────────────────────┤
│                                      │                                      │
│                                      │                                      │
├──────────────────────────────────────┴──────────────────────────────────────┤
│ j/k move  ⏎ open  r resume  n fork  f focus  ? help             indexed 3s  │
└─────────────────────────────────────────────────────────────────────────────┘
```

The UI runs full-screen in the terminal (alternate screen, restored on exit). **Size is never configured.** The layout is recomputed from the current terminal size at startup and on every resize (SIGWINCH). The breakpoints below only pick how the panes are arranged; everything inside them scales continuously.

| Terminal | Arrangement |
|---|---|
| ≥ 100 columns | side-by-side panes, tree 60% and details 40%, adjustable with `<`/`>` |
| 60–99 columns | stacked: tree on top, details below (40% of the body, at most 11 rows) |
| 40–59 columns | tree only; `Enter` opens details as an overlay |
| < 20 rows | one-line header: no filter chips, no anchor line |
| < 40 × 12 | centered notice: `terminal too small · 38×10 · needs 40×12` |

What scales with width:

- Titles and prompts truncate with `…`. The right-hand metadata (`+N · age`) stays and the title gives way.
- Each nesting level costs 2 columns; titles and fork info truncate first, the `+N · age` column never moves.
- The selected row stays in view after a resize, and the tree scrolls to keep it on screen.

## Tree layout: outline with fork info

Earlier rounds used framed boxes; they were dropped because boxes kept the right-hand metadata from lining up and made a title look more indented than its own contents. The tree is now an outline:

```
▣ Design the storage engine                                   +38 ·  1w
├─○ Earlier attempt: direct port                              +84 ·  1w
│     turn 3 · “List what the engine needs from the file format.”
│
└─▣ Implement WAL replay                                      +55 ·  7d
  │   turn 9 · “Agreed on the format. Now plan the WAL work in stages.”
  ├─▣ Rewrite compaction                                      +40 ·  7d
  │ │   turn 11 · “Stage 1 passes. Before stage 2, review the compacti…
  │ └─○ Compaction: try leveled                               +14 ·  6d
  │       turn 20 · “Merging is still too slow. Can we bound it?”
  │
  ├─○ Benchmark write stalls                                  +16 ·  6d
  │     turn 14 · “Run the crash-recovery tests and report what fails.”
  │
  └─● Explain fsync ordering                                  +36 ·  6d
    │   turn 14 · “Run the crash-recovery tests and report what fails.”
    └─○ fsync: batch alternative                               +4 · 21m
          turn 19 · “So the directory entry is not synced?”
```

- **One row per session**, indented 2 columns per level under the session it was forked from. Every row spans the full pane, so `+N · age` sits in one right-hand column.
- **Title first, then fork info.** Under each forked session, an indented line says where it split off: `turn N` (fork color) and the last shared prompt (muted). The root has no fork line; the header's `anchor ⌂` shows its first prompt.
- **Focus without frames:** the focus row gets `●` and an accent title, and the guide lines from the root down to it are drawn in the accent color.
- **Density:** comfortable (default) adds a guide-only row between siblings; `=` or `tree --compact` removes them.
- **Folding:** `h` folds a session's forks into `▸ +N`; sessions with forks that are off the focus path start folded.
- Fork info lines and spacer rows are not selectable; the cursor moves over sessions only.

### Symbols

The same table is shown in the app under `?` (keys & symbols).

| Sample | Meaning |
|---|---|
| `●` + accent title | focus: the conversation this tree is about |
| accent `│ ├─` | the path from the root to the focus |
| `○` title | a session |
| `▣` + bold green title | a session that has forks of its own |
| `◉` | live: Codex is writing to it right now |
| `◌` | archived (shown with `a`) |
| `├─` `└─` | fork link recorded by Codex (exact) |
| `├┄` + `≈` | inferred fork: copied history, no metadata (`x` confirm · `X` reject) |
| `turn 9 · “…”` | forked after turn 9; the quote is the last shared prompt |
| `+55 · 7d` | own turns after the fork point · age |
| `▸ +3` | folded: 3 sessions hidden below |

## Details pane

```
Explain fsync ordering                                               ● focus
bf7e5086-9daf-7076-91e8-afc0e26d7e87                         cli · 0.161.0
created 2026-10-03 03:06 · 6d ago · 50 turns

FORK
  from   7b3a1c42 Implement WAL replay                 native
  after  turn 14  "Run the crash-recovery tests and report what fails."
  shared 14 turns · own 36 turns
  children 1 · siblings 2

LAST EXCHANGE
  you    so the fix is one extra fsync on the directory per rotation?
  codex  Yes, and it only runs when the WAL segment rotates.

r resume   n fork   c compare   y copy id
```

- Section headings use small caps (an uppercase label in a dim color). There are no boxes inside panes.
- Quoted user text is truncated to one line with `…`. CJK width is measured with display width, not character count.
- Inferred forks add a line: `match  copied history · 14 turns · x confirm  X reject`.

## Color tokens

The palette is defined as semantic tokens. On truecolor terminals the default theme is **Catppuccin**: **Mocha** on dark backgrounds and **Latte** on light ones. The dark or light choice is detected by querying the terminal background with OSC 11, and `--theme mocha|latte|ansi` overrides it. On terminals without truecolor, and when detection fails, the tokens fall back to the 16 ANSI colors so the UI follows the user's terminal theme.

Earlier rounds tried Catppuccin's pastel accents. They were too cold (Blue), and then too similar (Peach, Yellow, Rosewater, Pink and Maroon all sit within about 60° of each other and share the same pastel lightness). The current scheme keeps Catppuccin's **Base and neutral grays**, which give the theme its warm-dark (Mocha) and soft-light (Latte) feel, and uses **saturated hues placed far apart on the color wheel** for the roles.

Rules:
- Each tree role has one hue. The five tree hues are 50–100° apart: orange 30°, green 95°, teal 178°, violet 270°, pink 330°.
- Yellow and red appear only in the status bar (`warn`) and the delete dialog (`danger`), never in the tree.
- No blue anywhere.
- The light theme uses the same hues, deepened so text stays at about 4.5:1 or better on Latte Base.

| Token | Hue | Dark (on Mocha `#1e1e2e`) | Light (on Latte `#eff1f5`) | ANSI-16 fallback | Used for |
|---|---|---|---|---|---|
| `fg` | — | `#cdd6f4` (Text) | `#4c4f69` (Text) | default | body text, forked-session titles `○` |
| `dim` | — | `#7f849c` (Overlay1) | `#7c7f93` (Overlay2) | bright black | metadata, guides, thin borders, turn labels |
| `accent` | orange 30° | `#ff9e44` | `#d9480f` | bright red | focus `●` and title, the path to the focus, key hints |
| `own` | green 95° | `#9be05c` | `#2b8a3e` | green | `▣` and bold title of sessions that have forks |
| `live` | teal 178° | `#2ed9cf` | `#0b7f8c` | cyan | `◉`, newly verified sessions |
| `fork` | violet 270° | `#c38bff` | `#7c3aed` | magenta | `turn N` in fork info lines, the transcript's \"forked after\" rule |
| `subtle` | — | `#a6adc8` (Subtext0) | `#6c6f85` (Subtext0) | gray | quoted prompts in fork info lines |
| `inferred` | pink 330° | `#ff6ab8` | `#c2255c` | bright magenta | dashed connectors, `≈` labels |
| `warn` | yellow 48° | `#ffd43b` | `#a66a00` | yellow | spinner, errors, stale cache (status bar only) |
| `danger` | red 0° | `#ff5555` | `#e03131` | red | delete confirmation only |
| `sel` | — | `#313244` (Surface0) | `#dce0e8` (Crust) | reverse video | selection bar |
| `frame` | — | `#45475a` (Surface1) | `#bcc0cc` (Surface1) | bright black | outer chrome, pane dividers |

Session names are the brightest text: bold green for sessions with forks, accent orange for the focus, plain text otherwise. Fork info is quieter: `turn N` in violet, the quoted prompt in the muted `subtle` color.

On the 16-color fallback, `fork` and `inferred` are both magenta (normal vs. bright). They stay apart by shape: `◆` and quoted text for fork points, dashed `┄`/`┆` for inferred edges.

- **Selection:** a tinted bar (`sel` token) across the full row width in the tree pane, so edge and provenance colors stay readable. On 16-color terminals, use reverse video instead. When the cursor is on the focus node, the `●` stays accent-colored inside the bar.
- `NO_COLOR` is respected: glyphs, bold and reverse video remain.
- Bold is used only for the focus title and the selected row title. Italics are never used, because terminal support varies.

## Motion

- No animations, apart from a 1-character spinner `⠋⠙⠹…` in the status bar while background work runs.
- Newly verified nodes show a `+` in the gutter for ~2 seconds. That is the only transient highlight.

## Demos

The clickable mockups that compare the style variants are described in [05-demos.md](05-demos.md).
