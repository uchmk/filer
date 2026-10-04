## 28. 外から加えられた変更 — ARM64（7ca342e / 0.73.71、arm レーン、無人の run）

`windows-role.md` の ARM64 の順番表の先頭「Re-tests of changed behaviour」に開いている行は無かったので、次の
「Unticked rows no queue owns」を取り、そこが名指ししている中から **28 節**（7 / 8、残りは 28.8）を進めた。
**28.8 を `[x]` にし、28 節は 8 / 8 になった。**28.1〜28.7 は x64 ではなくこのレーン（#108）が付けた印なので、
v0.73.71 で全部押し直し、結果をこの報告に残した（印は変えていない）。

- 機械: ARM64（Windows 11 Home 10.0.28000、`SystemType = ARM64-based PC`）。**昇格なし**。この節の行は昇格を要らない。
  窓は 1360 x 860 px、倍率 100%（`scale: 100% (ppp 1)`、どの `.done` にも出ている）。
- ビルド: `cargo build --release`（`filer env` が `Version 0.73.71` / `OS arch aarch64` / **`Process arch aarch64`**）。
  ConPTY は `fetch-conpty.ps1` で横に置いた（`ConPTY 1.24.260710001 (arm64)`、`filer.com` も）。
- `cargo test`: **660 passed; 0 failed**（`TEMP` は起動スクリプトが決めた
  `C:\Users\yuu06\AppData\Local\Temp\filer-scratch\run-20261004-110207`）。
  親の無い `OpenConsole` / `pwsh` / `powershell` は **48 → 48**（最後にもう一度数えても 48）。
- 一時ディレクトリは上と同じ（**R: は無い機械**）。テスト用のフォルダはその中の `s28a`、`s28-1`〜`s28-7`。
  `FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` / `FILER_STATE_HOME` はその中の空のフォルダにした（持ち主の設定には触れていない）。
  28.6 は**ごみ箱が要るので C: の中**（`%LOCALAPPDATA%\Temp\…`）で回した。
- キーは `--keys`（`PostMessage` も `SendInput` も使っていない）、読み取りは `FILER_KEYS_DONE` の `hovered:` / `toasts:` /
  `keys: done` と、`<Shot:>` と `PrintWindow(PW_RENDERFULLCONTENT)` の画像。入力デスクトップは毎回 `Default`、
  スクリーンセーバー無し、`LogonUI` 無し。
