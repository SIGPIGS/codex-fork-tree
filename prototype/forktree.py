#!/usr/bin/env python3
"""Throwaway prototype of the fork-tree algorithms in docs/06-algorithms.md.

Reads ~/.codex/sessions, builds the fork tree around one session, and prints it
as a framed trie. Standard library only; no cache (a cold scan takes about 1 s).

    python3 -I prototype/forktree.py [<session-id-prefix>] [--width N]
"""
import glob, hashlib, json, os, sys, unicodedata
from collections import defaultdict
from datetime import datetime

CODEX_HOME = os.environ.get("CODEX_HOME", os.path.expanduser("~/.codex"))
BURST_WINDOW_S = 2.0          # copied history is written within this window after creation
INJECTED_PREFIXES = ("# AGENTS.md", "<environment_context", "<user_instructions", "<permissions", "<collaboration_mode", "<turn_aborted")
PREFILTER = (b'"session_meta"', b'"user_message"', b'"agent_message"', b'"thread_rolled_back"', b'"UserMessage"', b'"AgentMessage"', b'"message"')


def ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp() if s else 0.0


def norm(t):
    return " ".join((t or "").split())


def text_of(content):
    if isinstance(content, str):
        return content
    return "".join(c.get("text", "") for c in content or [] if isinstance(c, dict))


class Turn:
    __slots__ = ("user", "agent", "ts", "ordinal", "uid", "fp")

    def __init__(self, user, ts_, ordinal=None, uid=None):
        self.user, self.agent, self.ts, self.ordinal, self.uid, self.fp = user, [], ts_, ordinal, uid, None


class Session:
    def __init__(self, path):
        self.path, self.meta, self.turns, self.fmt, self.meta_ordinal = path, None, [], None, None
        self.parent = self.fork_k = self.edge = None
        self.children = []

    # ---- derived fields -------------------------------------------------
    @property
    def id(self): return self.meta["id"]
    @property
    def created(self): return ts(self.meta.get("timestamp"))
    @property
    def sub(self): return not isinstance(self.meta.get("source"), str)
    @property
    def title(self):  # Codex's thread name, else the first own prompt
        return NAMES.get(self.id) or (self.turns[0].user if self.turns else "(empty)")[:80]


def load_names():
    names = {}
    try:
        with open(os.path.join(CODEX_HOME, "session_index.jsonl")) as fh:
            for line in fh:
                try:
                    o = json.loads(line)
                    names[o["id"]] = o.get("thread_name") or names.get(o["id"])
                except (ValueError, KeyError):
                    pass
    except OSError:
        pass
    return names


NAMES = load_names()


def parse(path):
    """Extract the user-visible turn log. Three record formats exist in the wild:
    F2 item_completed UserMessage/AgentMessage (0.147+), F1 event_msg user_message/agent_message
    (0.104-0.147), F3 raw response_item messages. Subagent rollouts are out of scope and skipped."""
    s = Session(path)
    f1, f2, f3 = [], [], []
    with open(path, "rb") as fh:
        for i, raw in enumerate(fh):
            if i and not any(k in raw[:400] for k in PREFILTER):
                continue
            try:
                o = json.loads(raw)
            except ValueError:
                continue                      # torn last line while Codex is writing
            t, p = o.get("type"), o.get("payload") or {}
            when, ordinal = ts(o.get("timestamp")), o.get("ordinal")
            if t == "session_meta":
                if s.meta is None and not isinstance(p.get("source"), str):
                    return None                   # subagent: provisional, never tracked
                if s.meta is None:
                    s.meta, s.meta_ordinal = p, ordinal
            elif t == "event_msg":
                pt = p.get("type")
                if pt == "item_completed":
                    it = p.get("item") or {}
                    if it.get("type") == "UserMessage":
                        f2.append(Turn(norm(text_of(it.get("content"))), when, ordinal, it.get("id")))
                    elif it.get("type") == "AgentMessage" and f2:
                        f2[-1].agent.append(norm(text_of(it.get("content"))))
                elif pt == "user_message":
                    f1.append(Turn(norm(p.get("message")), when, ordinal))
                elif pt == "agent_message" and f1:
                    f1[-1].agent.append(norm(p.get("message")))
                elif pt == "thread_rolled_back":
                    for log in (f1, f2, f3):
                        del log[max(0, len(log) - p.get("num_turns", 1)):]
            elif t == "response_item" and p.get("type") == "message":
                txt = norm(text_of(p.get("content")))
                if p.get("role") == "user" and txt and not txt.startswith(INJECTED_PREFIXES):
                    f3.append(Turn(txt, when, ordinal))
                elif p.get("role") == "assistant" and f3:
                    f3[-1].agent.append(txt)
    s.fmt, s.turns = ("F2", f2) if f2 else ("F1", f1) if f1 else ("F3", f3)
    for t in s.turns:
        t.fp = hashlib.sha1((t.user + "\0" + "\x01".join(t.agent)).encode()).hexdigest()[:16]
    return s if s.meta and "id" in s.meta else None


