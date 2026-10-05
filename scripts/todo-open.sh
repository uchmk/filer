#!/usr/bin/env bash
# How many open TODO.md items the development session may take: `- [ ]`
# items with none of 【人】 【QA】 【実機】 【後】 or 要確認 anywhere in them
# (an item's mark can sit on a continuation line). Prints the count alone.
#
#   scripts/todo-open.sh        12
#   scripts/todo-open.sh -v     the items too, one per line with its line number
#
# The development routine ends with ALL_DONE only when this says 0
# (.claude/dev-routine.md). On 2026-10-05 a session judged "what is left is
# mostly for the machine or the eye" with 70 such items open, and stopped.

set -euo pipefail
cd "$(dirname "$0")/.."

python3 - "${1:-}" <<'PY'
import re, sys

MARKS = ("【人】", "【QA】", "【実機】", "【後】", "要確認")
lines = open("TODO.md", encoding="utf-8").read().split("\n")
items, i = [], 0
while i < len(lines):
    m = re.match(r"^(\s*)- \[ \] ", lines[i])
    if not m:
        i += 1
        continue
    indent, start, text = len(m.group(1)), i, [lines[i]]
    i += 1
    # The item goes on while lines are indented deeper and are not an item.
    while i < len(lines) and lines[i].strip() and not re.match(r"^\s*- \[", lines[i]) \
            and len(lines[i]) - len(lines[i].lstrip()) > indent:
        text.append(lines[i])
        i += 1
    if not any(mark in " ".join(text) for mark in MARKS):
        items.append((start + 1, lines[start].strip()))

print(len(items))
if sys.argv[1] == "-v":
    for n, first in items:
        print(f"TODO.md:{n}: {first[:160]}")
PY
