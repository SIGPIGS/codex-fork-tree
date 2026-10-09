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
- Frames nest until a nested frame would be narrower than 30 columns. From there it renders folded, as a leaf row with `▸ +N`.
- The selected row stays in view after a resize, and the tree scrolls to keep it on screen.

## Tree layout: framed trie (default)

The turn trie is the base, and session outlines are drawn on top of it. **Every session that has visible forks is drawn as a frame.** The frame groups all the branches that share it as their parent.

```
╭─ ▣ Design the storage engine ─────────────────────────── +38 ·  1w ─╮
│ ◆ turn 3 “List what the engine needs from the file format… +3 turns │
│ ├─○ Earlier attempt: direct port                          +84 ·  1w │
│ ◆ turn 9 “Agreed on the format. Now plan the WAL work in … +6 turns │
│ └─╭─ ▣ Implement WAL replay ────────────────────────── +55 ·  7d ─╮ │
│   │ ◆ turn 11 “Stage 1 passes. Before stage 2, review t… +2 turns │ │
│   │ ├─╭─ ▣ Rewrite compaction ────────────────────── +40 ·  7d ─╮ │ │
│   │ │ │ ◆ turn 20 “Merging is still too slow. Can we … +9 turns │ │ │
│   │ │ │ └─○ Compaction: try leveled                   +14 ·  6d │ │ │
│   │ │ ╰─────────────────────────────────────── ends at turn 51 ─╯ │ │
│   │ ◆ turn 14 “Run the crash-recovery tests and report … +3 turns │ │
│   │ ├─○ Benchmark write stalls                          +16 ·  6d │ │
│   │ └─┏━ ▣ Explain fsync ordering ━━━━━━━━━━━━━━━━━━ +36 ·  6d ━┓ │ │
│   │   ┃ ◆ turn 19 “So the directory entry is not sync… +5 turns ┃ │ │
│   │   ┃ └─○ fsync: batch alternative                   +4 · now ┃ │ │
│   │   ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ends at turn 50 ━┛ │ │
│   ╰───────────────────────────────────────────── ends at turn 64 ─╯ │
╰─────────────────────────────────────────────────── ends at turn 38 ─╯
```

Anatomy of a frame:

| Part | Content | Selectable |
|---|---|---|
| top border | owner symbol `▣`, session title in the `own` color, `+N` own turns (turns after its fork point), age | yes, this row stands for the session |
| spine `◆` rows | each fork point in turn order: `turn N`, the last shared prompt, and `+N turns` since the previous fork point | yes, details show the fork point and its branches |
| branch rows | `├─`/`└─` off the spine; a leaf session is one row, and a session with forks of its own is a nested frame | yes |
| bottom border | `ends at turn N`: where the session's own history stops | no |

Rules:

