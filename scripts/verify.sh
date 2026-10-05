#!/usr/bin/env bash
# Every check a push to main must pass, in the order CI would fail them, from
# a Linux machine (a cloud session). The first failure stops with its output.
#
#   scripts/verify.sh
#
# Until 2026-10-04 this lived in one session's scratch folder, and the steps
# were written out again in each routine's instructions. Before that, a chain
# of commands that only looked at the last one's exit code pushed v0.78.13
# with a test build that did not compile (CLAUDE.md, 自動実行モード).
#
# The Windows clippy is a type check only -- it never runs Windows code, so
# read path strings by eye as CLAUDE.md asks. Never add `cargo fmt` here.

set -o pipefail
cd "$(dirname "$0")/.." || exit 1

rustup target list --installed 2>/dev/null | grep -qx x86_64-pc-windows-msvc \
    || rustup target add x86_64-pc-windows-msvc >/dev/null 2>&1

run() {
    local out rc
    out=$("$@" 2>&1)
    rc=$?
    if [ $rc -ne 0 ]; then
        echo "FAILED: $*"
        echo "$out" | tail -30
        exit 1
    fi
    last=$out
}

# First, and `--locked`: CI builds that way, and a plain cargo command quietly
# rewrites a stale Cargo.lock, so the commit that bumped Cargo.toml without it
# passed every later step here (v0.78.42 to v0.78.44 went red on main). When
# this fails, run `cargo build`, then commit Cargo.lock with the version bump.
run cargo build -q --locked
run cargo test -q
tests=$(echo "$last" | grep -m1 'test result')
run cargo +stable clippy -q --all-targets -- -D warnings
run cargo +stable clippy -q --all-targets --target x86_64-pc-windows-msvc -- -D warnings
run cargo run -q --example make-testcheck -- --check
run cargo run -q --example make-testcheck -- --lane linux --check
run cargo run -q --example make-keycheck -- --check
echo "ALL OK: $tests"
