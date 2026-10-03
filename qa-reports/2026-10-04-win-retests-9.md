## Re-tests of changed behaviour — x64（48ac55a / 0.73.37、win レーン、無人の run）

`.claude/windows-role.md` の順番表の先頭「Re-tests of changed behaviour」。この回に開いていたのは **21.16**（v0.73.36）、**12.11a**（v0.73.35）、
**19.6 / 19.7**（v0.73.34）、**33.19**（v0.73.33）の 5 行。**5 行とも `[x]`**。21.14a は行の書き直し待ち、47.1 はバックエンドの直し待ちなので触っていない。

- 機械: x64、Windows 11 Pro 10.0.26200、DPI 144（1.5 倍）、窓のクライアント領域 2040 x 1290 px。**昇格なし**（`IsInRole('Administrators')` = False）。この回の行は昇格を要らない。
- ビルド: `cargo +stable build --release`、ConPTY 1.24.260710001 を `fetch-conpty.ps1` で横に置いた。`filer --version` = `filer 0.73.37 (x86_64)`。
- `cargo +stable test`: **637 passed; 0 failed**（と doctest 1）。前後の親の無い `OpenConsole` / `pwsh` / `powershell` は **0 → 0**。
- 一時ディレクトリは `R:\Temp\run-20261004-000706`。`FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` / `FILER_STATE_HOME` をその中のフォルダにした（持ち主の設定には触れていない）。
- ホイールと `Ctrl` は `SendInput`（ホイール 1 ノッチ = 120、4 分の 1 = 30）。送る前に毎回、入力デスクトップが `Default`、スクリーンセーバー無し、`LogonUI` 無しを確かめた。
- 証拠は `C:\dev\filer-evidence\win-retests-9\`（`*.done` の状態ファイル、画面、`filer env` の出力、使った設定 `cfg\`、`w19.ps1`）。

### 結果

| 行 | 印 | 根拠 |
| --- | --- | --- |
| 12.11a | `[x]` | `R:` の `d12\a.txt` で `--keys "d<Shot:12a-d>D<Shot:12a-D>y<Shot:12a-Dy>q"`。`toast: Deleted a.txt`、一覧は `b.txt` だけ。`12a-D.png` では赤い `Trash: a.txt: the Recycle Bin can't take files from R: (…). Use D to delete permanently` と確認 `Delete permanently? a.txt` が出ていて、`12a-Dy.png` では右上のトーストが `Deleted a.txt` の 1 つだけ（赤い行は無い） |
| 21.16 | `[x]` | `to-pack\` と `sample.zip` だけのフォルダで `E<Enter>` → `e` → `G<Space>k<Space>e`。`toasts: Packed into to-pack.zip \| Unpacked into to-pack_1\ \| Unpacked 2 archives into sample\ and 1 more`。ディスク: `to-pack_1\` は `file1..5.txt` と `nested\` を直接持つ（中身が本当に入った所）、2 つ目の `e` で `sample\` と `to-pack_2\` ができた |
| 19.6 | `[x]` | `zoom-me.png`（キャプション `3200 × 2400 · fit 15%`）の上で `Ctrl`+上 1 ノッチ → `zoom: 19%`、2 ノッチ → `23%`、上 1・下 1 → `15%`。`Ctrl` 無しの 2 ノッチは `fit` のまま。`long.rs` の上で `Ctrl`+下 2 ノッチ → `preview top: 0 of 3984`、`zoom: fit`（スクロールしない） |
| 19.7 | `[x]` | 分割（`<C-w>`）した `many\` で下の表。左右に 4 分の 1 ずつ → どちらも 0（#202 の前は右が 1）。右にもう 4 分の 1 → 右だけ 1。非分割の `long.rs` でプレビュー ↔ 一覧に 4 分の 1 ずつ → `preview top` 0、`list top` 0 |
| 33.19 | `[x]` | `YAZI_CONFIG_HOME\keymap.toml` に `Q` → `quit`、`FILER_CONFIG_HOME\keymap.toml` に `Q` / `T` → `hidden toggle`。`filer-com.exe env` の Warnings が下の 2 行 |

#### 19.7（`many\`、500 項目。ポインタは左 (0.18, 0.5)、右 (0.55, 0.5)。それぞれ 2 回起動し、終わりに `<C-w>` を足した回で左を読んだ）

| 送ったもの | 右の `list top` | 左の `list top` |
| --- | --- | --- |
| 左 4 分の 1 → 右 4 分の 1 | 0 | 0 |
| 左 4 分の 1 → 右 4 分の 1 → 右 4 分の 1 | **1** | 0 |
| 右 4 分の 1 → 左 4 分の 1 | 0 | 0 |
| 右 4 分の 1 ×2（対照） | 1 | 0 |
| 左 1 ノッチ（対照） | 0 | 2 |

#### 33.19 の `filer env`（`cmd /c … > file` で取った。PowerShell のパイプを通すと端末の幅で折り返す）

```
Warnings : [mgr] `Q` is bound more than once; only `hidden toggle` (R:\Temp\run-20261004-000706\cfg\filer\keymap.toml) runs, not `quit` (R:\Temp\run-20261004-000706\cfg\yazi\keymap.toml)
           [mgr] `T` is bound more than once; only `hidden toggle` (R:\Temp\run-20261004-000706\cfg\filer\keymap.toml) runs, not `plugin toggle-pane max-preview` (the built-in defaults)
