# 節 50（Claude Code が窓を読む）— ARM64（31eb3c1 / 0.85.6、arm レーン、無人の run）

ARM64 の表の「Re-tests」は修正待ちの行しか残っておらず（1.44 / 33.7 / 16.13 / 16.13a / 25.4e は TODO.md のまま、0.85.4〜0.85.6 でコードは直っていない）、
TESTING-KEYS.md の未チェックは 0 件だった。そこで「どのキューも持たない行」から、#301 の提案 1 で挙がった **節 50（`filer mcp`、0 / 7）** を取った。
**50.1〜50.6 は ARM64 で通り、`[x]` を付けた。50.7 は半分（同じ答え）だけ通り、`[ ]` のまま。**

## 環境

| 項目 | 値 |
| --- | --- |
| 機械 | Windows 11 Home（build 28000）、aarch64、Qualcomm Adreno X2-90 |
| ビルド | `cargo build --release`（0.85.6）、`filer env` は `Process arch : aarch64`、`filer.exe` / `filer.com` の PE machine は `AA64` |
| 昇格 | なし |
| 持ち主の窓との分離 | 全部の run で `FILER_ADDRESS=\\.\pipe\filer-arm-tN` を窓と `filer mcp` の両方に渡した（既定の `\\.\pipe\filer-<user>` は使っていない）。`filer.toml` の行は `FILER_CONFIG_HOME` を scratch の空の箱に向けて試し、持ち主の設定には触っていない |
| Claude Code | 持ち主の `claude mcp add` は使わず、`claude -p --mcp-config <scratch の JSON> --strict-mcp-config --model haiku` で足した（登録は残らない） |

## cargo test

**741 passed; 0 failed**。親の無い `OpenConsole` / `pwsh` / `powershell` は前後とも **48**（増えず。人の窓が混じる）。

## 事前の後始末

- ごみ箱: scratch の根の下が元の場所のものは **38 件**（全体 65 件）。前の run と同じく消していない（無人では確認の箱に答える人がいない）。この run は `d` を押していないので増減なし。
- Chrome: 人の窓なので触っていない。ブラウザを開く行は取っていない。

## ARM64 の結果（証拠: `C:\dev\filer-evidence\arm-retests-4\`）

道具: PowerShell から `filer mcp` に JSON-RPC を行ごとに流す小さなドライバ（scratch の `mcpdrv.ps1`）と、`claude -p` の `stream-json`。
窓の側の本当の状態は `<State:>` のファイル（`w1sel.txt` ほか）で読み、`filer_state` の答えと突き合わせた。

- **50.1 ✓** `claude -p … --mcp-config` の `init` イベントが `mcp_servers: {"name":"filer","status":"connected"}`、ツールは `mcp__filer__filer_reveal` と `mcp__filer__filer_state` の 2 つだけ（`claude-stream.jsonl`）。
  行は対話の `/mcp` と書くが、同じ接続状態を `claude` 自身が出した文字で読んだ。`/mcp` の画面そのものは押していない。`tools/list` を直接流しても 2 つ（`run1.log`）。
