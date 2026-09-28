#!/usr/bin/env bash
#
# Start filer on a virtual display, photograph it, and fail if what came back is
# not a drawn window.
#
# Until this existed, CI knew one thing about the Linux and macOS builds: that
# they compile and link. The release notes said so in as many words -- "nobody
# has started the program on either" -- which means a binary that panics on
# launch, or opens a window and paints nothing, would have shipped green. That
# is the gap this closes for Linux. Not what the program *does*, only that it
# comes up and puts something on the screen, which is the one thing every other
# check takes for granted.
#
#     scripts/smoke-linux.sh [--out shot.png] [--keep]
#
# Needs Xvfb, ImageMagick, and a software Vulkan driver (Mesa's lavapipe). On
# Ubuntu: xvfb imagemagick mesa-vulkan-drivers libxkbcommon-x11-0.
#
# Why software Vulkan: eframe draws through wgpu, which wants a real adapter. A
# CI runner has no GPU, so lavapipe stands in -- it is slow and it is correct,
# and correctness is all this asks for.

set -euo pipefail

OUT="shot.png"
KEEP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --keep) KEEP=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/debug/filer"
[ -x "$BIN" ] || BIN="$ROOT/target/release/filer"
if [ ! -x "$BIN" ]; then
  echo "no filer binary; run cargo build first" >&2
  exit 1
fi

# A display number nothing else is using. `-a` on xvfb-run picks one for you but
# gives no way to reach it afterwards, and the screenshot has to know where to
# point.
DISP=":88"
LOG="$(mktemp)"
Xvfb "$DISP" -screen 0 1400x900x24 >/dev/null 2>&1 &
XVFB=$!
cleanup() {
  [ -n "${APP:-}" ] && kill "$APP" 2>/dev/null || true
  kill "$XVFB" 2>/dev/null || true
  [ "$KEEP" = 1 ] || rm -f "$LOG"
}
trap cleanup EXIT

# Xvfb is not listening the instant it is spawned, and a client that connects
# too early fails in a way that reads like the program's fault.
for _ in $(seq 1 40); do
  DISPLAY="$DISP" xdpyinfo >/dev/null 2>&1 && break
  sleep 0.25
done

DISPLAY="$DISP" "$BIN" "$ROOT/src" >"$LOG" 2>&1 &
APP=$!

# Long enough for the window to exist, the first scan to land and a frame to be
# painted. Shorter than this and a slow runner photographs an empty window and
# calls it a failure.
sleep 12

if ! kill -0 "$APP" 2>/dev/null; then
  echo "filer exited before it could be photographed:" >&2
  sed 's/^/    /' "$LOG" >&2
  exit 1
fi

DISPLAY="$DISP" import -window root "$OUT"

# Two questions, in order of how badly they fail.
#
# A window that never painted is a uniform rectangle, so the spread of pixel
# values is zero. Any drawn frame -- text, rules, a highlighted row -- puts it
# well above that. The threshold is deliberately near the floor: this is asking
# "did anything happen", not "did the right thing happen", and a tighter bound
# would start failing on a theme change, which is not a regression.
SD=$(identify -format "%[fx:standard_deviation]" "$OUT")
echo "pixel spread: $SD"
if awk -v v="$SD" 'BEGIN { exit !(v < 0.01) }'; then
  echo "the window is blank -- filer started but painted nothing" >&2
  exit 1
fi

# And the frame should be filer's, not a default grey. The theme's background is
# #16181d; a wgpu surface that failed over to something else would not be.
DARK=$(identify -format "%[fx:mean]" "$OUT")
echo "mean brightness: $DARK"
if awk -v v="$DARK" 'BEGIN { exit !(v > 0.5) }'; then
  echo "the frame is light -- filer's default theme is dark, so this is not it" >&2
  exit 1
fi

echo "filer started, drew a frame, and it looks like filer: $OUT"
if [ -s "$LOG" ]; then
  echo "--- stderr (not a failure; here because it is easy to lose) ---"
  sed 's/^/    /' "$LOG"
fi