def burst_len(s):
    """Leading turns written in one burst right after creation = history copied at fork time."""
    k, c = 0, s.created
    for t in s.turns:
        if t.ts - c > BURST_WINDOW_S:
            break
        k += 1
    return k


def effective(s, by_id, memo):
    """Effective turn log: native forks with an ordinal reference inherit parent[: ordinal]."""
    if s.id in memo:
        return memo[s.id]
    memo[s.id] = s.turns                      # cycle guard
    ff = s.meta.get("forked_from_id")
    # explicit cut (0.160+), else the child's own first ordinal continues the parent's sequence (0.147+)
    cut = s.meta.get("forked_from_ordinal_exclusive") or (s.meta_ordinal if s.meta_ordinal else None)
    if ff in by_id and cut is not None:
        inherited = [t for t in effective(by_id[ff], by_id, memo) if t.ordinal is not None and t.ordinal < cut]
        memo[s.id] = inherited + s.turns
    return memo[s.id]


def lcp(a, b):
    n = 0
    for x, y in zip(a, b):
        if x.fp != y.fp:
            break
        n += 1
    return n


def build(sessions):
    by_id = {s.id: s for s in sessions}
    memo = {}
    eff = {s.id: effective(s, by_id, memo) for s in sessions}
    # prefix index: (k, H_k) -> sessions, H_k = rolling hash of the first k turn fingerprints
    index = defaultdict(list)
    for s in sessions:
        h = ""
        for k, t in enumerate(eff[s.id], 1):
            h = hashlib.sha1((h + t.fp).encode()).hexdigest()[:16]
            index[(k, h)].append(s)
        s._h = h
    def prefix_hash(s, k):
        h = ""
        for t in eff[s.id][:k]:
            h = hashlib.sha1((h + t.fp).encode()).hexdigest()[:16]
        return h

    for s in sessions:
        ff = s.meta.get("forked_from_id")
        if ff in by_id:                                              # native edge
            p = by_id[ff]
            k = lcp(eff[p.id], eff[s.id])
            s.parent, s.fork_k, s.edge = p, k, "native"
            continue
        k = burst_len(s)
        if k == 0:
            continue                                                 # organic first turn: a root
        cands = [c for c in index[(k, prefix_hash(s, k))] if c is not s and c.created < s.created]
        # the origin of turn k is the candidate that wrote it organically (not inside its own burst)
        origin = [c for c in cands if burst_len(c) < k] or cands
        if origin:
            p = min(origin, key=lambda c: c.created)
            s.parent, s.fork_k, s.edge = p, k, "inferred"
    for s in sessions:
        if s.parent is not None and s.fork_k == 0:
            s.parent = None                                          # nonempty-prefix rule: k >= 1
        if s.parent is not None:
            s.parent.children.append(s)
    return eff



def tree_of(focus):
    root = focus
    while root.parent is not None:
        root = root.parent
    return root


