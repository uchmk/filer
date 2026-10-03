## Re-tests of changed behaviour — x64（abf2e1f / 0.73.30、win レーン、無人の run）

`.claude/windows-role.md` の順番表の先頭「Re-tests of changed behaviour」。この回に開いていたのは **47.1 / 47.4**（#204）、**12.11a**（v0.73.28）、
**13.12a**（v0.73.27）の 4 行。21.14a は行の書き直し待ちなので触っていない。

- 機械: x64、Windows 11 Pro 10.0.26200。GPU は AMD Radeon RX 9070 XT（ドライバ 32.0.31041.1004）と内蔵の AMD Radeon(TM) Graphics
  （32.0.21045.5002）。どちらも日付 2026-08-17。**昇格なし**（`IsInRole('Administrators')` = False）。この回の行は昇格を要らない。
- ビルド: `cargo +stable build --release`（rustc 1.99.0）、ConPTY 1.24.260710001 を `fetch-conpty.ps1` で横に置いた。`filer --version` = `filer 0.73.30 (x86_64)`。
- `cargo +stable test`: **631 passed; 0 failed**（と doctest 1）。前後の親の無い `OpenConsole` / `pwsh` / `powershell` は **0 → 0**。
- 一時ディレクトリは `R:\Temp\run-20261003-220706`。設定は空のフォルダを `FILER_CONFIG_HOME` にした（持ち主の設定には触れていない）。
- 証拠は `C:\dev\filer-evidence\win-retests-8\`（状態ファイル、画面、測定スクリプト `cpu.ps1`）。

### 結果

| 行 | 印 | 根拠 |
| --- | --- | --- |
| 47.1 | `[ ]` のまま（**失敗**） | 既定（Vulkan）で 10 秒の差 **9.97 s**。`vulkan` 10.00 s、`dx12` 10.00 s、`gl` **0.00 s**。陽性対照（`gl`、100 ms ごとに `j` / `k` を 10 秒）は 3.41 s |
| 47.4 | `[x]` | 増えるのは UI ではない 1 本のスレッド。開始アドレス: Vulkan は `amdvlk64.dll+0x2F3DA4C`（9.2.10.395）、DX12 は `amdxc64.dll+0x1298D0`（8.18.10.0504）。内蔵 GPU では `amdvlk64.dll+0x2EF8C3C`（9.2.10.353） |
| 12.11a | `[x]` | `d` のあと `toast: Trash: a.txt: the Recycle Bin can't take files from R: (this drive can't report its own paths: a RAM disk or a virtual drive). Use D to delete permanently`、`a.txt` は残る。続けて `D` → 確認 `Delete permanently? a.txt` → `y` で `a.txt` だけが消え `b.txt` は残る |
| 13.12a | `[x]` | `C:\dev\filer\target` で `<Tab>` → `C`: `Kind Junction` / `Target R:\cargo-target\filer` / `Resolves R:\cargo-target\filer (as written: this volume cannot normalize it, ファンクションが間違っています。 (os error 1))`。作業フォルダの `C:\dev\filer-wintest\target`（→ `R:\cargo-target\filer-wintest`）でも同じ形 |

### 47.1 / 47.4 の測定

1 回ごとに filer を `fx`（`make-fixtures.ps1`、15 項目）で起動し、10 秒何もせず、`(Get-Process).CPU` とスレッドごとの `TotalProcessorTime` を
10 秒あけて 2 回読んだ。開始アドレスは `NtQueryInformationThread(ThreadQuerySetWin32StartAddress)` で取り、読み込まれたモジュールの範囲に当てた
（`cpu.ps1`）。

| exe | `WGPU_BACKEND` | 10 秒の差 | 増えたスレッド（開始アドレス） |
| --- | --- | --- | --- |
| main 0.73.30 | （既定） | 9.97 s | `amdvlk64.dll+0x2F3DA4C` (9.2.10.395) +9.98 s |
| main 0.73.30 | `vulkan` | 10.00 s | `amdvlk64.dll+0x2F3DA4C` (9.2.10.395) +10.03 s |
| main 0.73.30 | `dx12` | 10.00 s | `amdxc64.dll+0x1298D0` (8.18.10.0504) +10.03 s |
| main 0.73.30 | `gl` | **0.00 s** | 無し（全スレッド +0） |
| main 0.73.30 | `vulkan` + `WGPU_POWER_PREF=low`（内蔵 GPU） | 10.00 s | `amdvlk64.dll+0x2EF8C3C` (**9.2.10.353**) +10.02 s |
| v0.72.2 の zip | `vulkan` | 10.03 s | `amdvlk64.dll+0x2F3DA4C` +10.03 s |
| v0.72.2 の zip | `dx12` | 10.00 s | `amdxc64.dll+0x1298D0` +10.02 s |
| v0.72.2 の zip | `gl` | 0.00 s | 無し |
| v0.64.2 の zip | `vulkan` | 10.00 s | `amdvlk64.dll+0x2F3DA4C` +10.02 s |
| **v0.47.10 の exe**（v0.54.2 の直しより前） | `vulkan` | 9.98 s | `amdvlk64.dll+0x2F3DA4C` +10.03 s |
| 陽性対照: main、`gl`、`--keys` で 100 ms ごとに `j`/`k` | `gl` | 3.41 s | UI スレッド +3.12 s、`atio6axx.dll` +0.81 s |

- どの版も、そのスレッドの累計は 10 秒の時点ですでに約 10 s で、**起動直後から回り続けている**（何かの操作で始まるのではない）。filer の UI スレッドは 0。
- **filer の版では変わらない。**v0.47.10 から main まで、同じオフセットの同じドライバのスレッドが 1 コアを使う。#204 の「どこかで変わった」の答えは「filer の側では変わっていない」。
- **これまでの x64 の 47.1 の `[x]` は、実は x64 で測ったものではなかった。**QA-REPORT.md の 47 節の 2 回（0.54.12、0.55.4）はどちらも ARM64 レーンで、
  47.1〜47.3 の `[x]` はそこから来ている。x64 で 47 節を測ったのは今回と #204 が初めて。そして **#86（x64、45 節の run）が見た「起動から約 130 ms 後に作られた 1 本が
  1.0 CPU 秒/秒、最小化しても変わらない」は、今回と同じ形**で、v0.54.2 の描き直しの直しはこれには効いていなかった公算が高い。
- 始まりがドライバの更新かどうか: 今のドライバは 2026-08-17 付で、#86 はそれより後。前のドライバに戻して試すことはこの run ではできない（持ち主の機械へのインストールになる）。
  **dGPU（9.2.10.395）と内蔵 GPU（9.2.10.353）の両方の Vulkan ドライバ、それに DX12 のドライバでも起きる**ので、filer の使い方
  （常に開いているスワップチェーン、present の仕方）と AMD のドライバの組み合わせで起きている、と読むのが近い。GL では起きない。
- 47.2 / 47.3 / 47.5 は ARM64 の `[x]` のまま。x64 の既定（Vulkan）では、どれも同じドライバのスレッドで 10 s / 10 s になるはず（スレッドは起動直後から回るので）。
  この run では 47.1 以外は測っていない。

### 見つけたこと

1. **x64（AMD GPU）では、何もしていない窓が既定の Vulkan でも DX12 でも 1 コアを使い続ける。filer の版に依らない（v0.47.10 から 0.73.30 まで同じ）。**
   上の表。GL なら 0。47.1 は x64 では失敗のまま。
2. **（手順の注意）`--keys` の `FILER_KEYS_DONE` を `Select-String -Quiet -ErrorAction SilentlyContinue` で待つと、ファイルができる前のエラーで待ちが 20 秒空回りした。**
   `Test-Path` で待つと問題ない。filer の不具合ではない。

### Proposals（2 件）

#### 提案 1: GPU のバックエンドを `filer.toml` で選べるようにし、AMD の Windows では GL を勧める（または既定にする）

- **踏んだこと**: 上の測定。既定のまま開いて何もしないだけで、10 秒で 10 CPU 秒。`WGPU_BACKEND=gl` を環境変数で付ければ 0 になるが、それは
  利用者には見えない。
- **どう変えるべきか**:
  1. `filer.toml` に `renderer = "auto" | "vulkan" | "dx12" | "gl"` のような設定を足し、`filer env` に今のバックエンドとアダプタを出す（`WGPU_BACKEND` は今でも効くので、まず README に書く）。
  2. present mode（`Fifo` / `AutoVsync`）や `desired_maximum_frame_latency` を変えて Vulkan / DX12 で回り続けが止まるかを、この機械で試す（どれかで止まれば、それを既定にする）。
  3. 止まらなければ、Windows で AMD のアダプタのときは GL を既定にするかを持ち主が決める（QUESTIONS.md 向き。GL は wgpu の中で一番手薄なので、見た目や不具合の差を見てから）。
- **なぜ**: 1 コアの空回りは、ノート PC なら電池に直に効き、デスクトップでもファンが回る。#86 からずっと x64 で起きていて、印は ARM64 のものだった。
- **大きさ**: 1 は設定 1 つと `NativeOptions` の数行。2 は数行と実機での測定。3 は設計の判断。

#### 提案 2: `D` で消し終えたら、残っている `d` のエラーを消し、`Deleted a.txt` と言う

- **踏んだこと**: 12.11a。`d` で `… Use D to delete permanently` の赤いエラーが出て、言われたとおり `D` → `y` で消した。ファイルは消えたが、
  画面には同じ赤いエラーが残ったまま（`12a-Dy.png`、`toast:` も同じ文）。`D` は成功しても何も言わない。
- **どう変えるべきか**: `D` が終わったら `Deleted a.txt`（`d` の `Trashed <name> — u to undo` と同じ形）を出し、前のエラーを置き換える。
- **なぜ**: 消したあとも「消せなかった」という赤い文が残るので、本当に消えたのかを一覧で探して確かめることになる。エラーが案内した操作の結果を、エラーが隠している。
- **大きさ**: 1 関数（`D` の完了で toast を 1 つ）。

### Votes

- 開いている `投票中` の質問は無い（QUESTIONS.md に `状態: 投票中` が 1 件も無い）。今回の票は無し。

### 後片付け

- 起動した `filer.exe` はすべて終了した。
- 人の設定ファイルには触れていない。テスト用のファイルは R: の一時ディレクトリの中だけに作り、証拠は `C:\dev\filer-evidence\win-retests-8\` に写した。
