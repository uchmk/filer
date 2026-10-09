#!/usr/bin/env bash
# Write the per-version part of a release page: one `## vX.Y.Z: ...` heading
# per version commit in RANGE, with the first paragraph of its message under
# it. Prints Markdown on stdout; release.yml appends the platforms note after.
#
#   scripts/release-notes.sh RANGE [BUDGET]
#
# BUDGET (default 100000) is the most bytes this part may take. GitHub refuses a
# release body over 125000 characters, and the platforms note and the SHA-256
# table (release-sums.sh) still have to fit after it: v0.79.0's notes covered
# every version since v0.72.2, came to 124999 characters, and the table was
# never added (v0.80.5). Over the budget, the oldest versions are left out and
# a line at the top says how many and from where; CHANGELOG.md has them all.
# Bytes rather than characters: a byte count is never smaller, so the cap holds.
#
# Kept out of release.yml so that it can be run by hand on any range.
set -euo pipefail
range="${1:?usage: scripts/release-notes.sh RANGE [BUDGET]}"
budget="${2:-100000}"
# A range git cannot read is an error here, not an empty page.
git rev-list "$range" > /dev/null

# Version commits only, which here is not a filter so much as the shape of the
# history: the version bump and the changelog entry are committed together, so
# `vX.Y.Z: ...` is one released change and nothing else is. What it leaves out
# is the traffic in between -- ticking off TESTING-KEYS.md, a regenerated
# checklist -- which carries no message body and would list itself as a
# heading with nothing underneath. Merges go too: their body is the conflict
# list.
#
# Falling back to every non-merge commit if that matches nothing, so a release
# whose commits were named some other way still says something rather than
# coming out blank.
mapfile -t commits < <(git log --reverse --no-merges --format='%H %s' "$range" \
                       | grep -E '^[0-9a-f]+ v[0-9]+\.[0-9]+\.[0-9]+' | cut -d' ' -f1 || true)
[ "${#commits[@]}" -gt 0 ] || mapfile -t commits < <(git log --reverse --no-merges --format=%H "$range")
[ "${#commits[@]}" -gt 0 ] || exit 0

# One section per version, not per commit: a version number on two commits is
# one release of both. That happened when sessions pushed side by side
# (v0.78.29 twice, v0.78.31 three times, 2026-10-05; push-main.sh renumbers
# since v0.78.38), and the page listed the same heading twice or three times.
# The later commits go under the first one's heading, each led by its own
# subject. A section sits where its version first appears.
work=$(mktemp -d)
trap 'rm -rf "${work:?}"' EXIT
declare -A slot=()
keys=()
para() {
  git log -1 --format=%b "$1" | awk '
    /^(Co-Authored-By|Co-authored-by|Claude-Session|Signed-off-by):/ { next }
    NF == 0 { if (seen) exit; next }
    { print; seen = 1 }
  '
}
for sha in "${commits[@]}"; do
  subject=$(git log -1 --format=%s "$sha")
  # The version, or the whole hash for the fallback's unversioned commits.
  key=$(printf '%s\n' "$subject" | grep -oE '^v[0-9]+\.[0-9]+\.[0-9]+' || echo "$sha")
  if [ -z "${slot[$key]+x}" ]; then
    i=${#keys[@]}
    slot[$key]=$i
    keys+=("$key")
    commits[$i]=$sha
    { printf '## %s\n\n' "$subject"; para "$sha"; printf '\n'; } > "$work/$i.md"
  else
    rest=${subject#"$key"}
    rest=${rest#:}
    rest=${rest# }
    { printf '**%s**\n\n' "$rest"; para "$sha"; printf '\n'; } >> "$work/${slot[$key]}.md"
  fi
done
n=${#keys[@]}
# Newest first until the budget runs out, leaving room for the line that says
# what was cut.
room=$((budget - 400))
total=0
first=$n
for ((i = n - 1; i >= 0; i--)); do
  size=$(wc -c < "$work/$i.md")
  if [ $((total + size)) -gt "$room" ]; then break; fi
  total=$((total + size))
  first=$i
done
# Not even the newest fits: it goes in alone, cut to the budget.
if [ "$first" -eq "$n" ]; then
  head -c "$room" "$work/$((n - 1)).md"
  printf '\n\n(The rest of this note is in the commit.)\n\n'
  exit 0
fi
if [ "$first" -gt 0 ]; then
  oldest=$(git log -1 --format=%s "${commits[0]}" | cut -d: -f1)
  last=$(git log -1 --format=%s "${commits[$((first - 1))]}" | cut -d: -f1)
  which=$oldest
  [ "$first" -eq 1 ] || which="$oldest to $last"
  printf '_This release spans %d versions. To fit the release page, the notes of the oldest %d (%s) are left out; CHANGELOG.md has every one._\n\n' \
    "$n" "$first" "$which"
fi
for ((i = first; i < n; i++)); do cat "$work/$i.md"; done
