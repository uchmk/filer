# 再テスト 2 — ARM64 の「Re-tests of changed behaviour」の続き（eeff93c / 0.84.3、arm レーン、無人の run）

前回（`2026-10-09-arm-retests.md`）が時間切れで残した行のうち、`--keys` と状態ファイル・クリップボードで足りるものを押した。
押した行の多くは通ったが、**1.35 と 25.4e は ARM64 で食い違い**があった（所見 1・2）。

## 環境

| 項目 | 値 |
| --- | --- |
| 機械 | Windows 11 Home（build 28000）、aarch64、Qualcomm Adreno X2-90 |
| ビルド | `cargo build --release`（0.84.3）、ネイティブの aarch64、`adapter: D3D12 (…) (Gl, Other)` |
| 昇格 | なし |
| 設定 | run ごとに `FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` を scratch の `cfg-<名前>` に向けた |
| 端末 | `FILER_TERM_SHELL=pwsh`、`FILER_TERM_ARGS=-NoProfile`（ペインの行のみ） |

## cargo test

親の無い `OpenConsole` / `pwsh` / `powershell` は前後とも **48**（前後で同じ）。

## 事前の後始末

- ごみ箱: 始めに scratch の根（`…\filer-scratch\`）の下が元の場所のものは **38 件**（全体 65 件）。**消せなかった**（0 件）。
  `Shell.Application` の `InvokeVerb('delete')` は確認のダイアログ（`ファイルの削除`）で止まり、`$Recycle.Bin` の中身を `Remove-Item` で消すのは実行環境に止められた。ダイアログは閉じた。終わりの数も 38。
- Chrome の `file:///` の窓: 人の窓なので触っていない。数えていない。

## ARM64 の結果

