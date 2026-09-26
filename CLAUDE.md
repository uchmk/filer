# filer — Claude 向けメモ

yazi 風のキーボード操作ファイルマネージャーを Rust + egui 0.36 で作った GUI 版。yazi の設定ファイル（`yazi.toml` / `keymap.toml` / `theme.toml`）をそのまま読む。Windows 優先。

## 作業ルール

- 返答は日本語。コード・コメント・コミットメッセージは英語。
- 頼まれるまでコミットしない（例外は下の「自動実行モード」）。
- 実行中の `filer.exe` を止めるときは事前確認を不要とし、ビルドを優先する。
- 改行は LF（`.gitattributes` の `eol=lf`）。スクリプトで書き換えるときは改行を変えない（Python なら `newline=''`）。
- タスクは [TODO.md](TODO.md)、人への確認事項は [QUESTIONS.md](QUESTIONS.md) で管理する（書き方は「確認事項」の節）。
- **実機でしか確かめられないことは [TESTING.md](TESTING.md) に積む。**画面の無い環境で書いた機能は、
  ここにチェック項目として足してから完了にする。テスト用ファイルは `scripts/make-fixtures.ps1` が作る。

## ブランチ・バージョン・変更ログ

一人開発なので、`main` への直接コミット・push を許可する。区切りたいときはブランチと PR を
使ってもよい（どちらでもよい。PR にするなら CI が緑になってからマージする）。

### セマンティックバージョニング

`MAJOR.MINOR.PATCH`。版は `Cargo.toml` の `version` が正とする。

| 上げる桁 | 対象 | 例 |
| --- | --- | --- |
| PATCH | バグ修正、ドキュメント更新、内部の小さな整理 | 0.0.1 → 0.0.2 |
| MINOR | 新機能の追加、大きめの変更 | 0.0.2 → 0.1.0 |
| MAJOR | 正式版リリース、大改修 | 0.1.1 → 1.0.0 |

- 1.0.0 未満なので、MINOR が破壊的変更を含んでもよい（semver の 4 項）。
  既定キーの変更や設定ファイルの互換性に関わる変更は、CHANGELOG に**変更**として明記する。
- 迷ったら小さいほうに倒す。機能が入っているなら MINOR。

### `main` にコミット・push するときの手順

**バージョンの繰り上げと [CHANGELOG.md](CHANGELOG.md) への追記はセットで行う。**片方だけを
コミットしない。

1. 変更の大きさから桁を決める（上の表）。
2. `Cargo.toml` の `version` を書き換える。
3. `cargo build`（または `cargo test`）を一度回して `Cargo.lock` の `filer` の版も更新する。
   これを忘れると CI の `cargo build --locked` が落ちる。
