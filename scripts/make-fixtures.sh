#!/usr/bin/env bash
# Build the files TESTING.md's checklist needs, under one directory -- the
# Linux lane's counterpart of make-fixtures.ps1 (see .claude/linux-role.md).
#
# The same files under the same names, so a row written against the Windows
# fixtures reads the same here. Safe to run again: the directory is emptied
# first, and nothing is written outside it.
#
#   scripts/make-fixtures.sh [DIR]      # default: $TMPDIR/filer-fixtures
#
# Needs bash, git, zip and ImageMagick's `convert`; a missing one skips its
# group with a warning rather than stopping the rest.

set -u
root="${1:-${TMPDIR:-/tmp}/filer-fixtures}"
if [ -e "$root" ]; then
    echo "Clearing $root"
    rm -rf -- "$root"
fi
mkdir -p -- "$root"
root="$(cd -- "$root" && pwd)"
echo "Building fixtures in $root"
echo

short=0
warn() { echo "WARNING: $*" >&2; }
# What a group actually left on disk, against what it meant to make (#111).
count() {
    local dir="$1" want="$2" label="$3" got
    got=$(find "$dir" -mindepth 1 -maxdepth 1 | wc -l)
    if [ "$got" -ne "$want" ]; then
        short=$((short + 1))
        warn "$label: $got entries on disk, expected $want"
    fi
}

# --- text: long enough that the minimap has to compress.
for i in $(seq 1 4000); do
    case $((i % 40)) in
        0) echo ;;
        1) printf '// ---- section %d %s\n' $((i / 40)) "$(printf -- '-%.0s' $(seq 1 50))" ;;
        2) echo "fn block_$i() {" ;;
        39) echo "}" ;;
        *) printf '%*slet value_%d = compute(%d);\n' $((4 * (1 + i % 3))) '' "$i" "$i" ;;
    esac
done > "$root/long.rs"
echo '  long.rs              4000 lines, for the minimap and preview scrolling'

# --- two files a line apart, for the compare view (<A-d>).
for i in $(seq 1 200); do echo "line $i"; done > "$root/compare-left.txt"
{
    for i in $(seq 1 150); do
        if [ "$i" -eq 100 ]; then echo 'line 100 -- CHANGED'; else echo "line $i"; fi
    done
    echo 'line 150.5 -- INSERTED'
    for i in $(seq 151 200); do echo "line $i"; done
} > "$root/compare-right.txt"
echo '  compare-left/right   200 lines, one changed and one inserted'

# --- word-level compare rows (5.11): one word changed, a Japanese word
# changed (set off by spaces, as the word split needs), a line with nothing in
# common, and one that is the same.
printf '%s\n' 'the price is firm' '宛先 太郎 さんです' 'alpha beta' 'unchanged line' > "$root/words-left.txt"
printf '%s\n' 'the cost is firm' '宛先 花子 さんです' 'gamma delta' 'unchanged line' > "$root/words-right.txt"
echo '  words-left/right     4 lines: one word, one Japanese word, nothing in common, same'

echo identical > "$root/same-a.txt"
echo identical > "$root/same-b.txt"
for i in $(seq 1 512); do printf "\\$(printf '%03o' $((i % 256)))"; done > "$root/binary.dat"
echo '  same-a/b, binary.dat identical pair, and one that is not text'

# An extension no machine associates with an app (16.13). A machine with HKCR\.xyz cannot run that row.
echo 'no default app for this' > "$root/unknown.xyz"
echo '  unknown.xyz          no default app (16.13; needs HKCR\.xyz absent)'

cat > "$root/notes.md" <<'MD'
# Title

Body text that should wrap when the pane is narrow, and stay put when it is wide.

## A heading

- a list item
- another one

```rust
fn fenced() -> u32 { 42 }
```

## Another heading

> a quote

| a | b |
| --- | --- |
| 1 | 2 |
MD
echo '  notes.md             rendered/source toggle, outline, no minimap when rendered'

