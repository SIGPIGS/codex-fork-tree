# 04 — Project Cache (UX-relevant parts)

The cache is what makes the tree show up instantly. This document covers only what affects the user experience. Storage internals belong in the implementation plan.

## Location and scope

- `${XDG_CACHE_HOME:-~/.cache}/codex-fork-tree/<sha1(project_root)>.sqlite`
- The cache is project-wide. It records every rollout whose `cwd` is inside the project root, including archived sessions, so that the archived toggle never needs a rescan. Subagent rollouts are skipped after reading line 1 (`session_meta.source` is an object), so they are never parsed.
- The **ledger** (recorded edges plus the user's confirm/reject verdicts) is stored separately at `${XDG_DATA_HOME:-~/.local/share}/codex-fork-tree/ledger.sqlite`. It is user data and must survive `index --rebuild`.

## What is cached per rollout

| Field | Why |
|---|---|
| path, size, mtime, last byte offset parsed | incremental re-parse; rollouts are append-only |
| id, cwd, created/updated, source, archived, title, model | rendering rows without opening files |
| `forked_from_id`, `forked_from_ordinal_exclusive` | native edges |
| **anchor key** = hash of the first complete turn (prompt + reply) | candidate discovery: everything in the displayed tree shares it |
| **turn fingerprints** = rolling hash after each real user turn | O(turns) prefix length between any two sessions without reading the files |

## Discovery → verification

1. **Discover (cheap, synchronous):** look up the candidates whose anchor key matches the focus's anchor, then add native and recorded edges from the ledger.
2. **Render** from the cached fingerprints.
3. **Verify (async):** for candidates whose file changed since it was cached (size or mtime), re-parse from the stored offset and recompute the fingerprints. Insert, move or remove nodes as results arrive.
4. **Rescan the project (async, throttled):** list `~/.codex/sessions` to find new rollouts. Reading only `session_meta` (line 1) is enough to filter by `cwd` before doing a full parse.

## Invalidation

- A new Codex version changes injected-context formats → the normalization rules carry a `normalizer_version`. When it changes, all fingerprints are recomputed on the next run, and the status bar shows `re-indexing (rules updated)`.
- Rollouts can move when Codex migrates storage (`codex migrate-rollouts`, `thread_history_1.sqlite`). If a cached path disappears, look it up again by id through `state_5.sqlite:threads.rollout_path` before treating it as deleted.
