#!/usr/bin/env python3
"""Write a synthetic CODEX_HOME for demos and screenshots: an invented key-value store project.

    python3 scripts/make_demo_home.py /tmp/demo-codex-home

Rollouts use the current Codex format (item_completed events, ordinal-reference forks), so the
real binary reads them like any other sessions. All names, prompts and ids are made up.
"""
import json
import os
import sys
import time
from datetime import datetime, timezone

DAY = 86400
CWD = "/home/dev/kvstore"


def turns(prefix, n):
    return [(f"{prefix} {i}", "ok") for i in range(n)]


# id, thread name, parent id, fork point (turns shared), created (days ago), own turns, cwd suffix
SESSIONS = [
    ("5c1e8a20-3d4f-7a10-9b2e-4f6a8c0d1e21", "Design the storage engine", None, 0, 7.2, [
        ("We have a log-structured key-value store prototype; the storage engine is half done and the WAL is missing.", "I'll start by mapping the current write path."),
        ("What does the write path look like today?", "Writes go straight to the memtable; nothing is durable."),
        ("List what the engine needs from the file format.", "Record framing, checksums, and a manifest."),
        ("Keep the record header to 8 bytes.", "Done: length plus CRC32."),
        ("Sketch an on-disk B-tree index.", "Here is a page layout with 4 KiB nodes."),
        ("Drop the B-tree; an SSTable index is enough.", "Agreed; sparse index per table."),
        ("Write the format notes to docs/format.md.", "Written."),
        ("Add a version byte to the manifest.", "Added."),
        ("Agreed on the format. Now plan the WAL work in stages.", "Five stages, each ending with a green crash test."),
    ] + turns("Planning detail", 29), ""),
    ("6a2f0b31-4e5a-7b21-8c3f-5a7b9d1e2f32", "Earlier attempt: direct port", "5c1e8a20-3d4f-7a10-9b2e-4f6a8c0d1e21", 3, 7.1,
        [("Skip the plan. Port the WAL from the old prototype file by file.", "Ported 4 of 9 files; the rest need the new framing.")] + turns("Port step", 83), ""),
    ("7b3a1c42-5f6b-7c32-9d4a-6b8cae2f3a43", "Implement WAL replay", "5c1e8a20-3d4f-7a10-9b2e-4f6a8c0d1e21", 9, 6.9, [
        ("Start stage 1: write the WAL record format.", "Record writer and reader are in place."),
        ("Stage 1 passes. Before stage 2, review the compaction module.", "Compaction holds a global lock during merges."),
        ("Fix the lock first.", "Now per-level locks."),
        ("Continue with stage 2.", "Replay works for clean shutdowns."),
        ("Run the crash-recovery tests and report what fails.", "2 of 40 fail: both lose the last write."),
    ] + turns("Stage work", 50), ""),
    ("8c4b2d53-6a7c-7d43-ae5b-7c9dbf3a4b54", "Rewrite compaction", "7b3a1c42-5f6b-7c32-9d4a-6b8cae2f3a43", 11, 6.8,
        [("Rewrite compaction to merge runs lazily.", "Lazy merging cut write amplification by 30%.")] + turns("Compaction step", 7)
        + [("Merging is still too slow. Can we bound it?", "Yes, with a per-level budget.")] + turns("Budget step", 31), ""),
    ("9d5c3e64-7b8d-7e54-bf6c-8daec04b5c65", "Compaction: try leveled", "8c4b2d53-6a7c-7d43-ae5b-7c9dbf3a4b54", 20, 6.4,
        [("Try leveled compaction with 10x fan-out.", "Read amplification drops; writes go up 2x.")] + turns("Leveled step", 13), ""),
    ("ae6d4f75-8c9e-7f65-80d7-9ebfd15c6d76", "Benchmark write stalls", "7b3a1c42-5f6b-7c32-9d4a-6b8cae2f3a43", 14, 6.2,
        [("Find which commit made write stalls longer.", "Bisected to the commit that added per-level locks.")] + turns("Bench step", 15), ""),
    ("bf7e5086-9daf-7076-91e8-afc0e26d7e87", "Explain fsync ordering", "7b3a1c42-5f6b-7c32-9d4a-6b8cae2f3a43", 14, 6.0, [
        ("Explain in detail why fsync ordering loses the last write after a crash.", "The data file is synced, but its directory entry is not."),
        ("Show the exact sequence of syscalls.", "write, fsync(file), rename; the rename is never synced."),
        ("Is that true on ext4 with data=ordered?", "Yes; ordering covers data blocks, not the directory."),
        ("Write a test that reproduces it.", "Added a crash test with a fault-injecting filesystem."),
        ("So the directory entry is not synced?", "Right: rename is durable only after fsync on the directory."),
    ] + turns("Follow-up", 30) + [("so the fix is one extra fsync on the directory per rotation?", "Yes, and it only runs when the WAL segment rotates.")], ""),
    ("c08f6197-aeb0-7187-a2f9-b0d1f37e8f98", "fsync: batch alternative", "bf7e5086-9daf-7076-91e8-afc0e26d7e87", 19, 0.0, [
        ("Sync the directory in a batch instead of per write.", "Batching saves 40% of fsync calls."),
        ("Run the crash suite again.", "Running: 9 of 14 done, all passing."),
        ("Also measure p99 latency.", "p99 went from 4.1 ms to 3.2 ms."),
        ("Write it up.", "Written to docs/fsync.md."),
    ], ""),
    # a second, smaller tree
    ("d1a07f10-1b2c-7d3e-8f40-516273849a01", "Port the CLI to clap", None, 0, 3.0, [
        ("Port the argument parser to clap derive.", "Done; all subcommands moved."),
        ("Add shell completions.", "Added for bash, zsh and fish."),
        ("Generate a man page too.", "Added via clap_mangen."),
    ], ""),
    ("d2b18021-2c3d-7e4f-9051-62738495ab12", "CLI: keep getopts", "d1a07f10-1b2c-7d3e-8f40-516273849a01", 1, 2.5,
        [("Actually keep getopts but clean up the help text.", "Help text rewritten.")], ""),
    # standalone sessions
    ("e3c29132-3d4e-7f50-a162-7384950abc23", "Explain the bloom filter sizing", None, 0, 1.0,
        [("How many bits per key does the bloom filter need for 1% false positives?", "About 9.6 bits per key with 7 hash functions.")], ""),
    ("f4d3a243-4e5f-7061-b273-849506bcd134", "Fix typo in README", None, 0, 0.2,
        [("Fix the typo in the README install section.", "Fixed.")], ""),
    ("05e4b354-5f60-7172-c384-9506a7cde245", "Profile the benchmark harness", None, 0, 4.0,
        [("Profile bench/run.rs and find the hot loop.", "80% of the time is spent formatting keys.")], "/bench"),
]