# ---- framed-trie rendering (same rules as the demo, plain text) -----------
def cw(ch):
    return 2 if unicodedata.east_asian_width(ch) in "WF" else 0 if unicodedata.combining(ch) else 1


def dw(text):  # display width in terminal cells (CJK = 2)
    return sum(cw(c) for c in text)


def fit(text, w):
    """Truncate to at most w cells, padding to exactly w."""
    if dw(text) <= w:
        return text + " " * (w - dw(text))
    out, n = "", 0
    for c in text:
        if n + cw(c) > w - 1:
            break
        out, n = out + c, n + cw(c)
    return out + "…" + " " * (w - 1 - n)


def lr(left, right, w):
    return fit(left, w - dw(right) - 1) + " " + right


def render(s, eff, focus, from_k, W, out):
    kids = s.children
    own = len(eff[s.id]) - from_k
    mark = "●" if s is focus else "○"
    if not kids or W < 30:
        return out.append(("leaf", s, mark, own))
    h, v, tl, tr, bl, br = ("━", "┃", "┏", "┓", "┗", "┛") if s is focus else ("┄", "┆", "╭", "╮", "╰", "╯") if s.edge == "inferred" else ("─", "│", "╭", "╮", "╰", "╯")
    lab = f" +{own} "
    title = fit(s.title, min(dw(s.title), W - 10 - len(lab)))
    title = title.rstrip()
    lines = [tl + h + " ▣ " + title + " " + h * max(1, W - 8 - dw(title) - len(lab)) + lab + h + tr]
    inner = []
    forks = sorted({c.fork_k for c in kids})
    for fi, k in enumerate(forks):
        prompt = eff[s.id][k - 1].user if k - 1 < len(eff[s.id]) else "?"
        inner.append(lr(f"◆ turn {k} “{prompt}”", "", W - 4))
        group = sorted((c for c in kids if c.fork_k == k), key=lambda c: c.created)
        for j, c in enumerate(group):
            last = fi == len(forks) - 1 and j == len(group) - 1
            conn = ("└" if last else "├") + ("┄" if c.edge == "inferred" else "─")
            sub_out = []
            render(c, eff, focus, k, W - 6, sub_out)
            for i, line in enumerate(sub_out):
                if isinstance(line, tuple):
                    _, cs, m, n = line
                    inner.append(lr(conn + m + " " + cs.title + ("  ≈" if cs.edge == "inferred" else ""), f"+{n}", W - 4))
                else:
                    inner.append((conn if i == 0 else ("  " if last else "│ ")) + line)
    lines += [v + " " + fit(x, W - 4) + " " + v for x in inner]
    end = f" ends at turn {len(eff[s.id])} "
    lines.append(bl + h * max(1, W - 3 - len(end)) + end + h + br)
    out.extend(lines)


def main():
    args = sys.argv[1:]
    width = int(args[args.index("--width") + 1]) if "--width" in args else 100
    want = next((a for a in args if not a.startswith("--") and not a.isdigit()), None)
    files = glob.glob(os.path.join(CODEX_HOME, "sessions", "*", "*", "*", "*.jsonl"))
    sessions = [s for s in map(parse, files) if s]
    eff = build(sessions)
    if not want:
        roots = [s for s in sessions if s.parent is None and s.children]
        print(f"{len(sessions)} sessions, {len(roots)} trees with forks")
        for r in sorted(roots, key=lambda s: s.created):
            n = sum(1 for _ in walk(r))
            print(f"  {r.id[:8]}  {n:3d} sessions  {r.meta.get('cwd')}  {r.title[:50]}")
        return
    matches = [s for s in sessions if s.id.startswith(want)]
    if len(matches) != 1:
        sys.exit(f"{len(matches)} sessions match {want!r}")
    focus = matches[0]
    out = []
    render(tree_of(focus), eff, focus, 0, width, out)
    for line in out:
        print(lr(f"○ {line[1].title}", f"+{line[3]}", width) if isinstance(line, tuple) else line)


def walk(s):
    yield s
    for c in s.children:
        yield from walk(c)


if __name__ == "__main__":
    main()
