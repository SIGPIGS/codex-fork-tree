# 06 — Algorithms

This document covers how codex-fork-tree turns rollout files into the fork tree on screen. Every rule here was checked against the 317 real rollouts in `~/.codex` (745 MB, Codex 0.104 to 0.161, checked 2026-10-09). `prototype/forktree.py` is a stdlib-only reference implementation of the whole pipeline; see [Validation](#12-validation-on-real-data).

```
rollout files ──parse──▶ turn logs ──fingerprint──▶ prefix index ─┐
session_meta ───────────▶ fork metadata ──────────────────────────┼─▶ edges ─▶ tree(focus) ─▶ framed rows ─▶ screen
session_index / state db ▶ names, archived ───────────────────────┘     ▲
ledger (recorded forks, user verdicts) ─────────────────────────────────┘
```

## 1. Inputs

| Source | What we take | Notes |
|---|---|---|
| `sessions/YYYY/MM/DD/rollout-*.jsonl` (and `archived_sessions/` if present) | `session_meta` (line 1), the turn log, ordinals | append-only JSONL; the last line can be torn while Codex is writing |
| `state_5.sqlite:threads` (read-only) | `rollout_path`, `archived`, `title`, `cwd` | the authority for where a rollout lives if files move |
| `session_index.jsonl` | `thread_name` (last line wins) | display titles |
| our ledger | recorded forks, confirm/reject verdicts | the only thing we write |

One session id maps to exactly one file. A file can contain several `session_meta` lines with the **same** id; these mark resumes, not forks.

## 2. Turn log extraction

A **turn** is one real user message plus the agent messages that answer it. We extract only the user-visible conversation and ignore tool calls, reasoning, token counts and context records.

Three record formats exist. Each file is parsed with the richest format it contains:

| Format | Records | Codex versions | Files here |
|---|---|---|---|
| **F2** | `event_msg/item_completed` with `item.type` = `UserMessage` (has a stable `id`) / `AgentMessage` | 0.147 → current | 58 |
| **F1** | `event_msg/user_message`, `event_msg/agent_message` | 0.104 → 0.147 | 185 |
| **F3** | raw `response_item` `message`, role `user` / `assistant`, injected context removed | rare | 9 |

Counts are for the 252 user sessions; the 65 subagent rollouts are skipped (§5.4).

Only F3 needs filtering. User messages whose text starts with `# AGENTS.md`, `<environment_context`, `<user_instructions`, `<permissions`, `<collaboration_mode` or `<turn_aborted` are injected context, not turns. F1 and F2 events already carry only what the user typed.

State machine per file:

```
for each line (prefiltered by substring before JSON parsing):
    session_meta           → keep the first one
    user turn record       → push Turn(text, ts, ordinal, uid)
    agent message record   → append text to the last turn
    thread_rolled_back(n)  → pop the last n turns
    compacted              → ignore (it rewrites model context, not the user-visible turns)
    torn / invalid JSON    → stop; do not advance the stored offset past this line
```

**Prefilter.** Only about 13k of the millions of lines matter. Before JSON-parsing a line, test for `"user_message"`, `"agent_message"`, `"UserMessage"`, `"AgentMessage"`, `"message"`, `"session_meta"` or `"thread_rolled_back"` within its first 400 bytes. A cold scan of all 745 MB then takes about 1.8 s in Python. Rust should be well under that.

## 3. Fingerprints and the prefix index

```
fp(turn) = H(normalize_ws(user_text) ‖ 0x00 ‖ join(0x01, normalize_ws(agent_texts)))
P_k(s)   = H(P_{k-1}(s) ‖ fp(turn_k))          P_0 = ""       (rolling prefix hash)
index    : (k, P_k) → {sessions}                for every k of every session
```

- `H` is any 64-bit or stronger hash; the prototype truncates SHA-1 to 64 bits. On the order of 10⁴ turns, collisions don't matter. Where it does matter, verification compares fingerprints exactly (§5).
- **The reply is part of the fingerprint.** Of 33 session pairs here with the same first prompt, 27 got different replies: the user re-typed the prompt, which is not a fork. Matching on prompts alone would merge them.
- `index` has one row per turn in the project (a few thousand), so "who shares my first k turns" is a single lookup. The **anchor** of a session is `P_1`.

## 4. Effective history

Since Codex 0.147, forks **don't copy** history. Every record carries an `ordinal`, and the child's ordinals continue the parent's sequence. The fork cut `X` is:

| Codex | Where X comes from | Verified here |
|---|---|---|
| ≥ 0.160 | `session_meta.forked_from_ordinal_exclusive` | parent ordinals 781–1647, child starts at 1648 |
| 0.147–0.15x | no explicit field: **`X` = the ordinal of the child's own `session_meta` record** | 3 of 3 forks: the parent's ordinals end at exactly X − 1 |
| older, or `ordinal` = 0 / missing | no reference: the history was copied into the child (§5.2) | |

```
effective(s) = [t ∈ effective(parent(s)) | t.ordinal < X] ++ own_turns(s)    if s has forked_from_id and X
             = own_turns(s)                                                   otherwise
```

Without the implicit-cut rule, those three 0.147 forks appear to share zero turns with their parents and would be wrongly dropped.

This is memoized, with a cycle guard (a session is marked in progress before recursing). Every later step (fingerprints, LCP, fork points, the rendered `turn N`) uses effective histories.

## 5. Fork evidence → edges

Each session gets at most one parent edge `(parent, k, kind)`. `k` is the **fork point**: the number of turns shared with the parent.

**Precedence:** user verdict > native metadata > recorded ledger entry > inference.

### 5.1 Native (`forked_from_id` in `session_meta`)

`k = LCP(effective(parent), effective(child))`, comparing fingerprints. This one rule covers both cases:
- ordinal-reference forks (≥ 0.160), where `k` equals the inherited length
- copying forks (0.142–0.154), where the history was duplicated into the child

`kind = native`.

### 5.2 Inferred (no metadata): the burst test

Copying forks write the inherited history **in one burst at creation time**. Measured here, the copied turns of three real forks were all written within 0.02–0.07 s, starting 0.07–0.12 s after `session_meta.timestamp`. A coincidental match (two sessions that both started with "hello" and got the same reply) was written 13 s after creation and spread over 4 s.

```
burst(s) = number of leading turns whose ts − created(s) ≤ 2 s
if burst(s) = k ≥ 1:
    cands  = index[(k, P_k(s))] − {s},  filtered to created < created(s),  verified by exact fp comparison
    origin = [c ∈ cands | burst(c) < k]          # c wrote turn k itself instead of copying it
    parent = earliest-created of (origin or cands)
    edge   = (parent, k, inferred)
```

- A session without a burst is never given an inferred parent. Sessions that merely share text are not forks, even if the reply matches.
- **Ambiguity.** If A wrote turns 1–10, B copied A at 10 and C copied B at 10, then C's first 10 turns match both. The rule picks A, the session that produced turn 10 organically. The details pane lists the other matches ("also matches B"), and the user can confirm or reject the edge.
- **If the parent is missing** (deleted), the session hangs under a virtual `⌂` root labeled `parent not found · k shared turns`.

### 5.3 Recorded (ledger)

When the TUI runs `codex fork <id>` itself (§10), it records `(child, parent)` in the ledger. With native metadata in every Codex since about 0.142, the ledger mostly serves as a safety net. **No Codex hook is needed.**

### 5.4 Subagents are excluded

Subagent sessions are provisional and short-lived, so they are out of scope. A rollout whose `session_meta.source` is not a plain string (it is a `{subagent: …}` object) is skipped after reading line 1. It is never parsed, indexed, given edges or displayed, and there is no toggle for it.

- If a user session's `forked_from_id` points at a subagent (rare), the edge is dropped and the session becomes a root.
- If the focus resolved at launch is a subagent (for example, an id pasted from a subagent's log), the TUI follows `forked_from_id` up to the nearest user session, focuses that one, and shows a one-line notice: `03fc… is a subagent; showing its parent session`.

On this machine, 38 of the 47 `forked_from_id` links belong to subagents. Excluding them leaves 9 user forks.

### 5.5 The nonempty rule

An edge with `k = 0` is dropped and the child becomes a root.

## 6. Scope: which sessions are displayed

```
displayed(F) = the tree containing F, built from edges with k ≥ 1
```

This matches the spec's "sessions sharing a nonempty history prefix". Every edge preserves at least turn 1, and fork points are prefixes, so every member of the tree shares turn 1 with F. Conversely, any session that genuinely shares history with F reached it through a chain of fork edges. Text-only coincidences are excluded by design (§5.2).

**Discovery** reads only the cache, never files:
1. `index[(1, P_1(F))]`, the sessions with the same anchor
2. plus the native and recorded parents and children of those sessions, through a reverse map `forked_from_id → children`
3. then closure over edges to get the tree


## 7. Tree → outline rows

```
walk(s, prefix, conn, cont):
    row  prefix ++ conn ++ marker(s) ++ title(s) [++ "▸ +N" if folded]      right: "+own · age"
    if s has a parent edge (p, k):
        info row  prefix ++ cont ++ ("│" if s shows children) ++ "   turn k · “prompt of turn k”"
    for each visible child c (ordered by fork point k, then creation time):
        walk(c, prefix ++ cont, "├─" or "└─" (dashed "├┄" when inferred), "│ " or "  ")
        comfortable density: a spacer row "prefix ++ cont ++ │" between siblings
```

- **Width** is measured in terminal cells, not characters: CJK and other East Asian wide characters count as 2. Ambiguous-width characters (`…`, `“`, `●`) count as 1, as in default terminal settings. Truncation never splits a wide character. Every session row is padded to the pane width minus one, so the `+N · age` column lines up.
- **Focus path:** guides and connectors on the path from the root to the focus use the accent role: a child's connector when it is on the path, and a parent's vertical guide while an on-path child is still below.
- **Row model:** `{key?, parentKey?, session?, cells}`. Only session rows have a key; fork info and spacer rows don't, so navigation (`j`/`k`/`h`/`l`/`[`/`]`) skips them.
- **Stable order:** children sort by fork point `k`, then creation time. New sessions almost always sort last, so they appear without moving existing rows.
- **Viewport anchoring:** after any rebuild (data update or resize), find the selected key's new row index and adjust scroll so it stays visible.
- **Titles:** `thread_name`, otherwise the first own prompt. Forks **inherit their parent's thread name** (seen in real data: three sessions in one tree carried the same name). When siblings or a parent and child share a title, append the first own prompt in `dim` color: `Implement WAL replay · “Start stage 2 with checksums…”`.

## 8. Project cache

`${XDG_CACHE_HOME:-~/.cache}/codex-fork-tree/<sha1(project_root)>.sqlite`, where the project root is the git root of the focus session's `cwd`, or the `cwd` itself.

```sql
files(path PK, session_id, size, mtime_ns, offset, head_hash, format, parsed_at)
sessions(id PK, path, cwd, created, source_kind, forked_from_id, forked_from_ordinal,
         thread_name, archived, n_turns, burst_len, anchor)
turns(session_id, k, fp, prefix_hash, ts, ordinal, uid, user_preview)      -- PK(session_id, k)
CREATE INDEX turns_prefix ON turns(k, prefix_hash);
CREATE INDEX sessions_anchor ON sessions(anchor);
CREATE INDEX sessions_ff ON sessions(forked_from_id);
meta(normalizer_version, codex_home)
```

The ledger is separate (`${XDG_DATA_HOME}/codex-fork-tree/ledger.sqlite`: `edges(child, parent, k, source, created)`, `verdicts(child, parent, verdict, at)`) and survives `index --rebuild`.

**Incremental update for each file:**

| Observation | Action |
|---|---|
| size and mtime unchanged | nothing |
| size grew, `head_hash` (hash of the first 4 KiB) unchanged | parse from `offset`, continuing the state machine: keep the last turn open for more agent messages, and apply rollbacks against the stored turns |
| size shrank or `head_hash` changed | re-parse the file from scratch |
| file missing | look up `threads.rollout_path` by session id (it may have been archived or migrated); otherwise mark the session gone |

`offset` always points just after the last complete newline, so a torn line gets re-read next time. A changed `normalizer_version` (new injected-context prefixes, new formats) triggers a full rebuild.

**Discovering new files:** list today's and yesterday's `sessions/Y/M/D` directories every refresh. Do a full directory walk at startup and every few minutes. To filter by project cheaply, read only line 1 (`session_meta.cwd`) of unknown files.

**Measured cost:** a cold full scan takes about 1.8 s in Python (prefiltered). A warm start does one `stat` per known file, plus parsing only the bytes appended since the last run.

## 9. Live updates

- Watch the active date directories with inotify or FSEvents (the `notify` crate in Rust, `watchfiles` in Python), and debounce by 250 ms. Polling every 2 s is the fallback.
- On change: run the incremental update (§8), rebuild only the focus tree, and apply viewport anchoring (§7). Highlight newly verified sessions for 2 s (`live` color, `+` gutter).
- **Live sessions:** the rollout was modified within the last 5 s, or a lock for it exists in `~/.codex/thread-writer-locks`. A live session gets the `◉` marker, and `r` warns before resuming it.

## 10. Resume and fork handoff

```
r: leave alt-screen → spawn `codex resume <id>` (inherit tty) → wait → re-enter → incremental update → keep cursor on <id>
n: snapshot = set of session ids in the project
   leave alt-screen → spawn `codex fork <id>` → wait → re-enter → incremental update
   new = ids − snapshot;  child = the one whose forked_from_id = <id> (else the newest of `new`)
   ledger.edges += (child, <id>, source=recorded);  focus ← child
```

If no new session appears (the user quit before typing anything), show a one-line notice and keep the focus.

## 11. Edge cases

| Case | Handling |
|---|---|
| Rollback after a fork in the parent | fork points are computed from the turn log with rollbacks applied, so the shared prefix is what both logs contain now; when the parent rolled back past the fork point, `k` shrinks to the remaining common prefix |
| Compaction (`compacted`, `ContextCompaction`) | ignored; it changes model context, not the user-visible turn log |
| Same-id `session_meta` repeated (resume) | first one wins |
| Fork of a fork made by ordinal reference | `effective()` recursion handles chains |
| Cycles or a self-reference in `forked_from_id` | the cycle guard breaks them; the session becomes a root |
| Clock skew between machines (synced `~/.codex`) | the burst test compares timestamps within one file only; parent choice uses `created`, and ties break by session id |
| Very large sessions (5k+ lines) | only turn records are parsed; per-turn work is constant |
| Empty sessions (no user turn) | never part of a tree; shown only when they are the focus (empty state) |
| Fork with no own turns yet (`+0`, created and then abandoned) | a leaf with `+0`; hidden unless it is the focus, or the "show empty" toggle `e` is on |

## 12. Validation on real data

Running `python3 -I prototype/forktree.py` with no argument lists the trees; with a session-id prefix it renders one tree.

| Check | Result |
|---|---|
| Rollouts | 317: 252 user sessions parsed (F1 185, F2 58, F3 9), 65 subagent rollouts skipped at line 1 |
| Cold scan | 1.8 s (Python, prefiltered) |
| User `forked_from_id` links | 9 of 9 attached with k ≥ 1: 1 explicit cut (0.160), 3 implicit cuts (0.147), the rest by LCP |
| Fork trees found | 4, in 3 projects, with 2–5 sessions each |
| Inferred edges | 0. Every real fork here carries native metadata; inference matters only for Codex older than about 0.142 |
| Coincidental matches (same prompt, same reply, no burst) | rejected (the "hello" pair) |
| Ordinal-reference chain, Codex 0.160 (fork of a fork) | rendered two levels deep with correct fork points |
| Implicit-cut forks (Codex 0.147, 3 of them) | attached at the correct fork points |
| Inherited thread names | 3 sessions in one tree shared one name, which motivates the title disambiguation rule (§7) |
| Line widths with mixed CJK and English | every line exactly W cells at 60, 80 and 96 columns |

## 13. Implementation choice

| | Rust: ratatui + crossterm | Python: Textual |
|---|---|---|
| Fit with Codex | same language as Codex, single static binary | needs a Python environment (pipx or uv) |
| Libraries | `serde_json`, `rusqlite`, `notify`, `unicode-width`, `sha1`/`xxhash` | stdlib `json`/`sqlite3`, `watchfiles`, `wcwidth` |
| Alt-screen, resize, suspend for `codex` | explicit; crossterm handles SIGWINCH | built in, with heavier styling machinery |
| Cold-scan speed | best | measured 1.8 s, fine with the cache |
| Iteration speed | slower | fastest; the prototype is already Python |

**Chosen and implemented: Rust with ratatui** (2026-10-09). The tool ships next to Codex as one binary, and the tree renderer is a small amount of custom cell-level drawing, which ratatui handles directly. Keep `prototype/forktree.py` as the **oracle**: generate golden outputs from it on synthetic fixtures (one per format, plus rollback, ordinal chain, burst fork and coincidental match) and check the Rust core against them.
