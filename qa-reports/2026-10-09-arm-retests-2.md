# 再テスト 3 — ARM64 の「Re-tests of changed behaviour」の続き（5c02871 / 0.85.2、arm レーン、無人の run）

前の 2 回（`2026-10-09-arm-retests.md`、`2026-10-09-arm-retests2.md`）が残した行のうち、`--keys`・状態ファイル・クリップボード・`SendInput` のマウスで押せるものを押した。
**49.1・49.2・49.3・49.6・29.12・1.45 は通り、1.44・33.7・1.35・25.4e は食い違いが残る**（所見 1〜4）。

## 環境

| 項目 | 値 |
| --- | --- |
| 機械 | Windows 11 Home（build 28000）、aarch64、Qualcomm Adreno X2-90 |
| ビルド | `cargo build --release`（0.85.2）、ネイティブの aarch64、`adapter: D3D12 (…) (Gl, Other)` |
| 昇格 | なし |
| 設定 | run ごとに `FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` を scratch の `cfg-<名前>` に向けた |
| 端末 | `FILER_TERM_SHELL=pwsh`、`FILER_TERM_ARGS=-NoProfile`（49 は `-NoProfile -NoExit -File init49*.ps1`） |
| マウス | `SetCursorPos` + `SendInput` のボタン（入力デスクトップが `Default`、前面が filer であることを毎回確かめた） |

## cargo test

`cargo test`（TEMP は scratch）: **741 passed; 0 failed** と、もう一つのテストの 1 passed。`test.log`。
親の無い `OpenConsole` / `pwsh` / `powershell` は始めに 49、終わりに **48**（pwsh 26、powershell 22、OpenConsole 0。人の窓が混じる）。

## 事前の後始末

- ごみ箱: scratch の根の下が元の場所のものは 38 件。前回と同じく**消せなかった**（0 件）。やり直していない。
- Chrome の窓: 人の窓なので触っていない。このため 49.5 / 49.7（ブラウザが開く）は押していない。

## ARM64 の結果

- **49.1 ✓** 4 つのコマンド（`dir`、`echo hi`、`Get-Date`、`dir`）のあと `<C-S-Up>` を 5 回: `pane back:` は 14 → 18 → 20 → 45 → 45（5 回目はトースト `No prompt above this one`）。続けて `<C-S-Down>` で 20（`echo hi` の位置）。上の 3 つが `dir` / `Get-Date` / `echo hi`、4 つ目が最初の `dir` で、行の期待どおり。`o491c/`。
  （3 つのコマンドだけでは、プロンプトが表示の中に全部入っていて `No prompt above this one` しか出ない。4 つ目の `dir` で表示から押し出して初めて動く。所見 5。）
