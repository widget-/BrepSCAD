#!/usr/bin/env bash
# Render the viewer's UI to a PNG on a virtual X display, without taking over
# the real desktop. Requires Xvfb and ffmpeg with x11grab support.
set -euo pipefail

output=${1:?usage: $0 <output.png> [settle-seconds]}
settle=${2:-4}
binary=${VIEWER_BIN:-target/debug/brepscad-viewer}

if [[ ! -x "$binary" ]]; then
    echo "missing $binary; run: cargo build -p brepscad-viewer" >&2
    exit 1
fi

display=:$((100 + RANDOM % 100))
while [[ -e "/tmp/.X${display#:}-lock" ]]; do
    display=:$((100 + RANDOM % 100))
done

Xvfb "$display" -screen 0 1280x800x24 >/dev/null 2>&1 &
xvfb_pid=$!
app_pid=
cleanup() {
    if [[ -n "$app_pid" ]]; then
        kill "$app_pid" 2>/dev/null || true
    fi
    kill "$xvfb_pid" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 50); do
    if xset -display "$display" q >/dev/null 2>&1; then
        break
    fi
    sleep 0.1
done

DISPLAY="$display" WAYLAND_DISPLAY= "$binary" >/dev/null 2>&1 &
app_pid=$!
sleep "$settle"

DISPLAY="$display" ffmpeg -loglevel error -y \
    -f x11grab -video_size 1280x800 -i "$display+0,0" \
    -frames:v 1 "$output"
echo "$output"
