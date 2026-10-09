# 再テスト 4 — ARM64 の「Re-tests of changed behaviour」の続き（e373a0b / 0.85.3、arm レーン、無人の run）

前回（`2026-10-09-arm-retests-2.md`、0.85.2）のあとに入った変更は 0.85.3 の 1 つ（1.35: End the shell? の箱がシェルの名前を言う）だけ。
再テストの行に押せる残りは 1.35 だけだったので、それを押し、`cargo test` を回した。**1.35 は ARM64 で通った。**

## 環境

| 項目 | 値 |
| --- | --- |
| 機械 | Windows 11 Home（build 28000）、aarch64、Qualcomm Adreno X2-90 |
| ビルド | `cargo build --release`（0.85.3）、`filer env` は `Process arch : aarch64`、ConPTY 1.24.260710001 (arm64) を `target\release` に |
| 昇格 | なし |
| 端末 | `FILER_TERM_SHELL=pwsh`、`FILER_TERM_ARGS=-NoProfile` |

## cargo test

`cargo test`（TEMP は scratch）: **741 passed; 0 failed**、もう一つのテストで 1 passed（`test.log`）。
親の無い `OpenConsole` / `pwsh` / `powershell` は前後とも **48**（増えず。人の窓が混じる）。

## 事前の後始末

- ごみ箱: scratch の根（`%TEMP%\filer-scratch`）の下が元の場所のものは **38 件**（全体 65 件）。前の 2 回と同じく**消していない**（0 件）。無人では確認の箱が出ても答える人がいないので、`InvokeVerb` は試さなかった。
- Chrome の窓: 人の窓（`chrome` 31 プロセス）なので触っていない。

## ARM64 の結果

- **1.35 ✓**（行は x64 の `[x]` のまま）`<C-t>` → `ping -n 120 127.0.0.1` を動かして `<C-S-t>`:
  `<State:box>` は `overlay: confirm`、`confirm: End the shell? | `pwsh` is still running in the terminal, and ends with it.`、`confirm keys: [y] / <Enter> End it | [n] Keep it`。
  0.85.2 では「A program is still running…」でプログラム名が無かったが、`pwsh` と出る。
  `n` → `<State:after_n>` は `pane: 12x159`、`focus: pane`、`overlay` の行なし（箱が閉じてペインが残った）。もう一度 `<C-S-t>` → `y` → `<State:ended>` は `pane: closed`、`toast: Ended the shell`。
  証拠: `C:\dev\filer-evidence\arm-retests-3\o135\`（`box.txt`、`after_n.txt`、`ended.txt`、`box.png`、`ended.png`、`keys.done` は `keys: done`）。

## 押さなかった行（前回と同じ理由）

| 行 | 理由 |
| --- | --- |
| 1.44 / 33.7 / 16.13 / 16.13a / 25.4e | 前回の所見のまま。0.85.3 でコードは変わっていない（TODO.md に積み済み）ので、押しても同じになる |
| 49.4 / 49.5 / 49.7 / 49.8 | 帯の色は絵の判定。ブラウザは Chrome が人の窓で開いている。`chafa` / `wezterm` が無い |
| 33.23 / 41.15 | 狭めたときの折り返し・細いつまみは絵の判定 |
| 16.3 / 16.3a / 16.3b | `.docx` と Word が要る |
| 23.4b / 23.4c | UNC のホストが無い |
| 37.8a / 37.8b | 37.8a は #256 が ARM64 で押し済み（`[x]`）。37.8b は Chrome の窓が人のもので開いている |

## 所見

なし（新しい不具合はない）。

### Proposals

1. **TESTING.md の節 50（Claude Code が窓を読む、v0.85.0）を ARM64 の表に入れる。** いまどちらのレーンの表にも節 50 の名前が無く、0 / 7 のまま誰も取らない。
   50.1〜50.7 は `filer mcp` に標準入出力で JSON-RPC を流し、`<State:>` / `FILER_KEYS_DONE` と突き合わせれば、Claude Code を登録しなくても文字で読める行が多い。
   「Unticked rows no queue owns」の行の「押せる」の基準に合うので、その行に「節 50」と一言足すか、独立の行にする（小）。
2. 再テストの行（ARM64 の表）の「Nothing is open in this row」は、いまは 1.35 を押した後なので文のとおり。次の merge で 1.35 を括弧に入れ、残りの行（1.44・33.7・16.13・16.13a・25.4e）は「修正待ち」と書けば、次の run が押し直さずに済む（小）。

### Votes

開いている票（`状態: 投票中`）は origin/main の QUESTIONS.md に無い。票は 0。

## 後始末

起動した `filer.exe` と `ping`、`pwsh` のペインはすべて閉じた。親の無い `OpenConsole` / `pwsh` / `powershell` は前後とも 48。ごみ箱の古い 38 件は残したまま。人の設定は触っていない。
