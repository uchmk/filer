# kura — Claude 向けメモ

yazi 風のキーボード操作ファイルマネージャーを Rust + egui 0.36 で作った GUI 版（v0.95.0 で filer から改名。古い記録には filer のまま残る）。yazi の設定ファイル（`yazi.toml` / `keymap.toml` / `theme.toml`）をそのまま読む。Windows 優先。

**ここには毎回要る規則だけを置く。詳細は `docs/claude/` にあり、該当する作業のときに読む**（2026-10-07、持ち主の判断。全部をここに置くと毎回 18k トークン読み込んでいた）。

| 読むもの | いつ |
| --- | --- |
| [docs/claude/auto-mode.md](docs/claude/auto-mode.md) | 「自動実行モード」で起動されたとき（Routine・`auto-todo.sh`） |
| [docs/claude/build.md](docs/claude/build.md) | CI・clippy・テストの書き方・Linux での検証を触るとき |
| [docs/claude/release.md](docs/claude/release.md) | タグ・リリース・成果物（ConPTY の同梱、SHA-256 の表）を触るとき |
| [docs/claude/lanes.md](docs/claude/lanes.md) | QA・Windows 実機・Linux のセッション、報告とチェック表、マージする側 |
| [docs/claude/questions.md](docs/claude/questions.md) | QUESTIONS.md に質問を書く・回答を反映する・票を数えるとき |

## 作業ルール

- 返答は日本語。コード・コメント・コミットメッセージは英語。
- 頼まれるまでコミットしない（例外は「自動実行モード」）。
- 実行中の `kura.exe` を止めるときは事前確認を不要とし、ビルドを優先する。
- 改行は LF（`.gitattributes` の `eol=lf`）。スクリプトで書き換えるときは改行を変えない（Python なら `newline=''`）。
- タスクは [TODO.md](TODO.md)、人への確認事項は [QUESTIONS.md](QUESTIONS.md)（書き方は docs/claude/questions.md）。
  - 未完了の項目の末尾の `【人】` `【QA】` `【実機】` `【後】` `【pane】` は開発のセッションが進めない印。「次の項目」を探すときは飛ばす。
    `【pane】` はターミナルペイン: コードは uchmk/ito の `ito-pane` にあり、kura は `Cargo.toml` の `rev` で固定して読む
    （直すときは tsumugi 側で直し、`rev` を上げて `cargo build` で `Cargo.lock` を合わせる）。
  - 節の項目が全部済んだら、その節を [TODO-DONE.md](TODO-DONE.md) の末尾に移す。
- **直した分は再テストに回す。**動きが変わった行は TESTING-CHECKS.md の `[x]` を外し、`.claude/windows-role.md` の
  2 つの順番表（x64 と ARM64）の先頭の「Re-tests of changed behaviour」に足す。自分でも Xvfb で `--keys` と `KURA_KEYS_DONE` を使って先に確かめる。
- **実機でしか確かめられないことは [TESTING.md](TESTING.md) に積む**（チェック項目を足してから完了にする）。`[x]` を付けてよいのは実機のセッションと持ち主だけ。

## 版・変更ログ・push

一人開発なので `main` への直接のコミット・push を許す。**マージは merge コミット**（rebase / squash を使わない）。

- semver。版は `Cargo.toml` の `version` が正。PATCH = 修正・ドキュメント・小さな整理、MINOR = 機能・大きめの変更。迷ったら小さいほう。
  既定キーや設定の互換性に関わる変更は CHANGELOG に**変更**として明記する。
- **版の繰り上げと [CHANGELOG.md](CHANGELOG.md) への追記はセット。**手順: `version` を書き換える → `cargo build` で `Cargo.lock` も更新
  （忘れると CI の `--locked` が落ちる）→ CHANGELOG の先頭に節（Keep a Changelog、`### 追加` / `### 変更` / `### 修正` / `### 削除`、
  **日付は JST**: `TZ=Asia/Tokyo date +%F`）→ コードと一緒にコミット。1 回の push に版は 1 つ。ドキュメントだけでも PATCH を上げる。
- コミットの件名は `vX.Y.Z: …`、**最初の段落はそのままリリースノートになる**ので英語で単体で通じるように書く（リリースは docs/claude/release.md）。
- **push の前は `scripts/verify.sh`**（最後の行が `ALL OK: …` なら push してよい）。`main` へは `scripts/push-main.sh`（verify を回し、
  `main` が進んでいれば版を付け直し、CI を待ってから push する。`git merge origin/main` はしない）。`--force` は使わない。

## ビルド・検証

- **`cargo fmt` は走らせない**（手で整形している。確認は `cargo fmt --check` だけ）。
- clippy は `--all-targets -- -D warnings` で警告ゼロ（CI が強制）。検証は CI と同じ stable で回す。
- Linux では `#[cfg(windows)]` のコードがコンパイルされないので、`verify.sh` が `x86_64-pc-windows-msvc` の clippy も回す。
- `cargo test` は Linux でも全件緑。Windows 固有の入力を assert するテストは `#[cfg(windows)]` を付ける。
- テストは機械の設定ファイルに左右されてはいけない。一時ディレクトリは `util::test_dir("ラベル")` を使う。
- 詳細（CI の構成と待ちを増やさない運用、Linux 上の注意）は docs/claude/build.md。

