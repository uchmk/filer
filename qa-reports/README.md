# Reports from the testing sessions

One file per run, added by the run's own pull request and never edited by
another: `<YYYY-MM-DD>-<branch without test/>.md`, for example
`2026-10-03-win-32-9a.md` for `test/win-32-9a`. The Windows lanes
(`.claude/windows-role.md`), the Linux lane (`.claude/linux-role.md`) and the
QA session (`.claude/qa-role.md`) all write here.

A file of its own because a new file conflicts with nothing. Until 2026-10-03
every run appended to the end of [QA-REPORT.md](../QA-REPORT.md), so any two
pull requests open at once conflicted there, and the merger held each one back
an hour. That file stays as the record of the runs before.

What goes in one: the rows run and how each was measured, findings (bugs, rows
that are wrong), `### Proposals`, and `### Votes`. The merger moves the
proposals and findings to TODO.md or QUESTIONS.md (`.claude/merge-role.md`, 4).
