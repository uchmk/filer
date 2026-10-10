# LLM との連携 — 案

2026-10-09 に書いた。持ち主の希望は「kura・tsumugi・これから作るアプリを LLM とつなげたい。ソースを使い回せるかも含めて課題にしたい」。
案を並べて 1 つを勧めた。持ち主は Q95・Q96 とも推奨の 1 を選んだ（2026-10-09）: 読むだけの道具の MCP サーバー、口は既定で入れる、共有のクレートは tsumugi に置き MCP は自分で書く。
段の 1〜3 は済み（tsumugi v0.75.0、kura v0.85.0）。4 は実機の TESTING.md 50 節を待つ。

## 言葉

- **MCP**（Model Context Protocol）: LLM のクライアント（Claude Code、Claude Desktop など）が外の「道具（tool）」を呼ぶための約束。
  クライアントがサーバーのプロセスを起こし、標準入出力で JSON-RPC をやり取りするのが一番普通の形（stdio）。
- **開発のセッションの MCP**（GitHub・Slack・Obsidian などのコネクタ）は別の話で、TODO.md の「MCP サーバーの認証・接続」。この文書はアプリの機能の話。

## 今あるもの（使い回せる部品）

| どこ | 何 | LLM 連携での使い道 |
| --- | --- | --- |
| kura | `[opener]` と `shell` のバインド、`send_pane` | kura → 外への一方通行。今でもペインの Claude Code に選択したパスを送れる |
| kura | `--keys` と `<State:>`（`KURA_KEYS_DONE`） | テスト用だが、「画面の状態を文字にする」関数（`state_report`）がもうある。道具の返す中身の下書きになる |
| kura | `kura env` | サブコマンドの入口（`src/main.rs`）。`kura mcp` を足す場所 |
| tsumugi | `tsumugi-mux` の `transport`（Unix のドメインソケット / Windows の名前付きパイプ、約 320 行） | アプリの外からアプリにつなぐ口。そのユーザーだけがつなげる設定（0700、`PIPE_REJECT_REMOTE_CLIENTS`）まで済んでいる |
| tsumugi | 長さ＋ postcard の枠、版の言い合い（`Hello`） | 同上 |
| tsumugi | `tsumugi notify`（Claude Code のフックから呼ぶ） | LLM → アプリの方向の小さな前例 |

kura にはもう `serde_json` が入っている（`Cargo.lock` にある）。`tokio` は入っていない。

## 案

### (a) アプリが MCP サーバーになる

Claude Code から「kura が今見ているフォルダ・カーソル・選択」を読み、「このファイルへ飛べ」「このフォルダを開け」と言える。
QUESTIONS.md の Q16 の案 3（別アプリと双方向の API）は、これがそのまま答えになる。

GUI のプロセスは標準入出力を持たないので、**同じ exe の `kura mcp` が橋になる**:

```
Claude Code ──stdio(JSON-RPC)── kura mcp ──名前付きパイプ / ソケット── kura（窓、動いているもの）
```

- `kura mcp` はクライアントが起こす。窓が動いていなければ「kura が動いていない」と返す（窓は起こさない。最初の版）。
- 窓が複数あれば、最初に口を開けた窓が答える。ほかの窓は 30 秒ごとに口を開け直そうとするので、その窓を閉じると次の窓が引き継ぐ（道具で窓を選べるようにするのは後）。
- tsumugi も同じ形で `tsumugi mcp` を足せる。サーバー（`tsumugi server`）がもうあるので、道具はそこに足すだけ。

道具の例（最初は読むだけ）:

| 道具 | 返すもの / すること |
| --- | --- |
| `kura_state` | 今のフォルダ、カーソルの下のパス、選択したパス、タブの一覧 |
| `kura_reveal` | パスを受けて、そのフォルダに移ってカーソルを置く（Q16 の「このファイルへ飛べ」） |
| `tsumugi_sessions` | セッションの一覧と状態（待っている・動いている）。`tsumugi ls` と同じ中身 |
| `tsumugi_screen` | 1 つのセッションの見えている画面の文字 |

ファイルの中身を読む道具は作らない。Claude Code は自分でファイルを読めるので、アプリが持つ価値は「**人が今どこを見ているか**」のほうにある。

書く道具（コピー・移動・削除・名前の変更、セッションへの入力）は 2 段目。足すときは、**アプリの窓に確認の箱を出して人が押すまで動かさない**のを約束にする。

### (b) アプリが LLM を呼ぶ

選択したファイルの要約、一括の名前の変更の案、エラーの説明などを、アプリの中から LLM に頼む。

- 呼び方は 2 通り。API キーで直接呼ぶか、ペインで動いている Claude Code に送るか（後者は `send_pane` で今もできる）。
- 直接呼ぶには HTTP と TLS の依存が要る。Pure Rust の組み合わせ（`ureq` + `rustls`）でも、`rustls` の暗号の部分（`ring` か `aws-lc-rs`）は C やアセンブリを含み、
  6 ターゲットのクロスコンパイルが今より重くなる（要確認。`ring` は ARM64 の Windows も通るはずだが、試していない）。
- API キーの置き場所（Windows の資格情報マネージャー、macOS のキーチェーン、Linux の Secret Service）、何を外に送るかの確認、料金、オフラインのときの動き。
- Q16 で持ち主が選んだ「kura はローカルのまま、速いまま」と、46.8 / 46.9 の「待たない・黙って壊れない」に、ネットワークが入ってくる。

