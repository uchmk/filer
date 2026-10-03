#!/usr/bin/env bash
# Run at the start of a Claude Code session (.claude/settings.json, SessionStart),
# and only does anything in a cloud session (CLAUDE_CODE_REMOTE=true): the
# Windows machines' sessions run the same settings and have disk to spare.
#
# A cloud session has a fixed disk allowance, and on 2026-10-03 one ran out of
# it twice: `target` reached 24 GB from debug builds, the Windows-target clippy
# and the examples, and a build that cannot write stops the work until someone
# notices. Two things keep it down:
#
# - CARGO_INCREMENTAL=0 for the session. The incremental caches were the
#   largest single part, and a session rebuilding after each small edit gets
#   little from them that the dependency cache does not already give.
# - If `target` is still over the limit when a session starts (one left by an
#   earlier session in the same container), clear it. A full rebuild costs a
#   few minutes; a full disk costs the session.
[ "${CLAUDE_CODE_REMOTE:-}" = "true" ] || exit 0
cd "$(dirname "$0")/.." || exit 0

if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  echo 'export CARGO_INCREMENTAL=0' >> "$CLAUDE_ENV_FILE"
fi

limit_gb="${FILER_TARGET_LIMIT_GB:-12}"
if [ -d target ]; then
  used_gb=$(du -s --block-size=1G target 2>/dev/null | cut -f1)
  if [ "${used_gb:-0}" -gt "$limit_gb" ]; then
    rm -rf target
    echo "cloud-session-start: target was ${used_gb} GB (limit ${limit_gb}); cleared it"
  fi
fi
exit 0