```

行は `%APPDATA%\yazi\config\` と `%APPDATA%\filer\` を名指ししているが、持ち主の設定を書き換えないために、2 つの `*_CONFIG_HOME` で同じ 2 か所を一時ディレクトリに向けた。
警告が出す道（どちらのフォルダのどのファイルか）は同じで、両方の完全なパスが出ている。

### 見つけたこと

1. **（順番表の文言）12.11a の「`toasts:` に `Deleted a.txt` があり `Trash: a.txt: …` の行が無い」は、成り立たない読み方だった。**
   `toasts:` は消えたものも含めたその実行の全トーストの記録（`src/main.rs` の `state_report`）なので、`d` のエラーは必ず残る
   （今回も `toasts: Trash: a.txt: … | Deleted a.txt`）。行の言う「赤い行が `Deleted` に置き換わる」は、`toast:`（最後の 1 つ）と、
   `12a-Dy.png` の画面に出ているトーストが 1 つだけであることで読んだ。filer の不具合ではない。

### Proposals（2 件）

#### 提案 1: `FILER_KEYS_DONE` に、いま画面に出ているトーストを全部書く（例 `shown: …`）

- **踏んだこと**: 12.11a。「赤い行が `Deleted a.txt` に置き換わった」を確かめたかったが、状態ファイルには `toast:`（最後の 1 つ）と
  `toasts:`（消えたものも含めた記録）しかなく、「エラーがまだ画面に残っているか」は書かれていない。結局 `<Shot:>` の画像から読んだ。
- **どう変えるべきか**: `app.toasts`（いま出ているもの）を ` | ` でつないだ `shown:` の行を足す。
- **なぜ**: 「前のトーストが消える／残る」を言う行（12.11a、置き換え系の直し）が、画像なしで読めるようになる。順番表の文言の取り違え（見つけたこと 1）も起きなくなる。
- **大きさ**: `state_report` に 1 行。

#### 提案 2: 自分の `keymap.toml` で既定のキーを上書きしたことは「警告」にしない（または別の見出しにする）

- **踏んだこと**: 33.19。filer の `keymap.toml` に `T` → `hidden toggle` を書いただけで、`filer env` の Warnings に
  `` `T` is bound more than once; only `hidden toggle` … runs, not `plugin toggle-pane max-preview` (the built-in defaults) `` が出た。
- **どう変えるべきか**: 既定と利用者のファイルがぶつかるのは `prepend_keymap` の本来の使い方なので、Warnings ではなく `Overrides` のような別の見出しに出す（または出さない）。
  警告は、利用者のファイル同士（yazi と filer、同じファイルの中）がぶつかったときに限る。
- **なぜ**: キーを付け替えた人ほど Warnings が増え、本当に意図しない重複（今回の `Q` のような yazi 側と filer 側の衝突）が埋もれる。
- **大きさ**: 警告を作る所で出どころが既定かどうかで分けるだけ。ただし Q57 の決定に関わるので、持ち主の判断（QUESTIONS.md 向き）。

### Votes

- 開いている `投票中` の質問は無い（`origin/main` の QUESTIONS.md に `状態: 投票中` が 1 件も無い）。今回の票は無し。

### 後片付け

- 起動した `filer.exe` はすべて終了した（最後に `Get-Process filer` は見つからない）。
- 人の設定ファイルには触れていない。テスト用のファイルは R: の一時ディレクトリの中だけに作り、証拠は `C:\dev\filer-evidence\win-retests-9\` に写した。