- **1.38 ✓** シェルを開いて `<C-t>` で一覧に戻り `<C-S-t>` → pane は閉じ、トースト `Ended the shell`。続けて `<C-S-t>` → `No terminal to close`。`toasts shown:` は `Ended the shell | No terminal to close` で、`Started …` は表に出ていない（`toasts:` の履歴には `Started pwsh -NoProfile — <C-t> back to the list` が残る）。`o138/`。
- **1.35 ✗（行は `[ ]` のまま）** `ping -t 127.0.0.1` を動かして `<C-S-t>` → 箱は **`End the shell?` / `A program is still running in the terminal, and ends with it.`**。行の期待は「何が動いているかをプログラム名（`pwsh`）で言う」。名前が出ていない。`n` で pane は残り（`overlay: none`、`focus: pane`）、`y` で pane が閉じて `Ended the shell`。名前の無い文言になったのは、`pwsh -NoProfile` のタイトルが取れなかったためか（ConPTY のタイトルが空）は未確認。`o135/dlg.png`。所見 1。
- **21.14 ✓** `to-pack\` の 3 つ（`f1.txt`、`inner\`、`inner\f2.txt`）と `to-pack\` 自身を `2021-06-15 12:34:57`（奇数秒）にして、`.zip` / `.tar.gz` / `.7z` に `E`、それぞれ別の `u-*\` に `e`。展開された `to-pack\`・`inner\`・2 つのファイルの `LastWriteTime` は 3 形式とも `12:34:57` のまま。`o2114-*/`。
- **25.4f ✓** `[ui]` 無し → `Backend  : [ui] backend = "auto" (this machine: Gl)`。`backend = "gl"` → `Backend  : [ui] backend = "gl"`（尾無し）。所見 3。
- **32.23a ✓** `;nosuchprog-xyz` → `Command failed: `nosuchprog-xyz` was not found — nosuchprog-xyz "…\a.txt"`。`o3223/`。
- **46.17 ✓** spot で `C` → クリップボードを `Get-Clipboard -Raw` で全文読んだ。節の題の下に `Label<TAB>value`、節の間は空行、`Came in via<TAB>#7<TAB>d0ea2a0`（3 列）、トースト `Copied the spot panel: 20 rows`。`o4617f/`。
- **46.17b ✓** 3 つ選んで `C` → 先頭が `#### <full path>`（`base.txt`、`pr.txt`、`sub`）の 3 つの塊、塊の前に空行 2 つ、トースト `Copied the spot panels of 3 files`。`o4617g/`。
- **25.4e ✗（行は `[ ]` のまま）** `pwsh`: `v7.6.6.500`、`(Get-Item …).VersionInfo.FileVersion` も `7.6.6.500` で合う。`powershell`: `filer env` は `v6.2.28000.2804`、ファイルは `10.0.28000.2113`（`FileVersion`）。所見 2。
- **29.12 部分（行は `[ ]` のまま）** `[term] shell = "pwsh"`、`args = ["-NoLogo"]` で、変数 `FILER_TERM_SHELL=powershell` → トースト `Started powershell (Windows PowerShell 5.1) — …`、`filer env` は `powershell … (terminal pane, from FILER_TERM_SHELL; [term] args not used: -NoLogo)`、`FILER_TERM_SHELL : powershell`。変数無し → `Started pwsh -NoLogo — …`、`pwsh … (terminal pane, from [term] shell)`。フォントと `theme.toml` が両方で効いたかは見ていない。`o2912a/`、`o2912b/`。
- **33.23 部分（`[ ]` のまま）** `~` で help が開き、`C` のクリップボードは警告の行を含む（状態ファイルは切り詰めるため全文は見ていない）。窓を狭めたときの折り返しは絵の判定なので押していない。`o3323b/help.png`。
- **41.15 部分（`[ ]` のまま）** spot の `<A-j>` で `spot top: 2 of 19`、収まる側は `fit.txt`。細い目盛りの絵は押していない。`o4115c/`。
- **21.19 / 21.22d / 23.6 / 46.19 / 46.20** は既に `[x]` かテストの分。ここでも押して同じ結果だった（`o2119/`、`o2122b/`、`o236_tpyo/`、`o4619*` は前回の手元分）:
  21.19 は `x` `d` `a` `r` `p` すべて `Inside an archive: read only — …`、`w2119\` は `z.zip` だけ。21.22d は `b.txt already exists — Enter is refused`、`a.zip already exists — Enter asks before replacing it`、`a.txt is being packed — Enter is refused`（`name hint:` に同じ文）。23.6 は `No such file or folder: ~\…\tpyo — showing ~\… (nearest: typo)`（赤は見ていない）。

## 押さなかった行

| 行 | 理由 |
| --- | --- |
| 16.3 / 16.3a / 16.3b | `.docx` と Word が要る。この run では用意していない |
| 1.44 / 1.45 | 境目のドラッグ。`--keys` ではできず、絵の判定 |
| 49.1–49.8 | この run の時間切れ |
| 37.8a / 37.8b | Chrome の窓が開いている（人のもの） |
| 23.4b / 23.4c | UNC のホストが無い |
| 16.13 / 16.13a | 前回の所見のまま（直していないので再実行しても同じ） |

## 所見

1. **1.35: 「End the shell?」の箱が実行中のプログラムを名指ししない（ARM64、`pwsh -NoProfile` で `ping -t`）。** 文は `A program is still running in the terminal, and ends with it.` だけ。v0.80.3 の期待は名前（`pwsh`）。タイトルが空なのか、名前を出す経路が働いていないのか、x64 でも同じか確かめる必要がある。**直していない。**
2. **25.4e: `filer env` の `powershell.exe` の版が `v6.2.28000.2804`。** ファイルの `FileVersion` は `10.0.28000.2113`。`src/envreport.rs:278` の `file_version()` は `GetFileVersionInfoW` + `VS_FIXEDFILEINFO` の上位・下位を読むが、Windows 10 の manifest を持たない呼び出し元には OS のファイルの版が 6.2 に化けて見える（互換性の層）。`pwsh`（アプリ側の版）は合う。（`2804` の部分は `FileVersion` の `2113` とも合わない。ビルド番号 `28000.2804` は OS 側の更新後の値かもしれない。）**直していない。**
3. 25.4f: `backend = "gl"` のとき `Adapter` の行の尾は `set by [ui] backend = "auto"` のままで、`Backend` の行と食い違う。`FILER_CONFIG_HOME` に `[ui] backend = "gl"` を置いた run で見た。**直していない。**

### Proposals

1. 1.35 の箱はタイトルが空のとき、起動した実行ファイルの名前（`pwsh`）で言い換える。
2. `file_version()` は `filer.exe` に Windows 10 の `supportedOS` manifest を付けるか、`RtlGetVersion` 系でなく `VerQueryValueW` の `\StringFileInfo\…\FileVersion` を読む。
3. `<State:>` の `clipboard set:` は切り詰めず、全文を別ファイルに書く（今回は `Get-Clipboard` で読み直した）。
4. 入力欄に `<C-u>` を効かせる（今は `<C-a>` の後に打つと置き換わる）。

### Votes

開いている票（`状態: 投票中`）は origin/main の QUESTIONS.md に無い。票は 0。

## 後始末

起動した `filer.exe` と `ping` はすべて閉じた。親の無い `OpenConsole` / `pwsh` / `powershell` は cargo test の前後とも 48。ごみ箱の古い 38 件は残したまま（上の「事前の後始末」）。証拠は `C:\dev\filer-evidence\arm-retests2\`。人の設定は触っていない。
