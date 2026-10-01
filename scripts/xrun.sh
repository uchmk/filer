#!/usr/bin/env bash
# Run filer once on a virtual X display and keep what it showed -- the Linux
# lane's way of pressing keys and reading the result (.claude/linux-role.md).
#
#   scripts/xrun.sh OUTDIR [filer arguments...]
#
# Starts its own Xvfb, launches filer with the arguments (`--keys` included),
# waits for the window and then XRUN_WAIT seconds (default 3) more, and writes
# into OUTDIR:
#
#   shot.png     the whole screen
#   title.txt    the window's title, which names the folder the list is in
#   clip.txt     the clipboard, read while filer still owns it (an X clipboard
#                dies with its owner, so reading it afterwards gets nothing)
#   filer.log    what filer printed
#
# The clipboard is armed with XRUN-SENTINEL first, so an unchanged clipboard
# reads as that rather than as the previous run's. FILER_BIN overrides the
# binary (default target/debug/filer). Needs Xvfb, xdotool, xclip, ImageMagick.

set -u
out="${1:?usage: scripts/xrun.sh OUTDIR [filer arguments...]}"
shift
mkdir -p -- "$out"
bin="${FILER_BIN:-target/debug/filer}"
wait_s="${XRUN_WAIT:-3}"

for tool in Xvfb xdotool xclip import; do
    command -v "$tool" >/dev/null || { echo "xrun: $tool is missing (apt-get install -y xvfb xdotool xclip imagemagick)" >&2; exit 2; }
done

# A display number nobody else holds.
n=90
while [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ]; do n=$((n + 1)); done
Xvfb ":$n" -screen 0 1400x900x24 -nolisten tcp >/dev/null 2>&1 &
xvfb=$!
trap 'kill "$xvfb" 2>/dev/null' EXIT
export DISPLAY=":$n"
for _ in $(seq 1 50); do xdotool getdisplaygeometry >/dev/null 2>&1 && break; sleep 0.1; done

printf 'XRUN-SENTINEL' | xclip -selection clipboard -i -loops 1 &
sleep 0.2

"$bin" "$@" >"$out/filer.log" 2>&1 &
pid=$!
win=""
for _ in $(seq 1 100); do
    win=$(xdotool search --pid "$pid" --name . 2>/dev/null | head -1)
    [ -n "$win" ] && break
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.2
done
if [ -z "$win" ]; then
    echo "xrun: no window appeared; see $out/filer.log" >&2
    kill "$pid" 2>/dev/null
    exit 1
fi
sleep "$wait_s"

import -window root "$out/shot.png"
xdotool getwindowname "$win" >"$out/title.txt" 2>/dev/null
timeout 2 xclip -selection clipboard -o >"$out/clip.txt" 2>/dev/null || true

kill "$pid" 2>/dev/null
wait "$pid" 2>/dev/null
echo "xrun: $out (title: $(cat "$out/title.txt"))"
