## Windows x64 実機: 変更の再テスト（2026-10-03、test/win-retests-7、無人）

順番表の先頭「Re-tests of changed behaviour」に並んでいた 9 行（7.7a、21.14、21.14a、25.19a、25.19c、
25.19d、25.19e、25.24a、29.12）を 1 回で回した。**8 行に `[x]`、21.14a は付けない**（エクスプローラーの半分が
行の期待どおりにならず、行の側が成り立たない。下の「見つけたこと 1」）。

- 機械: Windows 11 Pro 25H2（build 26200.9457）、x64、PowerShell 7.6.6。管理者ではない（今回の行には要らない）。
- ビルド: `origin/main` の 5500cc4（v0.73.18）を `cargo build --release`。
- **zip のフォルダ**: v0.73.5 以降の修正を含むリリースはまだ無い（最新は v0.72.2）ので、`release.yml` の
  「Package the binaries」と同じ形のフォルダを手で組んだ: `R:\Temp\run-20261003-190706\filer-v0.73.18-windows-x64\` に
  `filer.exe`、`filer-com.exe` を `filer.com` として、`fetch-conpty.ps1` の `conpty.dll` と `OpenConsole.exe`（1.24.260710001 x64）。
  `filer.com --version` → `filer 0.73.18 (x86_64)`。この機械の `PATH` には元々 `filer` が無い（`Get-Command filer -All` が空）。
- `cargo test`: **630 passed / 0 failed**（と doc 1 件）。親の無いシェル（#180）: 前 **88**（OpenConsole 44、powershell 24、pwsh 20）、後 **88**。増えていない。
- 証拠の写し: `C:\dev\filer-evidence\2026-10-03-win-retests-7\`（画像 4 枚、`FILER_KEYS_DONE` 5 つ、`r.txt`、zip 3 つ）。

### 結果

| 行 | 結果 | 根拠 |
| --- | --- | --- |
| 7.7a | `[x]` | 下 |
| 21.14 | `[x]` | 下 |
| 21.14a | 付けない | 7-Zip の 2 つの半分は通る。エクスプローラーの 2 つは、エクスプローラーが DOS 時刻しか読み書きしないので、行の期待は誰にも満たせない |
| 25.19a | `[x]` | 下 |
| 25.19c | `[x]` | 下 |
| 25.19d | `[x]` | 下 |
| 25.19e | `[x]` | 下 |
| 25.24a | `[x]` | 下 |
| 29.12 | `[x]` | 下 |

#### 25.19a / 25.19c（`pwsh -NoProfile -File` のスクリプトで、zip のフォルダを `PATH` の先頭に）

```text
(Get-Command filer).Source           -> ...\filer-v0.73.18-windows-x64\filer.com
$v = & filer env | Write-Output      -> 49
$v2 = & filer env  (後ろに何も無い)   -> 49
(cmd /c "filer env").Count           -> 49
(cmd /c "filer env" | Measure-Object -Line).Lines -> 45   （空行 4 つを Measure-Object が数えない。下の「見つけたこと 2」）
filer env --out r.txt; (Get-Content r.txt).Count  -> 49   （空行 4、最後の行は "    FILER_TERM_SHELL  : unset"、ファイルの末尾のバイトは "unset\n"。後ろに空行は無い）
filer --version > v.txt; Get-Content v.txt         -> filer 0.73.18 (x86_64)
filer --keys "<Tab"; $LASTEXITCODE   -> filer: --keys: `<Tab` has no closing `>` / exit: 2
filer   (引数なし)                    -> 749 ms でスクリプトに戻り、2.5 s 後に filer が pid 46348、タイトル "Filer: R:\Temp\...\t25d" で残っている
```

25.19a は `filer.exe` だけを置いたフォルダでも回した: `| Write-Output` 付きで **49**、後ろに何も無い形は **0**（PowerShell が窓のプログラムを待たない。行が書くとおり）。

#### 25.19d（`pwsh -NoProfile -File w.ps1 | Tee-Object t.txt; "back"`）

- `w.ps1` = `filer`: `back` が 989 ms で出て、その時点で `Get-Process filer` が pid 78672、タイトル `Filer: R:\Temp\run-20261003-190706\t25d`。窓は開いたまま。
- `w.ps1` = `filer C:\dev C:\Windows`: 画面に `filer: more than one path: "C:\dev" and "C:\Windows" (a path with a space in it needs quotes)`、
  `back` は 414 ms、`t.txt`（95 B）に同じ 1 行、filer のプロセスは 0。

#### 25.19e（空のフォルダ、`pwsh -NoProfile -File` の `.ps1`）

```text
filer env --out a.txt; Test-Path a.txt; $LASTEXITCODE        -> filer: wrote ...\t25e\a.txt / True / 0
& <zip>\filer.exe env --out b.txt | Out-Null; Test-Path b.txt -> True
& <zip>\filer.exe env --out | Out-Null; $LASTEXITCODE         -> 2
フォルダの中身: a.txt 2847 B、b.txt 2847 B（名前の無い --out は何も書いていない）
filer --help:   env --out FILE   the same, written to FILE as UTF-8
                                 (filer.exe in a script: add | Out-Null to wait)