## 自動実行モード

プロンプトに「自動実行モード」とあるとき（`auto-todo.sh`、クラウドの Routine）は **[docs/claude/auto-mode.md](docs/claude/auto-mode.md) を読み、そのとおりに動く。**

## 並行のセッション・確認事項

QA・Windows 実機（x64 / ARM64）・Linux レーン・マージの Routine の約束は [docs/claude/lanes.md](docs/claude/lanes.md)、
QUESTIONS.md の書き方と「多数決で進める質問」は [docs/claude/questions.md](docs/claude/questions.md)。

## 構成

| 場所           | 役割                                                                                                                                    |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `src/main.rs`  | eframe の起動、egui イベント → キー変換、オーバーレイごとの入力の振り分け（`handle_input` / `on_key_event`）                            |
| `src/app.rs`   | アプリ状態と `Act` の実行。`Overlay` 列挙、プレビュー・アウトライン・spot の状態                                                        |
| `src/config/`  | yazi 設定の読み込み。`cmd.rs`（コマンド文字列 → `Act`）、`keymap.rs`（`resolve`）、`keys.rs`、`theme.rs`、`defaults/keymap.toml`        |
| `src/core/`    | タブ・フォルダ・あいまい検索                                                                                                            |
| `src/fs/`      | 一覧のスキャン、ソート、監視、ファイル操作のジョブ                                                                                      |
| `src/preview/` | プレビューワーカー（最新の依頼だけ処理）。テキスト / Markdown / 画像 / SVG / フォント / シェルサムネイル / アウトライン（`symbols.rs`） |
| `src/spot.rs`  | spot パネルの情報。`inspect()` の provider 関数を足せば項目が増える                                                                     |
| `src/terminal.rs` | ターミナルペイン。中身は ito の `ito-pane`（git の依存）。`ui/term.rs` はその描画にテーマの色を渡す                         |
| `src/exec.rs`  | オープナーのテンプレート展開、行番号付き起動（`at_line`）、クリップボード                                                               |
| `src/ui/`      | 描画。`list.rs`、`preview.rs`、`overlay.rs`（help / tasks / confirm / pick / spot など）                                                |

## 設計の約束事

- ディスクに触る処理は UI スレッドで実行しない。ワーカースレッドと crossbeam チャネルを使い、古い依頼は捨てる（newest-wins）。
- キーは keymap 経由で動かす（ハードコードしない）。新しいオーバーレイには keymap のセクションと `feed_*_key` を用意する。
- **uchmk の共通仕様**（設定の画面 Ctrl+,・`<設定のフォルダ>/uchmk/common.toml` の言語・テーマ・時計）は ito の [docs/common-spec.md](https://github.com/uchmk/ito/blob/main/docs/common-spec.md) が正。
  部品は ito の `ito-common`・`ito-theme`・`ito-prefs`（TODO.md「設定の画面・テーマ・時計」）。
- yazi に無い独自コマンドは README の「Commands implemented」に書く。
- 機能を足したら README（キー表・各節）も更新する。
- **Lua は動かさない**（`init.lua` も `plugins/*.lua` も読まない。2026-10-04 に持ち主の依頼で理由を書き残した）。
  - yazi のプラグインが頼っているのは Lua という言語より、yazi が渡す `ya.*`・`cx`（タブや選択の状態）・`ui.*`（画面の部品）・非同期の仕組みで、
    どれも yazi の TUI の内側に直結している。Lua を組み込んでも、これを egui の上に作り直さない限りほとんど動かない。
  - Rust から Lua を動かす定番の `mlua` は C の Lua 本体をビルドに抱える。「Pure Rust を優先し、6 ターゲットへのクロスコンパイルを楽に」に反する。
  - プラグインの主な用途（カーソルの下のファイルに何かする）は、`shell` のバインドと `[opener]` で書け、右クリックメニューとパレットに並ぶ（TODO-DONE の 5 節）。
  - だから、よく使われるプラグインは **1 つずつ kura の機能で置き換える**（`toggle-pane`・`bookmarks`・`smart-enter`・`smart-filter` がその形。
    `src/config/cmd.rs` の `plugin` の扱い）。知らない `plugin …` は設定の読み込みを壊さず、ヘルプに未対応と出し、押せばトーストで言う。
  - Lua の実行環境を入れるなら依存クレートの追加と大きな設計になるので、QUESTIONS.md で範囲を決めてから。

将来的にこのアプリ（Yazi GUI版）を Windows / macOS / Linux（それぞれ x86_64 / ARM64） で動作させたいと考えています。

今後のコード実装や設計案を提案する際は、以下の点に配慮してください：

マルチOS対応: Windows（Win32/UNCパス）、macOS（/ パス、Cmdキー）、Linux（X11/Wayland）の差分を意識し、OS固有処理は #[cfg(target_os = "...")] などで適切に分離・抽象化すること。

マルチアーキテクチャ対応: x86_64 に加え、Apple Silicon (M1〜M4) や Linux ARM64 などへのクロスコンパイルが容易なライブラリ構成（可能な限り Pure Rust クレートを優先）にすること。

キーバインドの柔軟性: macOS では Cmd キー、Windows/Linux では Ctrl キーを自動判定・差し替えできる修飾キー抽象化を入れること。