### (c) 両方

(a) の口と (b) の呼び出しを同時に作る。手間が倍になり、決めることも倍になる。

## 比べる

| | (a) MCP サーバー | (b) LLM を呼ぶ | (c) 両方 |
| --- | --- | --- | --- |
| 新しい依存 | 無し（`serde_json` は入っている。MCP を自分で書く場合。下の「プロトコルの実装」） | HTTP・TLS・資格情報ストア | 両方 |
| クロスコンパイル | 変わらない | 重くなる | 重くなる |
| UI スレッド | 口を読むのはワーカー。返す中身は画面の状態の写しなので、ディスクを待たない | 応答を待つワーカーが要る | 両方 |
| 安全 | そのユーザーだけがつなげる口。書く道具は確認の箱 | キーの管理、外に出すデータの確認 | 両方 |
| 使い回し | 3 つのアプリで同じクレート | 同上 | 同上 |
| 今日の使い方への効き目 | 大きい。持ち主は Claude Code を tsumugi で並べ、kura で場所を見ている。その 2 つを Claude Code が読める | 中くらい。ペインの Claude Code で大体できる | — |

## 勧める形: (a) を読むだけの道具から

1. **(a) を、読むだけの道具（`kura_state`・`kura_reveal`・`tsumugi_sessions`・`tsumugi_screen`）で始める。**
   依存が増えず、クロスコンパイルにも響かず、窓に口を開けても書き換えはできない。
2. 使ってみて足りないものが見えたら、書く道具を確認の箱つきで足す。
3. (b) は、ペインの Claude Code で足りないと分かってから考える（Q16 と同じ「先に試す」の順番）。

`kura_reveal` は人の画面を動かすので、厳密には「読むだけ」ではない。ただ、ファイルは何も変わらず、人がいつでも戻れるので最初の組に入れる。

## 使い回しの形（共有のクレート）

`tsumugi-pane` と同じく tsumugi のリポジトリの `crates/` に置き、kura は `Cargo.toml` の `rev` で固定して読む（v0.93.10 から、どれも uchmk/ito の `ito-*`）。

| クレート（仮名） | 中身 | 出どころ |
| --- | --- | --- |
| `tsumugi-ipc` | アプリへの口: ソケット / 名前付きパイプ、枠、版の言い合い | `tsumugi-mux` の `transport` と枠を切り出す。`tsumugi-mux` はこれを使う形にする |
| `tsumugi-mcp` | stdio の JSON-RPC、`initialize` / `tools/list` / `tools/call`、道具の登録（名前・説明・引数の JSON Schema・関数）、書く道具の確認の約束 | 新しく書く |

アプリの側でやることは、道具を登録する関数を書くことと、口から来た依頼を UI スレッドで状態の写しにして返すことだけになる。
これから作るアプリも同じ 2 つを入れれば、Claude Code から読める。

### プロトコルの実装

- **自分で書く（勧める）**: 最初の版に要るのは `initialize`、`tools/list`、`tools/call`、`notifications/initialized` の 4 つで、`serde_json` で数百行。
  依存が増えない。プロトコルの版が上がったら追いかける手間がある。
- **公式の Rust SDK（`rmcp`）を使う**: プロトコルの追従は SDK に任せられる。`tokio` を引き込む（要確認。少なくとも stdio の transport は非同期で書かれている）。
  kura も tsumugi も今は `tokio` を持たず、スレッドとチャネルで書いている。

## OS ごとの違い

- 口は `tsumugi-mux` と同じ: Windows は `\\.\pipe\kura-<ユーザー名>`、Unix は `$XDG_RUNTIME_DIR/kura/sock`（無ければ `/tmp/kura-<uid>/sock`、0700）。macOS も Unix の側。
- `kura mcp` は窓を作らないので、Wayland / X11 の違いは関係しない。
- Claude Code の設定に書くのは `kura mcp`（`kura.exe` のフルパス）だけで、OS で変わらない。

## 安全

- ネットワークには口を開けない（ローカルの口だけ）。ほかのユーザーからはつなげない。
- 設定で切れるようにする（kura の `config.toml` の `[mcp] enable`）。既定は入れる（Q95 の 1）。
- 書く道具は 2 段目。足すときは窓の確認の箱を通す。確認の箱を通さない書き込みは作らない。
- 道具が返したものは、Claude Code の側で LLM に渡る。画面に出ているファイル名やペインの文字が外に出ることを README に書く。

## 段

| 段 | 中身 | 状態 |
| --- | --- | --- |
| 0 | この文書。Q95・Q96 | 済み（2026-10-09） |
| 1 | tsumugi: `tsumugi-ipc` を `tsumugi-mux` から切り出す（動きは変えない） | 済み（tsumugi v0.75.0） |
| 2 | tsumugi: `tsumugi-mcp`、`tsumugi mcp` と `tsumugi_sessions` / `tsumugi_screen` | 済み（tsumugi v0.75.0） |
| 3 | kura: 口と `kura mcp`、`kura_state` / `kura_reveal`。Xvfb で、窓を起こして `kura mcp` に JSON-RPC を流して確かめる | 済み（kura v0.85.0） |
| 4 | 実機: Claude Code の設定に足して呼べるか（Windows の名前付きパイプ、TESTING.md に行を足す） | 行は足した（TESTING.md 50 節）。実機待ち |
| 5 | 書く道具（確認の箱つき） | — |