- **50.2 ✓** 窓 `fx\` で `<Space><Space>`（`f1.txt`、`f2.txt` を選択、カーソルは `f3.txt`）。`claude` が `filer_state` を呼び（`tool_use: mcp__filer__filer_state`）、
  答えは `C:\Users\yuu06\AppData\Local\Temp\filer-scratch\run-20261009-232005\fx`、カーソル `…\fx\f3.txt`、選択 `…\fx\f1.txt` と `…\fx\f2.txt`、全部 `C:\…` のフルパス。
  `<State:w1sel>` は `selected: 2`、`cwd` も一致。`filer_state` の JSON は `selected_count: 2`、`overlay: none`、`view: list`。
- **50.3 ✓** `claude` が `filer_reveal {"path":"C:\\Windows\\notepad.exe"}` を呼び、答えは `filer is showing C:\Windows\notepad.exe`。窓は触っていない（クリックなし）のに、
  直後の `filer_state` は `cwd=C:\Windows`、`hovered=C:\Windows\notepad.exe`、窓自身の `<State:>` も `cwd: C:\Windows` / `hovered: C:\Windows\notepad.exe` / `selected: 2`（選択は動かない）。
  無いパス `…\fx\nope.txt` は `… does not exist`（`isError: true`）、直後の `filer_state` は `C:\Windows` / `notepad.exe` のまま（窓は動かない）。
  相対パス `f1.txt` は `f1.txt is not an absolute path`。
- **50.4 ✓** 窓が無いとき（最初の呼び出しと、`Stop-Process` で自分の窓を閉じた後）`filer_state` は
  `filer is not running (or its [mcp] enable is false in filer.toml); start filer and ask again`（`isError: true`）。`filer mcp` は生きたまま、続けた `tools/list` に答えた。
  `claude -p` に同じ道具を頼むと、同じ文を引用して答え、`mcp_servers` は `connected` のまま（`claude-stream-none.jsonl`）。
- **50.5 ✓** `FILER_CONFIG_HOME` を向けた scratch の `filer.toml` に `[mcp]` / `enable = false` を書いて窓を起動 → 窓は生きているのに同じ `filer is not running …`。
  行を消した（ファイルごと消した）同じ設定の窓では `filer_state` が JSON を返す（対照）。
- **50.6 ✓** `fx\` の窓 1 と `fx2\`（カーソル `g2.txt`）の窓 2。窓 1 が持つ扉に、`filer_state` は窓 1 の `fx\`（カーソル `f1.txt`）。窓 1 が `<Quit>` で閉じた直後は `not running`、
  **12 秒後**に窓 2 の `fx2\`、カーソル `g2.txt` に変わった（扉の取り直しは 30 秒ごと、行の「30 秒待つ」の範囲内）。
- **50.7 ✗（`[ ]` のまま）** 同じ答えの半分は ✓: `filer.com mcp` を `claude -p --mcp-config` と、ドライバの両方から起動して 50.2 と同じ答え
  （`claude-stream-com.jsonl`: `mcp_servers connected`、cwd・カーソル `f3.txt`・選択 2 つ、`filer.com` は `AA64`、subsystem 3 = console）、`filer.com` は stdin を閉じると終了コード 0 で終わる。
  残り半分「Claude Code が起動するときにコンソールの窓が出ない」は、窓が出る一瞬を文字や状態で読む手が見つからなかった（`claude -p` は子を隠して起動し、
  対話の Claude Code が同じかは分からない。窓の列挙を一瞬で拾う手はあるが、取りこぼしが「出なかった」と区別できない）ので外観の行として人に残す。

## 押さなかった行

| 行 | 理由 |
| --- | --- |
| 1.44 / 33.7 / 16.13 / 16.13a / 25.4e | TODO.md の修正待ち（前回と同じ） |
| 50.7 の後半 | 上のとおり |
| 49.4 / 49.5 / 49.7 / 49.8、33.23 / 41.15、16.3 系、23.4b / 23.4c、37.8b | #301 と同じ理由（絵の判定、Word・UNC のホストが無い、人の Chrome） |

## 所見

なし（新しい不具合はない）。

### Proposals

1. **`filer mcp` を JSON-RPC で叩く道具を `scripts\` に置く（`scripts\mcp.ps1 -Address … -Tool filer_state`）。**
   今回は `Start-Mcp` / `Rpc` / `Call` を scratch に書いた（`$args` という引数名が PowerShell の自動変数で潰れて `path is missing` を読み、直すのに 1 回余計に回した）。
   節 50 の行は `claude` を呼ばなくても `FILER_ADDRESS` と `<State:>` で読めるので、キーのときの `scripts\keys.ps1` と同じ形の道具があると次のレーンが同じ手間を繰り返さない。
   窓を生かす待ちは `<Wait:>` が 60000 までなので `<Wait:55000>` を重ねる（`<Wait:150000>` は `keys: refused`、`why: … up to 60000` で文面は分かりやすかった）。小。
2. **TESTING.md の節 50 の前書きに `FILER_ADDRESS` を書く。** 今は `claude mcp add filer -- "<folder>\filer.exe" mcp` で持ち主の既定の扉を使うと書いてあり、
   持ち主が自分の filer を開いている機械では、行を押す窓がその窓と扉を奪い合う（先に開いた方が持つ）。`env` に `FILER_ADDRESS` を渡した `claude mcp add --env`
   （または `--mcp-config`）の書き方を前書きに足せば、50.6 の「窓を 2 つ」も持ち主の窓と混ざらない。小（文書）。
3. **50.1 の「`/mcp`」は `claude -p … --output-format stream-json` の `init` で読める。** その旨を行に書き、取る側は対話を要らなくする（小、文書）。
   50.7 のコンソールの窓だけが人に残る行になる。`filer.com` を `CREATE_NO_WINDOW` で起動する設計なら、その意図を行に書くと外観の判定を減らせる。

### Votes

開いている票（`状態: 投票中`）は origin/main の QUESTIONS.md に無い（7 件とも反映済み）。票は 0。

## 後始末

起動した `filer.exe` の窓と `filer mcp` / `filer.com` はすべて終了した（`Get-Process filer` が空）。`claude -p` は 3 回（haiku）。
`FILER_ADDRESS` と `FILER_CONFIG_HOME` は run の中だけ。持ち主の設定とごみ箱・ブラウザには触っていない。