```

#### 25.24a（PowerShell から引用符なし）

`filer C:\dev C:\Windows` → `filer: more than one path: "C:\dev" and "C:\Windows" (a path with a space in it needs quotes)`、exit 2。
`filer.com` 経由でも、`filer.exe` を直接呼んでも同じ 1 行で、`\` は 1 つずつ。1 秒後の filer のプロセスは 0（窓は出ていない）。

#### 29.12（`FILER_CONFIG_HOME` を `R:\Temp\...\cfg2912` に。`filer.toml` は `[ui] window_width = 1000.0 / window_height = 700.0` と `[term] shell = "pwsh" / args = ["-NoLogo", "-NoProfile"]`）

- 変数あり（`FILER_TERM_SHELL=powershell`、`--keys "<C-t><Wait:1200><Shot:pane>"`）:
  `toast: Started powershell (Windows PowerShell 5.1) — <C-t> back to the list`。filer の子は `powershell`（引数なし）と `OpenConsole.exe --headless …`。
  `filer env` は `powershell : C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe   (terminal pane, from FILER_TERM_SHELL; [term] args not used: -NoLogo -NoProfile)`、
  Variables に `FILER_TERM_SHELL  : powershell`。
- 変数なし（`$env:FILER_TERM_SHELL = $null`）: `toast: Started pwsh — <C-t> back to the list`。子は `pwsh -NoLogo -NoProfile`。
  `filer env` は `pwsh : …\pwsh.exe   (terminal pane, from [term] shell)`（args の話は出ない）、`FILER_TERM_SHELL  : unset`。
- 「`filer.toml` の他の設定が効いている」の代わりに、フォント・テーマではなく**窓の大きさ**を読んだ: 2 回とも `filer env` の Last run が
  `Window : 1500 x 1050 px (1000 x 700 pt @ 1.5)`（この run の前の記録は `1360 x 860 pt`）。

#### 7.7a（`%LOCALAPPDATA%\Temp\filer-7.7a\` に `filer.toml`。C: で回した。R: は `canonicalize` が効かないので、行の `C:\cfg` に近い形を選んだ）

```text
YAZI_CONFIG_HOME  = C:\Users\yuu06\AppData\Local\Temp\filer-7.7a
FILER_CONFIG_HOME = c:\USERS\YUU06\APPDATA\LOCAL\TEMP\FILER-7.7A\
filer env の Config:
    C:\Users\yuu06\AppData\Local\Temp\filer-7.7a\ : filer.toml 106 B
                                                    not here: yazi.toml, keymap.toml, theme.toml
    State    : C:\Users\yuu06\AppData\Roaming\filer
    Warnings : none
~<Wait:500>C  -> toast: Copied the help panel: 154 keys。クリップボード（SENTINEL-77a を先に入れて確かめた）の 163 行のうち、
                 filer-7.7a を含む行は 1 つ（"C:\Users\yuu06\AppData\Local\Temp\filer-7.7a\" の下に "    filer.toml"）
~ <Esc><C-F5> -> toast: Reloaded 1 config file(s)
```

R: でも `filer env` だけ見た: `YAZI_CONFIG_HOME = R:\Temp\run-20261003-190706\cfg2912`、`FILER_CONFIG_HOME = r:\TEMP\RUN-20261003-190706\CFG2912\` で
Config の節は 1 行（`R:\Temp\run-20261003-190706\cfg2912\ : filer.toml 106 B`）。

#### 21.14（`E` で `.zip` / `.tar.gz` / `.7z`、`G` `e` で展開。形式ごとに別のフォルダ）

`to-pack\a.txt` と `to-pack\sub\c.txt` を `2021-06-15 12:34:56`、`to-pack\b.txt` を **`12:34:57`（奇数秒）**にした（行の例は偶数秒で、
v0.73.16 の変更を試さない。「見つけたこと 3」）。キーは zip が `E<Enter>Ge`、他は `E<End><Backspace><Backspace><Backspace>tar.gz<Enter>Ge` の形。

```text
           a.txt                        b.txt                        sub\c.txt
