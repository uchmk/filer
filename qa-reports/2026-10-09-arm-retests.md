# 再テスト — ARM64 の「Re-tests of changed behaviour」（88414f4 / 0.83.1、arm レーン、無人の run）

順番表の先頭で開いていた行のうち、`--keys` と状態ファイルで足りるものを押した。16.13 / 16.13a は **ARM64 では失敗**した（所見 1）。
押せなかった行は末尾に理由を付けて並べた。

## 環境

| 項目 | 値 |
| --- | --- |
| 機械 | Windows 11 Home（build 28000）、aarch64、Qualcomm Adreno X2-90 |
| ビルド | `cargo build --release`（88414f4、0.83.1）、ネイティブの aarch64、`adapter: D3D12 (…) (Gl, Other)` |
| 昇格 | なし（`IsInRole(Administrator)` = False）。昇格が要る行は押していない |
| 設定 | run ごとに `FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` を scratch の `cfg-<名前>` に向けた |
| 画面 | キーは `--keys` だけ。表示の倍率は変えていない（`scale: 100%`） |

## cargo test

`736 passed; 0 failed`。親の無い `OpenConsole` / `pwsh` / `powershell` は前後とも **48**（始めの数と同じ）。

## ARM64 の結果

- **16.13 ✗** `unknown.xyz` に `<Enter>`（オープナー無し）→ `No default app for .xyz` ではなく `Opened unknown.xyz with the system's default app`。Windows の「アプリを選ぶ」の箱（`OpenWith.exe`）が出て、`--keys` は 31 秒止まった。`r1613/`。所見 1。
- **16.13a ✗** `start "" %*` のオープナーの後ろに別のオープナーがある `unknown.xyz` → `No default app … — <S-Enter> to pick one` ではなく、`start` がそのまま走って `launched: … start "" "…\unknown.xyz"`。`r1613a/`。
- **45.19 ✓** 比較 `C` → `clipboard set: differ<TAB>differ.txt / left only … / same<TAB>sub\s.txt`、トースト `Copied 6 rows`。`z` で一致を隠した後は 3 行（`Copied 3 rows`）。比較の外では `Copy: only a folder comparison has rows to copy`。`r4519a/b/c/`。
- **33.6 ✓** 明るい下地（`overall = { bg = "#ffffff" }`）で警告と `.txt` の名前の色をピクセルで測り、WCAG のコントラスト比で読める濃さだった（警告は濃い琥珀、名前は濃い灰）。`r336/`、`r336c/`、`r336d/`。
- **33.9 ✓** 設定 3 本の警告 → トーストは 1 行で `… TOML parse error at line 1, column 5 — the rest and 2 more in `~``。1 本のときは `— the rest in `~``。`r337/`、`r339/`。
- **33.7 部分** 5 行は `~` と `filer env` に出た。トーストが 3 行に折れたとき、プレビューの先頭が隠れた（所見 2）。行は `[ ]` のまま。
- **47.9 ✓** `<Wait:16000>` の間の CPU を読んだ（`r479.done`、`r479b.done`）。
- **32.24 ✓** `pick: Pre opener | Mid opener | App opener`、`pick runs: echo pre %s [prepend] | echo mid %s | echo app %s [append]`。詳細の列にも ` [prepend]` / ` [append]` が付き、`rules` の分は付かない（`r3224/pick.png`）。
- **32.21 ✓** `pick: Bad tool (not found) | Typo tool (not found) | Good tool`、`picked: Good tool`（カーソルは入っている最初の行）。素の `<Enter>` は `… (skipped no-such-tool: not found)`。入っていない行を選ぶと `launch failed: no-such-tool …`。`r3221/`、`r3221b/`。
- **32.10 ✓** `Open failed: `Hidemruu.exe` was not found — …`。`cmd /c exit 1` では `Open failed: exit code 1 — …`。`r3221c/`、`r3210/`。
- **32.20 ✓** `out.txt` は `hello` だけ。トースト `$ echo hello >out.txt (no path: the line has a shell operator; use %* to place it)`、`;echo %*` は注記無しでパス入り、`;type nothing.txt >t.txt` の `Command failed: …` にも同じ注記。`r3220/`。
- **36.10 ✓** `<S-t>` に `quit` を割り当てて `<C-F5>` → `T` を押しても filer は続いた（`quit: yes` は `<Quit>` の分のみ。`T` の後の `<State:b>` が書かれた）。トーストと `filer env` の `Warnings` に `` `<S-t>` can never be pressed: … write `T` ``。`T` が既定の動きをするところまでは見ていない。`r3610/`。
- **1.46 `[~]`** `less` を動かすと `<State:>` に `pane badge: <C-t> list`、`q` の後は無い（`r146/afterq.txt`）。右上の札の絵は `r146/less-crop.png`（薄い下地に淡い字で `<C-t> list`、`less` の字に重ならない）。失敗の形: 札が `less` の字に重なる、読めない、`q` の後も残る。

## 押さなかった行

| 行 | 理由 |
| --- | --- |
| 37.8b | Chrome の窓が開いている（人のもの）。閉じない |
| 23.4b / 23.4c | UNC のホストが無い |
| 16.3 / 16.3a / 16.3b, 32.23 / 32.23a, 21.14, 21.19, 36.10 以外の 36 節、1.35, 1.38, 1.44 / 1.45, 29.12, 25.4e / 25.4f, 46.17 / 46.17b, 46.19 / 46.20, 41.15, 33.23, 32.20 以外の 32 節の残り, 23.6, 49.1–49.8 | この run の時間切れ。次の run に残す |
| 20 回の `--keys` 消えの数え | 前回から tsumugi-pane の `rev` は動かず、リリースも無い（docs/claude/build.md の条件に当たらない） |

## 所見

1. **16.13 / 16.13a は ARM64 で失敗する。** `has_default_app`（`src/exec.rs:711`）は `AssocQueryStringW(ASSOCSTR_EXECUTABLE, ".xyz")` が成功すれば「アプリがある」と見る。この機械（build 28000）では filer の中だと、関連付けの無い `.xyz` でも `C:\WINDOWS\system32\OpenWith.exe` を返して成功する（`.zzqq` も同じ。`.txt` は `Notepad.exe`）。つまり「アプリが無い」判定に落ちず、`start` が「アプリを選ぶ」の箱を出す。x64 では v0.82.0 で通っているので、機械か OS の版による差。**直していない。**
2. 設定ファイルが長い scratch のパスにあると、警告のトーストが 3 行に折れてプレビューの先頭を覆う（33.7 の「1 行」に反する）。通常のパスでは起きないが、`~` の短縮を使う手もある。

### Proposals

1. `has_default_app` は `OpenWith.exe`（と `rundll32.exe` 系）を「アプリ無し」として扱う。x64 でも同じ値が出るか確かめてから。（TODO に足す案）
2. 警告のトーストの 1 行目はパスを `~` や相対に短縮してから折る。

### Votes

開いている票は無い。

## 後始末

起動した `filer.exe` と `OpenWith.exe` はすべて閉じた。親の無い `OpenConsole` / `pwsh` / `powershell` は 48（始めと同じ）。証拠は `C:\dev\filer-evidence\arm-retests\`。人の設定は触っていない。