- A leaf session (no visible forks) is a plain row, never a one-row frame.
- **The frame's own session and its forked sessions never look alike.** The owner's title row starts with `▣`, and both the symbol and the title use the `own` color (green). Forked sessions start with `○` (or `●` `◉` `◌` `↳`) in the normal text color. When the focus session owns a frame, its `▣` is accent-colored and its title is bold.
- Turns are always spelled out: `turn 14`, `ends at turn 38`, `forked after turn 19`. The bare `t14` form is not used.
- Borders show which session they belong to: **heavy and accent-colored** (`┏━┓┃┗┛`) for the focus session, **dashed and inferred-colored** (`╭┄╮┆╰┄╯`) when the frame's own edge to its parent is inferred, and **thin and dim** (`╭─╮│╰─╯`) otherwise.
- Folding (`h`) collapses a frame to a single leaf row with `▸ +N`. Frames that are not on the path to the focus session start folded.
- Each nesting level costs 6 columns (2 for the branch connector, 4 for the frame's borders and padding) and 2 rows. A nested frame narrower than 30 columns is rendered folded (see [Layout](#layout)).
- Leave one column of space between the outer frame and the pane divider.

Layouts A (outline) and C (plain turn trie) stay in the demo as references. A is a candidate fallback for very deep trees.

## Tree rows (outline layout A)

Each row has one session per line and up to four fields, aligned in columns:

```
<guides><marker> <title…>                          <fork label>  <age>
```

```
⌂ "We have a log-structured key-value store…"
├─○ Implement WAL replay                             ·  7d
│ ├─● Explain fsync ordering             ⑂ turn 14   ·  6d   ← focus
│ │ └─○ fsync: batch alternative         ⑂ turn 19   · now
│ └┄○ Rewrite compaction                 ≈ turn 11   ·  7d
└┄○ Earlier attempt (pre-install)        ≈ turn 3    ·  1w
```

| Element | Glyph | Meaning |
|---|---|---|
| frame owner | `▣` (`own` green; `accent` orange when it is the focus) | the session a frame belongs to; only on frame top borders |
| focus node | `●` (bold, accent) | the current conversation, when it is a leaf row |
| other session | `○` | |
| live session | `◉` (pulsing is optional; static is fine) | Codex is writing to it now |
| archived | `◌` (dim) | only visible with `a` |
| common-prefix root | `⌂` | virtual node; its label is the shared first prompt |
| error | `!` (warning color) | unreadable rollout |

### Edges

| Provenance | Connector | Fork label | Color |
|---|---|---|---|
| native / recorded | `├─` `└─` solid | `⑂ turn N` | normal |
| inferred | `├┄` `└┄` dashed | `≈ turn N` | dim + hint color |
| inferred, confirmed by the user | solid | `⑂ turn N ✓` | normal |

ASCII fallback (`--ascii`, or detected when the locale isn't UTF-8): `|-`, `` `- ``, `:-` for inferred edges, `*` focus, `o` other, `~` for the inferred label.

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
| `accent` | orange 30° | `#ff9e44` | `#d9480f` | bright red | focus `●`/`▣`, focus frame border, key hints |
| `own` | green 95° | `#9be05c` | `#2b8a3e` | green | frame owner `▣` and title |
| `live` | teal 178° | `#2ed9cf` | `#0b7f8c` | cyan | `◉`, newly verified sessions |
| `fork` | violet 270° | `#c38bff` | `#7c3aed` | magenta | `◆` fork points, quoted prompts, the transcript's "forked after" rule |
| `inferred` | pink 330° | `#ff6ab8` | `#c2255c` | bright magenta | dashed edges and frames, `≈` labels |
| `warn` | yellow 48° | `#ffd43b` | `#a66a00` | yellow | spinner, errors, stale cache (status bar only) |
| `danger` | red 0° | `#ff5555` | `#e03131` | red | delete confirmation only |
| `sel` | — | `#313244` (Surface0) | `#dce0e8` (Crust) | reverse video | selection bar |
| `frame` | — | `#45475a` (Surface1) | `#bcc0cc` (Surface1) | bright black | outer chrome, pane dividers |

The fork point `◆` and its prompt share the violet hue, so a fork point reads as one unit and never looks like a session title. Session titles are green (frame owner), orange (focus) or plain text (forked session).

On the 16-color fallback, `fork` and `inferred` are both magenta (normal vs. bright). They stay apart by shape: `◆` and quoted text for fork points, dashed `┄`/`┆` for inferred edges.

- **Selection:** a tinted bar (`sel` token) across the full row width in the tree pane, so edge and provenance colors stay readable. On 16-color terminals, use reverse video instead. When the cursor is on the focus node, the `●` stays accent-colored inside the bar.
- `NO_COLOR` is respected: glyphs, bold and reverse video remain.
- Bold is used only for the focus title and the selected row title. Italics are never used, because terminal support varies.

## Motion

- No animations, apart from a 1-character spinner `⠋⠙⠹…` in the status bar while background work runs.
- Newly verified nodes show a `+` in the gutter for ~2 seconds. That is the only transient highlight.

## Demos

The clickable mockups that compare the style variants are described in [05-demos.md](05-demos.md).