4. CHANGELOG.md の先頭に新しい版の節を足す。形式は
   [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に寄せる（`### 追加` / `### 変更` /
   `### 修正` / `### 削除`）。日付は `YYYY-MM-DD`。
5. コードと一緒にコミットする。コミットメッセージは英語。

- 1 回の push に複数の変更が入るなら、版は 1 つだけ上げてまとめて書く。
- ドキュメントだけの変更でも PATCH を上げる。
- タグは `v0.32.2` の形。**全部の版に打つわけではない**ので、`main` への push と
  リリースは別物として扱う。
- **リリースの切り方**: `.github/workflows/release.yml` を Actions タブから
  `workflow_dispatch` で回し、`tag` に `vX.Y.Z` を渡す（**タグが無ければ作られる**）。
  `Cargo.toml` の版と一致しないと落ちる。
  - `git push origin <タグ>` は**クラウドセッションからは 403 で拒否される**
    （認証情報が `refs/heads/*` に限定されている）。上の手動実行を使うこと。
  - リリースノートは**前のタグからのコミットメッセージ**から作られる。だから
    `vX.Y.Z:` で始まるコミットの**最初の段落**が、そのままリリースページに出る。
    英語で、それ単体で意味が通るように書くこと。CHANGELOG.md（日本語）は使われない。

## ビルド・検証（PowerShell）

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo test          --manifest-path C:\dev\filer\Cargo.toml
cargo clippy        --manifest-path C:\dev\filer\Cargo.toml
cargo build --release --manifest-path C:\dev\filer\Cargo.toml
```

- clippy は `--all-targets` で警告ゼロを保っている。増やさないこと。
  **v0.33.0 から CI が `-D warnings` で強制している**ので、警告は即 CI 落ちになる。
- **検証は CI と同じ stable で回すこと。**`rustup update stable` してから
  `cargo +stable clippy`。**古い版で通っても意味がない。**
  - 実際、手元の 1.95 で緑だったコードが、CI の 1.98 で落ちたことがある
    （`chunks_exact_to_as_chunks` が 1.98 で追加された lint だった）。
    **lint は rustc の更新で増えるので、版が違えば別のものを見ている。**
  - egui 0.36 が要求するのは 1.95 以上。それは**下限**であって、検証に使う版ではない。
- CI は `.github/workflows/ci.yml`（Windows でテスト、Linux で clippy）、
  `audit.yml`（`cargo audit`、push と週 1 回）、`build.yml` が 6 ターゲットの
  バイナリをアーティファクトとして残す（Actions タブからダウンロードできる）。

### Linux 上で作業する場合（クラウドセッションなど）

Windows 専用のコード（ConPTY、`is_hidden`、`#[cfg(windows)]` のテスト）は Linux では 1 行も
コンパイルされないため、手元で通っても CI で落ちる。リンクしない型検査なら MSVC ツールチェーン
なしで通せるので、push 前にこれを回すこと。

```bash
rustup target add x86_64-pc-windows-msvc
cargo check --release --target x86_64-pc-windows-msvc --all-targets
```

- ただし型検査なので実行時の問題は捕まえない。実際、`format!("{:?}", "status")` が
  ディレクトリ名に引用符を入れ（Unix では合法、Windows では不正）、CI で初めて落ちたことがある。
  パス文字列は目視でも確認すること。
- `cargo test` の失敗 4 件（`exec::tests::*` 3 件と `util::tests::normalizes`）は Linux 限定。
  Windows のパス表記を前提にしたテストなので、CI（Windows）では通る。

## 自動実行モード

`auto-todo.sh` から `claude -p` で起動されたとき（プロンプトに「自動実行モード」とあるとき）のルール。人は見ていない前提で動く。

- 起動時に未コミットの変更（`git status`）があれば、前の run が利用制限や再起動で中断した作業の残り。新しいタスクは始めず、`git diff` と TODO.md から中断したタスクを特定し、続きを仕上げて検証・コミットする。筋が通らず続けられない変更なら、`git restore` / `git clean` で戻してから通常どおり進める。
- 1 回の起動で進めるのは、TODO.md の未完了（`- [ ]`）タスク 1 つだけ。大きいタスクは小さく区切り、その 1 区切りだけ進める。区切りは TODO.md に子項目として書き足す。
- 人の判断が要るタスクは飛ばし、「確認事項」の節の手順で質問を書く。例: キーの割り当て、依存クレートの追加、設計の選択肢が複数あるもの、UI の見た目を決めるもの。
  - 回答済みの質問があれば、そのタスクを優先して進める。
- 検証は `cargo test` と `cargo clippy` で行う。
  - テストは全件通すこと。
  - clippy は既存の指摘（never_loop など）を除いて、新しい警告やエラーを増やさないこと。
  - release ビルドは検証に要らないのでしない（test / clippy は debug ビルドなので、`filer.exe` が起動中でも動く）。
- タスクが 1 つ終わるたびにコミットとプッシュをする。検証が通ったら、TODO.md のチェックを更新し、
  「ブランチ・バージョン・変更ログ」の手順（版の繰り上げ＋ CHANGELOG.md）を踏んで `git commit` し、
  続けて `git push -u origin <今いるブランチ>` する。
  - コミットメッセージは英語。
  - **ブランチは切り替えない。**スクリプトが用意したブランチにそのまま積む（`auto-todo.sh` なら
    `auto/todo`）。人が見ていない前提なので、自動実行モードからは `main` へ push しない。
    対話中の `main` 直コミットは許可しているが、それは人が見ているときの話。
  - `--force` は使わない。
  - push に失敗したら（認証・ネットワークなど）、コミットはそのまま残して終わる。次の起動で、またはスクリプトの後処理で push し直す。
- 検証が通らず直せなかった場合は、変更を `git restore` / `git clean` で戻し、TODO.md に失敗の理由を書く。コミットはしない。
- 自動で進められるタスクが残っていなければ、何も変更しない。自動で進められるタスクとは、`要確認` の付いていない未完了タスクと、質問が回答済みになったタスクのこと。
  - そのときは「未回答の確認事項 N 件（QUESTIONS.md）」と出力し、最後の行に `ALL_DONE` とだけ出力する。

## 確認事項（QUESTIONS.md）

人の判断が要るときは、作業を止めて QUESTIONS.md に質問を書く。自動実行モードでも、対話中でも同じ。TODO.md には、どのタスクがどの質問で止まっているかだけを書く。

- **質問の書き方**: QUESTIONS.md の末尾に、下の形式で追記する。
  - 番号は通し番号で、再利用しない。
  - 選択肢には必ず推奨を 1 つ付ける。
  - 回答が来なくても推奨で進めてよい軽い事項なら、そう書き添える。
- **TODO.md への印**: 該当タスクの末尾に `（要確認: Q7）` と付ける。
- **回答の反映**
  1. 人は、質問の「回答」欄に書き込む。
  2. 回答が書かれていたら、その内容でタスクを進める。
  3. 進めたら、状態を `反映済み` にし、TODO.md の `（要確認: Qn）` を外す。
  4. 回答があいまいなら、同じ質問に追記して聞き直す（状態は `未回答` に戻す）。

```markdown
## Q7: 2 分割ペインのフォーカス移動キー
- 状態: 未回答            <!-- 未回答 / 回答済み / 反映済み -->
- タスク: TODO.md「1. 2 分割ペイン」
- 背景: `Tab` は spot に割り当て済み。yazi 互換を保つなら別のキーが要る。
- 選択肢:
  1. `<C-w>`（推奨）: vim のウィンドウ切り替えと同じで覚えやすい。
  2. `<F6>`: Explorer と同じ。F6 を移動に使う案とぶつかる。
  3. `Tab` を移し、spot を別のキーにする。
- 回答:
```

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
| `src/exec.rs`  | オープナーのテンプレート展開、行番号付き起動（`at_line`）、クリップボード                                                               |
| `src/ui/`      | 描画。`list.rs`、`preview.rs`、`overlay.rs`（help / tasks / confirm / pick / spot など）                                                |

## 設計の約束事

- ディスクに触る処理は UI スレッドで実行しない。ワーカースレッドと crossbeam チャネルを使い、古い依頼は捨てる（newest-wins）。
- キーは keymap 経由で動かす（ハードコードしない）。新しいオーバーレイには keymap のセクションと `feed_*_key` を用意する。
- yazi に無い独自コマンドは README の「Commands implemented」に書く。
- 機能を足したら README（キー表・各節）も更新する。

将来的にこのアプリ（Yazi GUI版）を Windows / macOS / Linux（それぞれ x86_64 / ARM64） で動作させたいと考えています。

今後のコード実装や設計案を提案する際は、以下の点に配慮してください：

マルチOS対応: Windows（Win32/UNCパス）、macOS（/ パス、Cmdキー）、Linux（X11/Wayland）の差分を意識し、OS固有処理は #[cfg(target_os = "...")] などで適切に分離・抽象化すること。

マルチアーキテクチャ対応: x86_64 に加え、Apple Silicon (M1〜M4) や Linux ARM64 などへのクロスコンパイルが容易なライブラリ構成（可能な限り Pure Rust クレートを優先）にすること。

キーバインドの柔軟性: macOS では Cmd キー、Windows/Linux では Ctrl キーを自動判定・差し替えできる修飾キー抽象化を入れること。