def iso(t):
    return datetime.fromtimestamp(t, timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")


def main(home):
    now = time.time()
    day_dir = os.path.join(home, "sessions/2026/10/01")
    os.makedirs(day_dir, exist_ok=True)
    history = {}  # session id -> [(ordinal, prompt, reply)] including inherited turns
    ordinal = 1
    names = []
    for sid, name, parent, k, ago, own, sub in SESSIONS:
        created = now - ago * DAY
        meta = {"id": sid, "cwd": CWD + sub, "source": "cli", "cli_version": "0.161.0", "timestamp": iso(created)}
        meta_ordinal, inherited = 0, []
        if parent:
            # forks reference the parent's history by ordinal instead of copying it
            meta["forked_from_id"] = parent
            ph = history[parent]
            cut = ph[k][0] if k < len(ph) else ordinal
            meta_ordinal, inherited = cut, [t for t in ph if t[0] < cut]
            ordinal = max(ordinal, cut)
        lines = [{"timestamp": iso(created), "ordinal": meta_ordinal, "type": "session_meta", "payload": meta}]
        mine, t = [], created + 30
        for prompt, reply in own:
            ordinal += 1
            user_ordinal = ordinal
            lines.append({"timestamp": iso(t), "ordinal": user_ordinal, "type": "event_msg", "payload": {
                "type": "item_completed", "item": {"type": "UserMessage", "id": f"u{user_ordinal}", "content": [{"type": "text", "text": prompt}]}}})
            ordinal += 1
            lines.append({"timestamp": iso(t + 5), "ordinal": ordinal, "type": "event_msg", "payload": {
                "type": "item_completed", "item": {"type": "AgentMessage", "content": [{"type": "Text", "text": reply}]}}})
            mine.append((user_ordinal, prompt, reply))
            t += 60
        history[sid] = inherited + mine
        ordinal += 100
        path = os.path.join(day_dir, f"rollout-2026-10-01T00-00-00-{sid}.jsonl")
        with open(path, "w") as f:
            f.writelines(json.dumps(l, ensure_ascii=False) + "\n" for l in lines)
        mtime = now if ago == 0 else created + 3600  # the newest fork is "live"
        os.utime(path, (mtime, mtime))
        names.append({"id": sid, "thread_name": name})
    with open(os.path.join(home, "session_index.jsonl"), "w") as f:
        f.writelines(json.dumps(n) + "\n" for n in names)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