- 証拠は `C:\dev\filer-evidence\arm-28\`（`*.done`、画面、切り抜き、使った `win.ps1` / `run28.ps1`）。
  起動した `filer.exe` は全部閉じた（最後に `Get-Process filer` が 0 件）。

### 結果

| 行 | 印 | 根拠 |
| --- | --- | --- |
| 28.1 | `[x]`（据え置き。ARM64 で再確認） | `G` で最終行へ（`c-28_1.png`: カーソルは `file5.txt`、プレビューは `body 5`）。6.0 s の時点で外から `file5.txt` を消すと、`28_1.done` は `hovered: …\s28-1\file4.txt`、`toasts:` は空、プロセスは生存 |
| 28.2 | `[x]`（据え置き。ARM64 で再確認） | 同じく最終行から、外で `file3/4/5.txt` を一度に削除 → `28_2.done` の `hovered:` は `file2.txt`（新しい最終行）。生存 |
| 28.3 | `[x]`（据え置き。ARM64 で再確認） | 全 5 ファイルを外から削除 → `28_3.done` の `hovered:` が**空**（空の一覧）。削除の**後に**押した `j` と `k` も通り、最後の行は `keys: done`（止まっていれば `keys: stalled` が出る）。生存 |
| 28.4 | `[x]`（据え置き。ARM64 で再確認） | `f` + `file` + `<Enter>` で絞り込み、`G`。`c-28_4-filtered.png`: `file1`〜`file5` の 5 行で `file` が強調され、**`other.txt` は出ていない**、カーソルは `file5.txt`。外から `file5.txt` を削除 → `c-28_4-after.png` は 4 行、カーソルは `file4.txt`、**`other.txt` はやはり出ていない**（絞り込みは保たれた）。`28_4.done` の `hovered:` も `file4.txt` |
| 28.5 | `[x]`（据え置き。ARM64 で再確認） | 前半（もう一方のペイン）: `<C-w>` で分割（`28_5a.done` が `split: yes, keys right`、`tab: 2 of 2`）、`G` のあと外から `file5.txt` を削除 → `hovered: …\file4.txt`、生存。後半（ディレクトリのプレビュー）: カーソルを `sub` に置き、外から `sub\s5.txt` を削除 → プレビューの一覧から `s5.txt` が消えた（`c-28_5b-before.png` → `c-28_5b-after.png`）、`28_5b.done` は `hovered: …\sub`、生存 |
| 28.6 | `[x]`（据え置き。ARM64 で再確認） | `G` のあと `d`。待っている間にごみ箱を数えると、**元の場所が `…\s28-6` の項目が 1 件 `file5.txt`**（`Shell.Application` の名前空間 `0xA`、詳細 1 = 元の場所）、ディスク側は 4 件。`28_6.done` の `toasts:` は `Trashed file5.txt — u to undo | Restored file5.txt`、`hovered:` は `file4.txt`（新しい最終行）。`u` で戻したので、ごみ箱に**この run の残りは 0 件**、ディスクは 5 件に戻った。生存 |
| 28.7 | `[x]`（据え置き。ARM64 で再確認） | カーソルを `file3.txt` に置き、外から `renamed.txt` に改名 → `28_7.done` の `hovered:` は `file4.txt`（同じ位置に留まった＝行の期待の後半）、生存。ディスクは `file1, file2, file4, file5, renamed.txt` |
| **28.8** | **`[ ]` → `[x]`** | 下の節のとおり。**キーを 1 つも押さずに**、外からの削除が 0.5 秒以内に一覧へ出た |

### 28.8 の測り方と、押していないことの示し方

行の文は「窓に触らず（キーもマウスも使わず）」なので、`--keys` は使えない（キーを押すことになる）。
`filer.exe` を**引数はフォルダだけ**で起動し、落ち着かせてから、外から 1 ファイル消して `PrintWindow` で窓を撮った。

1. **据え置き**: `filer.exe <s28a>` を起動し 5 秒。`probe.png` は `5 items` / `1/5`、`file1`〜`file5`。
   そこから何もせずに 3 秒おくと `(Get-Process filer).CPU` は **0.40625 → 0.40625**（何もしていない窓は CPU を使わない）。
   （`probe.png` は `C:\dev\filer-evidence\arm-28\` に置いた。）
2. **削除**: `[System.IO.File]::Delete(…\file3.txt)`。削除は 2 ms で返った。
3. **114 ms 後の 1 枚**（`28_8-immediate.png`）は、削除**前**の 1 枚（`28_8-before.png`）と
   **SHA-256 が同一**（`0A9B43FF838E8C68…`）。
4. **579 ms 後の 1 枚**（`28_8-after500ms.png`）は違う絵で、`4 items` / `1/4`、一覧は `file1, file2, file4, file5`。
   `file3.txt` の行が消えている。
5. その間の filer の CPU は **0.40625 → 0.453125**（+0.047 s）。**キーもマウスも送っていないのに 1 フレーム描いた。**
   最前面の窓は始めから終わりまで filer 自身（`GetForegroundWindow` が同じ HWND）で、**焦点は一度も変わっていない**
   （焦点が戻ると `Refresh` が走るので、それでは行が消えた説明にならない）。
6. **時刻をもう一段細かく**: 続けて `file5.txt` を消し、`PrintWindow` を繰り返した。
   撮り始めが **179 ms の 1 枚までは古い絵**（`t0179.png`、`4 items`）、**244 ms から撮った 1 枚で新しい絵**
   （`t0244.png`、`3 items` / `1/3`、`file5.txt` が消えている）。つまり**削除から 179〜315 ms の間に描き直した**。半秒の内側。

**`PrintWindow` が描き直しの引き金になっていないこと**（これが成り立たないと 4 は何も言えない）:

- 3 の 114 ms の 1 枚が、削除前の 1 枚と**バイト単位で同じ**。撮影が描き直しを起こすなら、ここで新しい絵が出ていたはず。
  6 でも、179 ms までの 4 枚は撮っても古い絵のままだった。
- 何も変えずに 10.1 秒で 21 枚撮ると、**21 枚とも同じハッシュ**で、filer の CPU は +0.031 s。
  本物の描き直し 1 回が 5 の +0.047 s なので、**21 回の撮影は 1 フレーム分にも足りない**。

### 見つけたこと

1. **`FILER_KEYS_DONE` の状態に「一覧の件数」が無い。**28.8 も 28.3 も「行が消えた」「空になった」が期待で、
   画面には `5 items` / `1/5` と出ているのに、状態のファイルから読めるのは `hovered:` だけ。
   28.8 は画像を読んで `5 items` → `4 items` → `3 items` を確かめた（撮れるから成立したが、文字で読めるべきもの）。
   28.3 の「空の一覧」も `hovered:` が空であることから**間接的に**言うしかない（提案 1）。
2. **絞り込みが効いているかも、状態のファイルから読めない。**28.4 の期待の後半「絞り込みも保たれる」は
   `view: list` では分からず、画像 2 枚（`other.txt` が出ていないこと）で確かめた（提案 1）。
3. **ごみ箱に、前の run が残した 38 件があった。**元の場所は `…\filer-scratch\s28` が 32 件、`…\filer-scratch\trash-test` が 3 件、
   `…\filer-scratch\s28\sub` が 2 件、`…\filer-scratch\s44\keys` が 1 件で、**どれもこの run のフォルダ（`run-20261004-110207`）の外**。
   `d` を押す行を回した過去の run が、持ち主のごみ箱に置いたまま終わっている。この run は `u` で戻したので 0 件。
   filer の不具合ではないが、役割定義に「`d` を押した run は、終わりにごみ箱から自分の分を戻すか消す」を 1 行足したい（提案 3）。

### Proposals（3 件）

#### 提案 1: 状態のファイルに `items:` と `filter:` を足す

- **何に当たったか**: 28.8 の「行が消える」、28.3 の「空の一覧」、28.4 の「絞り込みも保たれる」。
  3 つとも画面には数字と行が出ているのに、`FILER_KEYS_DONE` から読めるのは `hovered:` だけだった。
  28.8 は画像を 2 枚読んで `5 items` → `4 items` を数え、28.4 は `other.txt` が出ていないことを画像で見た。
- **どう変えるか**: `state_report` に 2 行足す。
  `items: 4`（今の一覧の行数。絞り込み中なら絞り込んだ後の数）と、
  `filter: file`（絞り込みが無ければ空）。`list top:` のすぐ上あたり。
- **なぜ**: 「何行あるか」はチェック表のいくつもの行の期待そのもので、今は**撮れる機械でしか確かめられない**。
  文字になれば Linux レーンでも同じ行が読め、見た目の判断とも混ざらない。
  `hovered:` が空である理由（空の一覧か、まだ読んでいないか）も `items: 0` で切り分けられる。
- **大きさ**: `state_report` に 2 行（数行）。

#### 提案 2: `--keys` に、途中で状態を書き出す段を足す（`<Dump:name>`）

- **何に当たったか**: 28.4 は「絞り込んだ最終行にカーソル」→「外から削除」→「絞り込みは保たれたか」で、
  **前と後の 2 つの状態**が要る。ところが状態のファイルは**script の終わりに 1 回だけ**書かれるので、
  前半は `<Shot:>` の画像を読むしかなかった（28.1 / 28.5 も同じ理由で画像を 1 枚ずつ読んだ）。
- **どう変えるか**: `<Shot:name>` と同じ場所に `<Dump:name>` を足し、`FILER_KEYS_DONE` の隣に
  `name.state`（中身は今の `state_report` と同じ）を書く。`<Shot:>` のすぐ横の実装で済む。
- **なぜ**: 役割定義が TESTING-KEYS.md のために求めている「1 つのキーの前と後で、変わってはいけないものを突き合わせる」が、
  **1 回の run の中で文字として**取れるようになる。今はそのために run を 2 回に割るか、クリップボードを使い回すかで、
  後者は 2 つのセッションがぶつかった事故（2026-09-30）の元でもある。
- **大きさ**: `keyscript.rs` に 1 つの段、`main.rs` に数行（`<Shot:>` と同じ流れ）。

#### 提案 3: `d` を押す run は、終わりに自分がごみ箱へ入れたものを片付ける（役割定義に 1 行）

- **何に当たったか**: 28.6 の最中にごみ箱を数えたら、**前の run の 38 件**が元の場所 `…\filer-scratch\s28` などで残っていた。
  持ち主のごみ箱に、テストの残骸が溜まり続けている。
- **どう変えるか**: `windows-role.md` の「How to work」に 1 行:
  「`d`（ごみ箱）を押した run は、終わりに `u` で戻すか、`Shell.Application` の `0xA` から**自分の scratch を元の場所に持つ項目だけ**を消す。
  他の項目には触らない」。この run は `u` で戻す形にした（`28_6.done` の `toasts:` に両方出ている）。
- **なぜ**: ごみ箱は持ち主のもので、消えていないファイルが溜まると本物の削除が埋もれる。
  戻す側（`u`）なら消す判断が要らないので安全。
- **大きさ**: 役割定義に 1 行（filer のコードは変わらない）。

### Votes

- `origin/main` の QUESTIONS.md に `投票中` の質問は**無い**（Q67・Q68・Q69 は `未回答` で、持ち主の見た目の判断を待っている）。
  この run の票は無し。

### Queue

ARM64 の順番表の「Unticked rows no queue owns」の行から、**28 を消す**（8 / 8 になった）。
`#220` が挙げた残りのうち、短くて文字で読めるのは **37（7 / 8、37.8 は `Get-CimInstance Win32_Process` でピッカーが起動したものを読む）** と
**32（13 / 18、Windows の行は 32.5 だけ）**。次の run はそのどちらかを取る。
行の文は次のように直すのが近い:

> **Unticked rows no queue owns** — … 短くて測れるもの（#220、この PR で更新）: 37（7 / 8；37.8 は `Get-CimInstance Win32_Process` で
> ピッカーが起動したものを読む）、32（13 / 18；32.5 がただ 1 つの Windows の行）。**28 は 8 / 8 で済んだ**（この PR）。…

### CHANGELOG の 1 行（マージする側へ）

```
- TESTING-CHECKS.md: 28.8 ticked on the ARM64 laptop — an idle window drops a row deleted from
  outside 179-315 ms later, with no key pressed (section 28 is now 8 / 8).
```
