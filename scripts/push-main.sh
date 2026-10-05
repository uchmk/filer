#!/usr/bin/env bash
# Push this checkout's own commits to main, renumbered onto whatever main has
# become, verified, without a merge commit and without --force.
#
#   scripts/push-main.sh
#
# Commit as usual first: one commit per change, its version bumped in
# Cargo.toml / Cargo.lock, its CHANGELOG.md section, a `vX.Y.Z: ` subject.
# Then run this instead of `git merge origin/main` + `git push`.
#
# Why (2026-10-05): the development routine, the merge routine and an
# interactive session pushed to main within ten minutes, each merged the
# others in and renumbered in the merge commit. The version came out right,
# but three commits kept the subject `v0.78.31:` and two `v0.78.29:`, and the
# release notes are built from those subjects (release.yml skips merges).
#
# What it does, up to five times until a push lands:
#   1. fetch main; if main is not under HEAD (or HEAD holds a merge of it),
#      replay each of HEAD's own non-merge commits onto main, one by one:
#      the version gets the same kind of bump on top of main's, and the
#      commit's `vOLD` becomes `vNEW` in its message, its CHANGELOG heading
#      and section, and in the lines it adds (TODO.md's `（vX.Y.Z。…）` ticks);
#   2. scripts/verify.sh on the result;
#   3. fetch again and start over if main moved meanwhile; else push.
#
# The commits replayed have never been pushed, so this rewrites nothing
# anyone else has seen; CLAUDE.md's merge-commit rule is about branches
# merged into main, which keep their merge.
#
# It stops, and changes nothing, when a commit's change does not apply on
# the new main (a real conflict, usually in TODO.md) or touches Cargo.toml /
# Cargo.lock beyond the version. Then: `git rebase origin/main`, resolve by
# hand, fix the version, the CHANGELOG heading and the subject, run it again.

set -euo pipefail
cd "$(dirname "$0")/.."

die() { echo "push-main: $*" >&2; exit 1; }

[ -z "$(git status --porcelain --untracked-files=no)" ] || die "uncommitted changes; commit or restore them first"

