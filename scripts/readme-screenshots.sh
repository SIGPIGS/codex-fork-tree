#!/bin/sh
# Regenerate docs/images/*.png from the real binary running on invented demo data.
# Needs tmux, python3 and Pillow. Usage: scripts/readme-screenshots.sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --quiet
work=$(mktemp -d)
# a placeholder session keeps one tmux server alive across shots (killing the last session
# stops the server, and the next shot can race its shutdown)
tmux new-session -d -s cft-keep "sleep 600"
trap 'tmux kill-session -t cft-shot 2>/dev/null || true; tmux kill-session -t cft-keep 2>/dev/null || true; rm -rf "$work"' EXIT
python3 scripts/make_demo_home.py "$work/home"
mkdir -p docs/images

# shot NAME COLS ROWS "ARGS" "KEYS" TITLE
shot() {
    tmux kill-session -t cft-shot 2>/dev/null || true
    tmux new-session -d -s cft-shot -x "$2" -y "$3" \
        -e CODEX_HOME="$work/home" -e XDG_CACHE_HOME="$work/cache" -e XDG_DATA_HOME="$work/data" \
        -e COLORTERM=truecolor "./target/release/codex-fork-tree $4 --theme mocha; sleep 30"
    # wait until the app has drawn its first frame
    for _ in $(seq 40); do
        tmux capture-pane -t cft-shot -p | grep -q "codex-fork-tree\|all trees" && break
        sleep 0.1
    done
    sleep 0.3
    if [ -n "$5" ]; then
        tmux send-keys -t cft-shot "$5"
        sleep 0.6
    fi
    tmux capture-pane -t cft-shot -p -e -N > "$work/$1.ans"
    python3 scripts/render_screenshot.py "$work/$1.ans" "docs/images/$1.png" --title "$6"
}

shot tree 118 26 "bf7e5086" "" "codex-fork-tree — fork tree"
shot overview 118 22 "--project /home/dev/kvstore" "" "codex-fork-tree — project overview"
shot symbols 120 33 "bf7e5086" "?" "codex-fork-tree — keys & symbols"