zip    -> 2021-06-15T12:34:56.0000000  2021-06-15T12:34:57.0000000  2021-06-15T12:34:56.0000000   (to-pack_1\)
tar.gz -> 2021-06-15T12:34:56.0000000  2021-06-15T12:34:57.0000000  2021-06-15T12:34:56.0000000
7z     -> 2021-06-15T12:34:56.0000000  2021-06-15T12:34:57.0000000  2021-06-15T12:34:56.0000000
```

3 形式とも秒まで元どおり。`7z l -slt` で filer の zip を見ると、各項目が `Characteristics = UT:M:1` で `b.txt` は `Modified = 2021-06-15 12:34:57`。

#### 21.14a（付けない）

`f.txt` を `2019-02-28 23:59:59` にし、7-Zip 26.02（`7z a x.zip f.txt`）と、エクスプローラーの「圧縮 (zip 形式) フォルダー」
（同じ zipfldr を `Shell.Application` の `NameSpace(zip).CopyHere` で呼んだ）で zip にした。

| 向き | 結果 | 行の期待 |
| --- | --- | --- |
| 7-Zip の zip → filer の `e` | `2019-02-28T23:59:59` | 通る |
| エクスプローラーの zip → filer の `e` | `2019-03-01T00:00:00` | 期待は `23:59:59`。**zip にその値が無い** |
| filer の zip（`b.txt` 12:34:57）→ 7-Zip の `7z x` | `12:34:57` | 通る |
| filer の zip → エクスプローラーで展開（`CopyHere` で取り出し） | `12:34:56` | 期待は 1 秒下がらないこと。**下がった** |

zip の中を読んだ:

- エクスプローラーの zip は拡張フィールドを持たない（`7z l -slt` の `Characteristics` が空）。ローカルヘッダーの DOS 時刻は `2019-03-01 00:00:00`。
  **エクスプローラーは奇数秒を切り上げて書き、その時刻しか持たない**ので、`e` が `23:59:59` を戻す手段は無い。7-Zip で展開しても同じ `00:00:00`。
- 7-Zip の zip は NTFS フィールド（`23:59:59`）を持ち、DOS 時刻は**同じく切り上げ**て `2019-03-01 00:00:00`。それをエクスプローラーで展開すると
  `2019-03-01T00:00:00`。**エクスプローラーは NTFS フィールドも UT フィールドも読まず、DOS 時刻だけを使う。**
- filer の zip は UT フィールドを持ち、DOS 時刻は**切り捨て**で `b.txt` が `12:34:56`。だからエクスプローラーは 1 秒下げる。

つまり**エクスプローラーを通る 2 つの半分は、どの拡張フィールドを書いても奇数秒にならない。**filer が変えられるのは DOS 時刻の丸めの向きだけ
（「提案 1」）。

### 見つけたこと

1. **21.14a のエクスプローラーの 2 つの半分は、行の期待を誰も満たせない。**上の表のとおり、エクスプローラーは DOS 時刻（2 秒刻み）しか
   読み書きしない。行は「filer の `e` は `23:59:59` に戻す」「他のツールも filer の zip から奇数秒のまま戻す」と書くが、エクスプローラーについては
   「filer の `e` はエクスプローラーの zip から `2019-03-01 00:00:00`（zip にある値）」「エクスプローラーは filer の zip から偶数秒」と書くしかない。
   7-Zip の半分はそのまま通るので、行を 7-Zip とエクスプローラーで分けるのがよい。**行は直していない**（役割の規則どおり）。
2. **順番表の 25.19a / 25.19c の測り方が、そのままでは回らない。**`(Get-Content (filer env --out r.txt; "r.txt")).Count` は PowerShell の
   構文エラー（`式に末尾の ')' がありません`）。`$( … )` にしても、`filer: wrote …` の行が `Get-Content` にパスとして渡り
   （`ドライブが見つかりません。'filer' という名前のドライブは存在しません`）、数えられたのは `r.txt` の分だけだった。
   また `(cmd /c "filer env" | Measure-Object -Line).Lines` は**空行を数えない**ので 45 になり、`$v.Count` の 49 と合わない（filer ではなく
   `Measure-Object` の動き）。比べるなら `(cmd /c "filer env").Count`（49）と、`filer env --out r.txt` のあとの `(Get-Content r.txt).Count`（49）。
3. **21.14 の例の時刻 `12:34:56` は偶数秒**なので、v0.73.16 より前の zip（偶数秒まで）でも通ってしまい、行が言う「秒まで」の変更を試さない。
   今回は `b.txt` を `12:34:57` にして試した。例を奇数秒（`12:34:57`）にするとよい。

### Proposals（3 件）

#### 提案 1: zip の DOS 時刻を切り上げにそろえるか決める

- **踏んだこと**: filer の zip をエクスプローラーで展開すると、奇数秒のファイルが **1 秒古く**なった（`12:34:57` → `12:34:56`）。
  7-Zip とエクスプローラーは自分の zip に DOS 時刻を**切り上げて**書く（`23:59:59` → `00:00:00`）ので、同じ元ファイルでも、
  7-Zip の zip をエクスプローラーで開くと 1 秒新しく、filer の zip だと 1 秒古い。
- **どう変えるべきか**: DOS 時刻の丸めを 7-Zip / エクスプローラーと同じ切り上げにする（UT フィールドは今のまま秒まで）。
  そうすれば、DOS 時刻しか読まない道具（エクスプローラー）では 3 つの道具の zip が同じ時刻になる。
- **なぜ**: 1 秒古い側に出ると、`robocopy /XO` のような「新しいほうを残す」同期で、展開したものが元より古いと判断されて上書きされる。
  切り上げにはその逆の危険がある（展開したものが 1 秒新しい）ので、どちらにそろえるかは持ち主の判断（QUESTIONS.md 向き）。
  ただ、**よく使われる 2 つの道具と違う向き**であることは、それだけで説明の要る差になる。
- **大きさ**: zip を書くところの 1 行（丸め）と、21.14a の文言。

#### 提案 2: `filer env --out` の `filer: wrote <path>` を標準エラーに出す

- **踏んだこと**: `$(filer env --out r.txt; "r.txt")` の形で書いたファイルを読もうとしたら、`filer: wrote R:\…\r.txt` が**標準出力**に出ていて
  （`$x = filer env --out z.txt 2>$null` が `filer: wrote …` を捕まえた）、`Get-Content` にパスとして渡った。
- **どう変えるべきか**: `filer: wrote …` は状況の報告なので標準エラーに出す。標準出力は空にする。
- **なぜ**: `--out` を使う人はレポートをファイルで受け取りたいので、標準出力に何かが出ると、スクリプトで変数に受けたときにそれが混じる。
  `git` も `curl -o` も、ファイルに書いたときの進み具合は標準エラーに出す。
- **大きさ**: 1 行（`println!` を `eprintln!` に）。25.19b の期待文（`filer: wrote <フルパス>` の 1 行が出る）は画面上は同じ。

#### 提案 3: 順番表の再テストの文に、そのまま貼って回せる PowerShell を書く

- **踏んだこと**: 「見つけたこと 2」の 2 つ（構文エラー、空行を数えない `Measure-Object -Line`）。順番表の文が「この 3 つの数を出せ」と言うのに、
  その 3 つの出し方のうち 2 つが、そのままでは違う数を出すか、動かない。
- **どう変えるべきか**: 数を比べさせる再テストは、比べる式を `scripts/` か TESTING.md の行に、手元で一度回した形で書く。
- **なぜ**: 無人の run は質問できないので、式が動かないと run が自分で直すことになり、直し方が run ごとに違えば同じ数を比べていることにならない。
- **大きさ**: 順番表の 1 行。

### Votes

- `origin/main` の QUESTIONS.md で `状態: 投票中` は Q57 だけで、`投票` 欄にはもう `win: 1`（#194）がある。新しく入れる票は無い。

### 後片付け

- 起動した `filer.exe` はすべて終了した（最後に `Get-Process filer` は見つからない）。
- `%LOCALAPPDATA%\Temp\filer-7.7a` は消した。人の設定ファイル（`$PROFILE`、`%APPDATA%\filer\filer.toml` など）には触れていない（読んだだけ）。
