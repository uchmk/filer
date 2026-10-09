#!/usr/bin/env bash
# Write a whole release page for TAG, built at REF: the per-version notes
# (scripts/release-notes.sh), the platforms note, and the link to CHANGELOG.md.
# Prints Markdown on stdout. The SHA-256 table goes on afterwards
# (release-sums.yml).
#
#   scripts/release-body.sh TAG [REF] [REPO]
#
# REF defaults to TAG. release.yml passes HEAD, because on that run the tag
# is created with the release and does not exist yet. release-sums.yml runs
# it on an existing tag to write a page again (v0.79.0's, cut off before its
# table): the platforms note is then today's, not the one the tag shipped
# with -- the same text in v0.79.0's case.
#
# REPO (owner/name) defaults to $GITHUB_REPOSITORY.
set -euo pipefail
tag="${1:?usage: scripts/release-body.sh TAG [REF] [REPO]}"
ref="${2:-$tag}"
repo="${3:-${GITHUB_REPOSITORY:?set GITHUB_REPOSITORY or pass REPO}}"
here=$(dirname "$0")

# The previous tag, not counting this one: `REF^` is what keeps `describe`
# from answering with the tag being released when REF is already tagged.
prev=$(git describe --tags --abbrev=0 "$ref^" 2>/dev/null || true)
# With no previous tag there is no range -- and the whole history is the
# wrong answer, so it is this version's own commit.
range="${prev:+$prev..}$ref"
[ -n "$prev" ] || range="$ref~1..$ref"
echo "notes from $range" >&2

# The versions in the range, newest kept when they would not all fit on the
# page (v0.79.0 lost its SHA-256 table that way).
notes=$(bash "$here/release-notes.sh" "$range")
# Checked before the footer is appended, since the footer alone would make an
# empty set of notes look like a full one.
if [ -z "$notes" ]; then
  printf 'No commits between %s and %s.\n\n' "${prev:-the start}" "$tag"
else
  printf '%s\n' "$notes"
fi

# The platforms note. It is here and not in a commit message because it is
# true of the download rather than of the change, so it belongs on every
# release and in none of the history.
cat <<'NOTE'

### Platforms

**Windows is the platform this is tested on.** The Linux build is started on
every push -- CI opens it on a virtual display and checks that a frame was
actually painted -- but that is a proof of life, not of correctness: nobody
works through the checklist on it. **The macOS builds are only compiled and
linked**, and that is the whole of what is known about them. They are here so
that somebody can start them, not because they are known to work.

Two things are known to be missing there, by design rather than by accident,
because the Windows shell is what supplies them: the thumbnail for formats
filer does not decode itself (HEIC, AVIF, PDF, video) and the listing of a file
server's shares. Both say so rather than failing quietly. PDF and video can be
previewed anyway with a `[[preview]]` rule; see the README.

### Signing

**Nothing here is code-signed**, so every platform will say so in its own way and
you should expect it rather than be alarmed by it. Windows shows the publisher as
unknown, and SmartScreen adds a second warning because a file published today has
no download history to weigh; this happens on every machine, personal or managed.
macOS is the same with a different name -- Gatekeeper refuses the binaries until
they are allowed through by hand.

What those warnings ask you to confirm, you can check. Every asset's SHA-256 is
in the table at the end of this page, and comparing it tells you the file is the
one CI built from this tag's commit and that nothing altered it on the way:

```powershell
Get-FileHash .\filer-...-windows-x64.zip -Algorithm SHA256 | Format-List Hash
```

A matching hash does not remove the warning. Only a certificate does, and there
is not one.

The macOS and Linux downloads are `.tar.gz` because a release asset does not carry
the executable bit and an archive does.

The Windows downloads are `.zip`: `filer.exe` comes with `conpty.dll` and
`OpenConsole.exe`, a newer ConPTY from Microsoft's own package (MIT, notice
included), and `filer.com`, which makes `filer env` and `filer --version`
typed in a terminal behave like any console command. The table at the end
lists each zip's files with their hashes.
**Keep them together.** filer runs without the others, but on
the older ConPTY built into Windows, programs in the terminal pane such as
lazygit misread keys.
NOTE

# Pointing at the tag rather than at `main`, so the link keeps saying what this
# version said once main has moved on. Written with printf rather than in the
# heredoc above, which is quoted and would not expand the names.
printf '\n---\n\n%s\n' \
  "詳細は [CHANGELOG.md](https://github.com/$repo/blob/$tag/CHANGELOG.md) にあります（日本語）。"
