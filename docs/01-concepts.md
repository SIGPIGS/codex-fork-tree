# 01 — Concepts and Tree Model

This document defines what the TUI shows: which conversations go into the tree, how they connect to each other, and which ones are left out. The interaction and visual docs build on these terms.

## Glossary

| Term | Meaning |
|---|---|
| **Session** | One Codex conversation. It has one rollout file in `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`, plus a row in `state_5.sqlite:threads` when Codex has indexed it. |
| **Focus session** | The "current conversation". The tree is always built around this one session. |
| **Project** | The scope used for the cache and for discovery. It is the git root of the focus session's `cwd`, or the `cwd` itself when there is no git root. |
| **Turn** | One real user message plus the agent messages that answer it. Injected context is not a turn (see [Turns and fingerprints](#turns-and-fingerprints)). |
| **Turn log** | A session's sequence of turns, with rollbacks applied. Every prefix comparison runs on turn logs. |
| **Shared prefix** | The longest common prefix of two sessions' (effective) turn logs. |
| **Nonempty prefix** | A shared prefix that contains **at least one complete turn** (prompt and reply). Matching boilerplate alone, such as AGENTS.md, environment context or base instructions, does not count. |
| **Fork point** | The position where two histories diverge: the last shared item, labeled by the user turn it belongs to. |
| **Candidate** | A session that the project cache says *might* share a prefix with the focus session. Candidates are cheap to find, and each one is verified before it appears in the tree. |

## The scope rule

```
displayed(F) = { F } ∪ { S ∈ candidates(project(F)) | sharedPrefix(S, F) is nonempty }
```

- The cache is **project-wide**, so it knows every session in the project.
- The **tree is not project-wide**. It holds only the focus session and the sessions related to it by history. A project with 80 sessions might show a tree of 4.
- Prefixes compare from the start of each history, so the rule is effectively transitive at the root: every displayed session has the same first real user turn as `F`. That makes this the right cache key for discovering candidates (see [04-cache.md](04-cache.md)).

## Three kinds of fork evidence

Forks reach the tree through three sources. The tree treats all of them the same, but the UI labels where each edge came from (see [03-ui-style.md](03-ui-style.md#edges)).

| Provenance | Source | Applies to | Confidence |
|---|---|---|---|
| **native** | `session_meta.forked_from_id`, with the fork cut taken from `forked_from_ordinal_exclusive` (≥ 0.160) or the child's own first ordinal (0.147+) | every fork made by Codex since about 0.142, including forks created before this package was installed | Exact |
| **recorded** | this package's ledger, written when the TUI itself runs `codex fork` | a safety net; no Codex hook is needed | Exact |
| **inferred** | a history copied in one burst at creation that matches an earlier session's turns | Codex older than about 0.142, which copied history without metadata | Heuristic, shown as such |

Notes from the rollouts on this machine (details in [06-algorithms.md](06-algorithms.md)):

- 47 of the 317 rollouts carry `forked_from_id`. 38 of them are **subagent spawns** (`session_meta.source` is an object, not `cli` or `vscode`). **Subagents are out of scope:** they are provisional and short-lived, so they are never discovered, cached or displayed. See [06-algorithms.md](06-algorithms.md#54-subagents-are-excluded).
- All 9 user forks carry native metadata. Since 0.147, forks do **not** copy history: the child's ordinals continue the parent's sequence, so `effective(child) = parent turns before the cut ++ own turns`.
- Prefix inference found no extra forks here. It stays as the path for older Codex histories.

## Turns and fingerprints

Prefixes are compared on the **user-visible turn log**, not on raw `response_item`s. Raw items differ even between true copies, and injected context (AGENTS.md, environment, permissions) is different in every session.

- A **turn** is one real user message plus the agent messages that answer it. It comes from `item_completed` `UserMessage`/`AgentMessage` events (0.147+), from `user_message`/`agent_message` events (older versions), or, as a last resort, from `response_item` messages with injected context filtered out.
- `thread_rolled_back` removes turns. Compaction is ignored, because it changes model context, not the conversation.
- A turn's fingerprint covers the **prompt and the reply**. Of 33 pairs here with the same first prompt, 27 got different replies (the prompt was re-typed, not forked).
- A shared prefix is **nonempty** when it contains at least one complete turn.
- Text alone never proves a fork. Two sessions that both said "hello" and got the same reply are not related unless metadata links them, or one of them copied the other's history in a burst at creation.

## Building the tree

1. Expand each session's effective history (resolving ordinal-reference chains).
2. Give each session at most one parent edge `(parent, k)`, where `k` is the fork point (shared turns). Precedence: user verdict > native > recorded > inferred.
3. Drop edges with `k = 0`. The displayed tree is the connected tree around the focus.
4. In the framed trie, a session's children are grouped by fork point `k` (the `◆ turn k` rows) and ordered by creation time.
5. A copied history whose parent no longer exists hangs under a virtual **⌂** root labeled `parent not found · k shared turns`.

## What "manage" means

| Action | How | Destructive? |
|---|---|---|
| Resume a session | `codex resume <id>` (TUI suspends, then restores) | no |
| Fork a session (from its tip) | `codex fork <id>`; the new session goes straight into the ledger as `recorded` | no |
| Re-focus | rebuild the tree around another node | no |
| Rename | write `thread_name` through Codex where possible | no |
| Archive / unarchive | `codex archive` / `codex unarchive` | reversible |
| Delete | `codex delete`, with typed confirmation | **yes** |
| Confirm / reject an inferred edge | stored in this package's ledger only | no |

The package **never edits rollout files**. Every change to Codex's own state goes through the `codex` CLI.

## Out of scope for v1

- Subagent sessions. They are provisional and short-lived.
- Forking from a mid-history turn. Codex's fork CLI takes a session id, not an ordinal; check before v2 whether the app-server API exposes this.
- Trees that span projects. If the same prompt appears in two repos, those sessions don't count as related.
- Codex Cloud tasks.