# --- pictures: a fine grid big enough to zoom into, and one too small to fit up.
if command -v convert >/dev/null; then
    convert -size 3200x2400 xc:'rgb(24,26,38)' -fill none -stroke 'rgb(90,200,255)' \
        -draw "$(for x in $(seq 0 8 3199); do printf 'line %d,0 %d,2399 ' "$x" "$x"; done)" \
        -draw "$(for y in $(seq 0 8 2399); do printf 'line 0,%d 3199,%d ' "$y" "$y"; done)" \
        -fill white -stroke none -pointsize 120 -annotate +60+180 '3200 x 2400
8px grid' "$root/zoom-me.png"
    convert -size 48x48 xc:goldenrod "$root/tiny.png"
    echo '  zoom-me.png, tiny.png  a grid to zoom into, and 48x48 that fit must not blow up'
else
    warn 'convert (ImageMagick) is missing: skipping zoom-me.png and tiny.png'
fi

# --- something to pack and unpack (`e` / `E`), plus a ready-made archive.
mkdir -p "$root/to-pack/nested"
for i in 1 2 3 4 5; do echo "contents $i" > "$root/to-pack/file$i.txt"; done
echo deep > "$root/to-pack/nested/deep.txt"
if command -v zip >/dev/null; then
    (cd "$root/to-pack" && zip -qr "$root/sample.zip" .)
    echo '  to-pack/, sample.zip pack with E, unpack with e, and preview the listing'
else
    warn 'zip is missing: skipping sample.zip'
fi

# --- names that have broken things before. Linux folders are case-sensitive,
# --- so the case pair is two files here, as 24.3 wants.
awkward="$root/awkward names"
mkdir -p "$awkward"
long_name="very-$(printf 'long-%.0s' $(seq 1 30))name.txt"
for name in 'a file with spaces.txt' 'ひらがなとカタカナ.txt' "quote'in-name.txt" "$long_name" 'UPPER.TXT' 'upper.txt'; do
    printf '%s\n' "$name" > "$awkward/$name"
done
echo '  awkward names/       spaces, CJK, a quote, a very long one, case pairs'
count "$awkward" 6 'awkward names'

# --- files to rename in bulk (`R`), including a pair to swap.
ren="$root/bulk-rename"
mkdir -p "$ren"
for i in $(seq 1 12); do echo x > "$ren/$(printf 'IMG_%04d.jpg' "$i")"; done
echo ab > "$ren/ab.txt"
echo ba > "$ren/ba.txt"
echo blocker > "$ren/in the way.txt"
echo '  bulk-rename/         IMG_0001..0012, ab/ba to swap, a name in the way'
count "$ren" 15 'bulk-rename'

# --- a repository with every state the git signs are drawn for.
if command -v git >/dev/null; then
    repo="$root/repo"
    mkdir -p "$repo/sub"
    (
        cd "$repo" || exit 1
        git init --quiet
        git config user.email 'fixtures@example.invalid'
        git config user.name 'Fixtures'
        echo 'committed and untouched' > clean.txt
        echo before > modified.txt
        echo 'to be removed' > deleted.txt
        echo before > sub/inside.txt
        git add -A && git commit --quiet -m fixtures
        echo after > modified.txt
        echo after > sub/inside.txt
        echo 'new and staged' > staged.txt
        git add staged.txt
        rm deleted.txt
        echo 'never added' > untracked.txt
        mkdir -p untracked-dir
        echo a > untracked-dir/a.txt
        echo b > untracked-dir/b.txt
    )
    echo '  repo/                clean / M / + / D / ? and an untracked directory'
else
    warn 'git is missing: skipping the repository fixture'
fi

# --- a directory big enough to scroll.
many="$root/many"
mkdir -p "$many"
for i in $(seq 1 500); do echo "$i" > "$many/$(printf 'item-%03d.txt' "$i")"; done
echo '  many/                500 entries, for scrolling and select-all'
count "$many" 500 'many'

if [ "$short" -gt 0 ]; then
    warn "$short group(s) did not come out as intended; see above"
fi
echo
echo "Done. Point filer at:"
echo "  $root"