- **49.2 ✓** 素の `pwsh -NoProfile`: `<C-S-Up>` → `The shell does not mark its prompts (OSC 133), so there is none to jump to`、`<C-S-l>` → `No finished command to copy (the shell has to mark its prompts, OSC 133)`。`pane back: 0 of 0`。`o492/`。
- **49.3 ✓** `dir` → `<C-S-l>` → トースト `Copied the last command's output (23 lines)`。`clipboard set:` は `ディレクトリ: …` から始まり、`Mode LastWriteTime Length Name` の表だけ。プロンプトの行もコマンドの行も無い。`o493/`。
- **49.4（行は `[ ]` のまま）** 帯の色は絵の判定なので押していない。ただし**行の prompt では失敗も緑になる**（所見 2）。直した prompt（`init49b.ps1`）では `Get-Item nothing-here` の脇が赤（197, 96, 103）、`dir` が緑。`o494/ok2.png` / `err2.png`。
- **49.6 ✓** `fx49\`（`src\main.rs` あり）で `echo src\main.rs:10`、出力の行を Ctrl+クリック → `cwd: …\fx49\src`、`hovered: …\fx49\src\main.rs`。存在しない `nope\x.rs:1` を Ctrl+クリック → `cwd` も `hovered` も動かず、トースト `…\fx49\nope: 指定されたパスが見つかりません。 (os error 3)`。Ctrl 無しのクリックでは動かない。`o496b/`、`o496c/`。
- **49.5 / 49.7（押さず）** Ctrl+クリックで既定のブラウザが開く。Chrome は人の窓が開いているので押していない。下線・ポインタが手になる、も絵の判定。
- **49.8（押さず）** `chafa` も `wezterm` も無い。
- **29.12 ✓** `filer.toml` に `[term] shell = "pwsh"`、`args = ["-NoProfile", "-NoLogo"]`。
  変数あり（`FILER_TERM_SHELL=powershell`）→ トースト `Started powershell (Windows PowerShell 5.1) — <C-t> back to the list`、`filer env` は `powershell : …\powershell.exe …   (terminal pane, from FILER_TERM_SHELL; [term] args not used: -NoProfile -NoLogo)`、`FILER_TERM_SHELL  : powershell`。
  変数なし → トースト `Started pwsh -NoProfile -NoLogo — <C-t> back to the list`、`filer env` は `(terminal pane, from [term] shell)`、`FILER_TERM_SHELL  : unset`。`env2912a.txt` / `env2912b.txt`、`o2912a/`、`o2912b/`。
  この `filer.toml` にはフォントもテーマも書いていないので、行の最後の「他の設定が効いている」は見ていない（#190 / #200 の分）。
- **1.45 ✓** `<C-t>` で 12 行 → 境目を y=300 へ引いて 22 行 → y=650 へ引いて 7 行 → 境目をダブルクリックで 16 行（一覧の上端 55 と状態バーの上端 829 の真ん中は 442、16 行の境目は 427。行の単位に丸まる）→ `<C-t>` 2 回と `<C-S-Enter>` 2 回のあとも **16 行**。窓を 260 px 低くすると 22 行 → 15 行、戻すと 22 行（比率は 0.0256 → 0.0250 → 0.0256）。新しい窓の始めは 12 行（860 px の窓）。`o144c/`、`o145r/`。
- **1.44（行は `[ ]` のまま）** 境目の上でポインタが上下矢印（`IDC_SIZENS`）、一覧の上と、ペインの内側では普通の矢印。境目はポインタに付いてくる（`o144c/`: 引いた先の y と境目の y が一致）。上限: y=60 まで引くと 27 行、境目 y=192 で一覧は y=55〜192（6 行ぶん）。**下限: y=845 まで引くと 3 行（`pane: 3x159`）、境目 y=737。行は 4 行で止まるとある。** 所見 3。`o144e/`。細い線の絵は押していない。
- **33.7 ✗（行は表に無い）** `cfg-337` / `~\.f337cfg` の壊れた `yazi.toml`: トーストは `Config: ~\.f337cfg\yazi.toml: TOML parse error at line 1, column 5 — the rest in `~``。`~` は効いたが、**2 行に折り返して**プレビューの先頭を覆う。所見 4。`o337b/t.png`、`o337/`。
- **1.35 ✗（行は `[ ]` のまま）** `ping -n 120 127.0.0.1` を動かして `<C-S-t>` → `End the shell? / A program is still running in the terminal, and ends with it.`、プログラム名は出ない（前回と同じ）。`n` でペインが残り、再度 `<C-S-t>` → `y` で `Ended the shell`。`o135b/`。なお `Start-Sleep 120`（cmdlet で子のプロセスが無い）では確認が出ずに終わる（1.36 のとおり）。
- **25.4e ✗（行は `[ ]` のまま）** `pwsh`: `v7.6.6.500` と `FileVersion` が合う。`powershell`: `filer env` は `v6.2.28000.2804`、`(Get-Item …).VersionInfo` の `FileVersionRaw` は `10.0.28000.2804`（文字列の `FileVersion` は `10.0.28000.2113 (WinBuild…)`）。前回と同じ。**前回の報告は「ファイルは 10.0.28000.2113」としていたが、数字の版は `10.0.28000.2804` で、`filer env` と違うのは先頭の `6.2` と `10.0` だけ**（所見 6）。`env2912b.txt`。

## 押さなかった行

| 行 | 理由 |
| --- | --- |
| 49.5 / 49.7 | ブラウザが開く。Chrome は人の窓が開いている |
| 49.8 | `chafa` / `wezterm` が無い |
| 33.23 | 窓を狭めたときの折り返しは絵の判定。自動テスト済みの行（TESTING-CHECKS の 604 行目）でもある |
| 41.15 | 細いつまみは絵の判定 |
| 16.13 / 16.13a | 前回の所見のまま（直っていない。再実行しても同じ） |
| 16.3 / 16.3a / 16.3b | `.docx` と Word が要る |
| 37.8b | Chrome の窓が開いている（人のもの） |
| 23.4b / 23.4c | UNC のホストが無い |
| 36.10 | この run では押していない（`<C-F5>` の書き換えが要る。次の run へ） |

## 所見

1. （1.35）前回と同じ。「End the shell?」の箱が動いているものの名前を出さない。直っていない。
2. **49.4: 行の prompt の例では失敗が赤にならない。** `function prompt { $e = [char]27; $c = if ($?) { 0 } else { 1 }; … }` は `$e = …` の代入で `$?` が真に戻ってから読むので、必ず `D;0`（緑）になる。`$ok = $?` を先に取れば赤になる（`init49b.ps1`）。これはドキュメントの直し（TESTING.md の節 49 の冒頭）。**直していない。**
3. **1.44: 下限が 4 行でなく 3 行。** `y=845` まで引くと `pane: 3x159`（境目 y=737、ペインの領域は 92 px）。行と実装のどちらかを揃える。`o144e/c.png`。
4. **33.7: 警告のトーストが 2 行になり、プレビューの先頭を覆う。** `~\.f337cfg\yazi.toml` のように短いパスでも `Config: ~\.f337cfg\yazi.toml: TOML parse error at line 1, column 5 — the rest in `~`` が約 85 文字で、トーストの幅（約 680 px）に収まらない。以前の 3 行から 2 行には減った（v0.85.1）が、行は 1 行を期待している。`o337b/t.png`。
5. 49.1: 行の例は `dir`、`echo hi`、`Get-Date` の 3 つだが、そのままでは 12 行の表示の中に 3 つのプロンプトが全部入り、`<C-S-Up>` は `No prompt above this one` しか言わない。（1 つ目の `dir` は 23 行の出力があるので、`dir` を 2 回入れれば足りる。）行に「ペインに前のプロンプトが表示の上から外れるまで出力を重ねる」と書くとよい。
6. 25.4e: 前回の所見 2 の訂正。`filer env` の `powershell.exe` の版は `v6.2.28000.2804`、ファイルの数字の版は `10.0.28000.2804`。下 2 桁（`28000.2804`）は合っていて、先頭の 2 つが `6.2`（互換性の層が OS の版を 6.2 に偽る）。`FileVersion` の文字列（`10.0.28000.2113 …`）とは別物なので、どちらに合わせるかを決めて行を書き直す必要がある。

### Proposals

1. 1.35 の箱はタイトルが空のとき、起動した実行ファイルの名前（`pwsh`）で言い換える。
2. TESTING.md の節 49 の prompt の例は `$ok = $?` を最初に取る（49.4 が赤になる）。
3. `file_version()` は `filer.exe` に Windows 10 の `supportedOS` manifest を付ける（`6.2` が `10.0` になる）。行は「`VersionInfo.FileVersionRaw` と合う」と書く。
4. 33.7 の警告は 1 行に収める（`the rest in ~` の前を切る。またはトーストの幅を広げる）か、行を「2 行まで」に書き換える。
5. 1.44 の下限は、行と実装のどちらかを 3 行か 4 行に揃える。
6. `--keys` に `<Click:x,y>` と、Ctrl を押しながらのクリックがあると 49.5〜49.7 が `SendInput` 無しで押せる。

### Votes

開いている票（`状態: 投票中`）は origin/main の QUESTIONS.md に無い。票は 0。

## 後始末

起動した `filer.exe` と `ping`、`pwsh` のペインはすべて閉じた。親の無い `OpenConsole` / `pwsh` / `powershell` は始め 49、終わり 48。ごみ箱の古い 38 件は残したまま。
証拠は `C:\dev\filer-evidence\arm-retests-2\`（`o*/` の状態ファイルと絵、`s*.ps1`、`harness.ps1`、`mouse.ps1`、`env2912*.txt`、`test.log`）。人の設定は触っていない。