replay() {
    python3 - <<'PY'
import os, re, subprocess, sys, tempfile

def git(*args, cwd=None, input=None, check=True):
    r = subprocess.run(["git", *args], cwd=cwd, input=input, capture_output=True)
    if check and r.returncode != 0:
        sys.exit(f"push-main: git {' '.join(args)} failed:\n{r.stderr.decode(errors='replace')}")
    return r

def out(*args, cwd=None):
    return git(*args, cwd=cwd).stdout.decode()

def show(rev, path):
    r = git("show", f"{rev}:{path}", check=False)
    return r.stdout.decode() if r.returncode == 0 else None

VERSION = re.compile(r'^version = "(\d+)\.(\d+)\.(\d+)"', re.M)

def version(text):
    m = VERSION.search(text)
    return tuple(int(x) for x in m.groups())

def vs(v):
    return ".".join(map(str, v))

def bump(tip, base, own):
    if own == base:
        return None
    if own[0] != base[0]:
        return (tip[0] + 1, 0, 0)
    if own[1] != base[1]:
        return (tip[0], tip[1] + 1, 0)
    return (tip[0], tip[1], tip[2] + 1)

def renamed(text, old, new):
    return re.sub(r"(?<![\w.])v" + re.escape(old) + r"(?![\d])", "v" + new, text)

commits = out("rev-list", "--reverse", "--no-merges", "origin/main..HEAD").split()
if not commits:
    sys.exit("push-main: nothing of this checkout's own is missing from main")

wt = tempfile.mkdtemp(prefix="push-main-")
git("worktree", "add", "-q", "--detach", wt, "origin/main")
try:
    for c in commits:
        subject = out("log", "-1", "--format=%s", c).strip()
        base = version(show(f"{c}^", "Cargo.toml"))
        own = version(show(c, "Cargo.toml"))
        tip = version(open(os.path.join(wt, "Cargo.toml"), encoding="utf-8", newline="").read())
        new = bump(tip, base, own)

        # Cargo.toml and Cargo.lock may differ only in filer's own version.
        for f in ("Cargo.toml", "Cargo.lock"):
            for line in out("diff", "-U0", f"{c}^", c, "--", f).splitlines():
                if line.startswith(("+++", "---", "@@", "diff ", "index ")):
                    continue
                if not re.fullmatch(r'[+-]version = "\d+\.\d+\.\d+"', line):
                    sys.exit(f"push-main: {c[:7]} ({subject}) changes {f} beyond the version; replay it by hand")

        skip = [":(exclude)Cargo.toml", ":(exclude)Cargo.lock"]
        if new:
            skip.append(":(exclude)CHANGELOG.md")
        patch = git("diff", "--binary", f"{c}^", c, "--", ".", *skip).stdout
        if new and new != own:
            lines = patch.split(b"\n")
            for i, line in enumerate(lines):
                if line.startswith(b"+") and not line.startswith(b"+++"):
                    lines[i] = renamed(line.decode(), vs(own), vs(new)).encode()
            patch = b"\n".join(lines)
        if patch.strip():
            r = git("apply", "--3way", "--index", "--whitespace=nowarn", cwd=wt, input=patch, check=False)
            if r.returncode != 0:
                sys.exit(f"push-main: {c[:7]} ({subject}) does not apply on the new main:\n"
                         f"{r.stderr.decode(errors='replace')}\n"
                         "Nothing was changed. `git rebase origin/main`, resolve, fix the version, "
                         "the CHANGELOG heading and the subject, and run this again.")

        if new:
            mine, before = show(c, "CHANGELOG.md"), show(f"{c}^", "CHANGELOG.md")
            m = re.search(r"^## \[" + re.escape(vs(own)) + r"\][^\n]*\n.*?(?=^## \[|\Z)", mine, re.S | re.M)
            if not m or mine[:m.start()] + mine[m.end():] != before:
                sys.exit(f"push-main: {c[:7]} ({subject}) changes CHANGELOG.md other than adding [{vs(own)}]; replay it by hand")
            section = renamed(m.group(0).replace(f"## [{vs(own)}]", f"## [{vs(new)}]", 1), vs(own), vs(new))
            path = os.path.join(wt, "CHANGELOG.md")
            log = open(path, encoding="utf-8", newline="").read()
            at = re.search(r"^## \[\d", log, re.M)
            log = log[:at.start()] + section + log[at.start():] if at else log + "\n" + section
            open(path, "w", encoding="utf-8", newline="").write(log)

            for f, pat in (("Cargo.toml", r'^(version = ")[\d.]+(")'),
                           ("Cargo.lock", r'^(name = "filer"\nversion = ")[\d.]+(")')):
                path = os.path.join(wt, f)
                text = open(path, encoding="utf-8", newline="").read()
                text, n = re.subn(pat, r"\g<1>" + vs(new) + r"\g<2>", text, count=1, flags=re.M)
                if n != 1:
                    sys.exit(f"push-main: could not set the version in {f}")
                open(path, "w", encoding="utf-8", newline="").write(text)
            git("add", "CHANGELOG.md", "Cargo.toml", "Cargo.lock", cwd=wt)

        message = out("log", "-1", "--format=%B", c)
        if new and new != own:
            message = renamed(message, vs(own), vs(new))
        env = dict(os.environ)
        for key, fmt in (("GIT_AUTHOR_NAME", "%an"), ("GIT_AUTHOR_EMAIL", "%ae"), ("GIT_AUTHOR_DATE", "%aD")):
            env[key] = out("log", "-1", f"--format={fmt}", c).strip()
        r = subprocess.run(["git", "commit", "-q", "--allow-empty", "-F", "-"], cwd=wt, env=env,
                           input=message.encode(), capture_output=True)
        if r.returncode != 0:
            sys.exit(f"push-main: commit failed:\n{r.stderr.decode(errors='replace')}")
        said = f"v{vs(own)} -> v{vs(new)}" if new and new != own else "kept its version"
        print(f"push-main: {c[:7]} {said}: {subject}")

    head = out("rev-parse", "HEAD", cwd=wt).strip()
finally:
    git("worktree", "remove", "--force", wt, check=False)

git("reset", "-q", "--keep", head)
print(f"push-main: replayed onto main as {head[:7]}")
PY
}

needs_replay() {
    ! git merge-base --is-ancestor origin/main HEAD || [ -n "$(git rev-list --merges origin/main..HEAD)" ]
}

for attempt in 1 2 3 4 5; do
    git fetch -q origin main
    [ -n "$(git rev-list origin/main..HEAD)" ] || { echo "push-main: nothing to push"; exit 0; }
    if needs_replay; then
        replay
    fi
    scripts/verify.sh
    git fetch -q origin main
    if needs_replay; then
        echo "push-main: main moved during the checks; again (attempt $attempt)"
        continue
    fi
    if res=$(git push origin HEAD:main 2>&1); then
        echo "$res" | tail -1
        git log -1 --format='push-main: pushed %h %s'
        exit 0
    fi
    echo "$res" | grep -qE 'rejected|fetch first|non-fast-forward' || die "push failed: $res"
    echo "push-main: main moved before the push; again (attempt $attempt)"
done
die "main kept moving; five tries and no push"
