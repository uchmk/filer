#!/usr/bin/env bash
# Works through TODO.md unattended: one task per `claude -p` run, committed to
# the auto/todo branch and pushed to origin/auto/todo after each task. Waits
# out usage limits and retries. Rules for the
# agent live in CLAUDE.md ("自動実行モード").
#
#   bash auto-todo.sh
#
# Log: $HOME/filer-auto.log

set -u
cd "$(dirname "$0")" || exit 1
export PATH="$HOME/.cargo/bin:$PATH"

LOG="$HOME/filer-auto.log"
BRANCH="auto/todo"
LIMIT_WAIT=3600   # seconds to wait when a usage limit is hit
ERROR_WAIT=300    # seconds to wait after any other failure
MAX_FAILS=3       # consecutive non-limit failures before giving up

# Uncommitted work on another branch would end up in the agent's first commit.
# On auto/todo it is what an interrupted run left behind (usage limit, reboot),
# and the next run picks it up (CLAUDE.md "自動実行モード").
if [ "$(git branch --show-current)" != "$BRANCH" ] && [ -n "$(git status --porcelain)" ]; then
  echo "作業ツリーに未コミットの変更があります。コミットするか退避してから実行してください。"
  git status --short
  exit 1
fi
git switch "$BRANCH" 2>/dev/null || git switch -c "$BRANCH" || exit 1

PROMPT='自動実行モードです。CLAUDE.md と docs/claude/auto-mode.md の「自動実行モード」のルールに従い、TODO.md の未完了タスクを 1 つだけ進めてください。'

TOOLS='Bash(cargo:*),Bash(git add:*),Bash(git commit:*),Bash(git status:*),Bash(git diff:*),Bash(git log:*),Bash(git restore:*),Bash(git clean:*),Bash(git push -u origin auto/todo),PowerShell(cargo:*),PowerShell(git add:*),PowerShell(git commit:*),PowerShell(git status:*),PowerShell(git diff:*),PowerShell(git log:*),PowerShell(git restore:*),PowerShell(git clean:*),PowerShell(git push -u origin auto/todo)'

# Pushes whatever the agent committed but did not get pushed. Never forced.
push_pending() {
  if [ -n "$(git log "origin/$BRANCH..$BRANCH" --oneline 2>/dev/null)" ] \
     || ! git rev-parse --verify --quiet "origin/$BRANCH" >/dev/null; then
    git push -u origin "$BRANCH" || echo "push に失敗しました。次の回で再試行します。"
  fi
}

fails=0
run=0
while :; do
  run=$((run + 1))
  echo "[$(date '+%F %T')] run $run"
  out=$(claude -p "$PROMPT" --permission-mode acceptEdits --allowedTools "$TOOLS" 2>&1)
  code=$?
  {
    echo "===== $(date '+%F %T') run $run exit=$code"
    echo "$out"
  } >> "$LOG"

  if echo "$out" | tail -n 1 | tr -d '\r' | grep -qx 'ALL_DONE'; then
    echo "$out" | tail -n 3
    echo "自動で進められるタスクは終わりました。QUESTIONS.md の未回答の質問に答えてから、もう一度起動してください。"
    break
  fi

  if [ "$code" -ne 0 ]; then
    if echo "$out" | grep -qi 'limit'; then
      echo "利用制限中。$((LIMIT_WAIT / 60)) 分後に再試行します。"
      sleep "$LIMIT_WAIT"
      continue
    fi
    fails=$((fails + 1))
    echo "失敗しました（$fails/$MAX_FAILS）。ログ: $LOG"
    if [ "$fails" -ge "$MAX_FAILS" ]; then
      echo "連続で失敗したので停止します。"
      exit 1
    fi
    sleep "$ERROR_WAIT"
    continue
  fi

  fails=0
  git log -1 --oneline
  push_pending
done
push_pending
