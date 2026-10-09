# filer — ビルド・検証の詳細

CLAUDE.md から分けた（2026-10-07）。CI・clippy・テストの書き方・Linux での検証を触るときに読む。

## ビルド・検証（PowerShell）

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo test          --manifest-path C:\dev\filer\Cargo.toml
cargo clippy        --manifest-path C:\dev\filer\Cargo.toml
cargo build --release --manifest-path C:\dev\filer\Cargo.toml
```

- **`cargo fmt` は走らせない。**このリポジトリは手で整形してある（`Self { a, b, c }` を 1 行に
  収める書き方）。`rustfmt.toml` は無く、CI も整形を検査していないので、`cargo fmt` を一度
  走らせると**触る必要のないファイルまで既定スタイルへ書き換わる**（v0.33.13 では 47 ファイル
  ・4300 行が混入した）。`use_small_heuristics = "Max"` を当てても 238 か所が合わないため、
  設定で追従することもできない。整形の確認が要るときは `cargo fmt --check` で見るだけにする。
- clippy は `--all-targets` で警告ゼロを保っている。増やさないこと。
  **v0.33.0 から CI が `-D warnings` で強制している**ので、警告は即 CI 落ちになる。
- **検証は CI と同じ stable で回すこと。**`rustup update stable` してから
  `cargo +stable clippy`。**古い版で通っても意味がない。**
  - 実際、手元の 1.95 で緑だったコードが、CI の 1.98 で落ちたことがある
    （`chunks_exact_to_as_chunks` が 1.98 で追加された lint だった）。
    **lint は rustc の更新で増えるので、版が違えば別のものを見ている。**
  - egui 0.36 が要求するのは 1.95 以上。それは**下限**であって、検証に使う版ではない。
- **CI の待ちを増やさない運用。**1 本 5 分、今日は 40 本 209 分だった日がある。
  - **CI を待って止まらない。**回っている間に次を進め、緑になった時点でマージする。
  - **1 つの PR に何度も push しない。**検証は手元で済ませ、まとめて 1 回上げる
    （`concurrency` が古い run を打ち切るので事故にはならないが、回数自体を減らす）。
  - **版の桁で CI を切り替えない。**PATCH こそバグ修正そのもので、一番検証が要る。
    版は人が手で決めるラベルなので、打ち間違いで検証が黙って無効になる。
    切るなら**変わったファイル**で切る（`ci.yml` の `paths-ignore`）。
- CI は `.github/workflows/ci.yml`（Windows と Linux でテスト、Linux で clippy。Linux のテストは v0.75.22、Q74）、
  **macOS のテストは回さない**（2026-10-04、持ち主の指示。課金が Linux の 10 倍。v0.75.22 に足した週 1 回の `test-macos.yml` は v0.75.25 で消した）、
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

**push の前は `scripts/verify.sh` を 1 回回せば足りる**（v0.78.35）。テスト、clippy（Linux と Windows の 2 ターゲット、`-D warnings`）、
ビルド、チェック表 3 つの `--check` を順に回し、1 つでも落ちたらその出力を出して止まる。最後の行が `ALL OK: test result: ok. …` なら push してよい。
`main` へ push するときは `scripts/push-main.sh`（v0.78.38）がこれを回してから push するので、別に回さなくてよい（「自動実行モード」）。

- ただし型検査なので実行時の問題は捕まえない。実際、`format!("{:?}", "status")` が
  ディレクトリ名に引用符を入れ（Unix では合法、Windows では不正）、CI で初めて落ちたことがある。
  パス文字列は目視でも確認すること。
- **`cargo test` は Linux でも緑になる（v0.44.1 以降）。**以前は `exec::tests::*` 3 件と
  `util::tests::normalizes` が Linux 限定で落ち、「既知の 4 件」として扱っていた。
  **その状態には戻さないこと** — 失敗件数を数えて判断するようになると、5 件目を見落とす。
  Windows 固有の入力を assert するテストは `#[cfg(windows)]` を付け、
  プラットフォームに依らない規則を試すテストは**双方で走る形に書く**
  （`exec.rs` の `sample()` がその形。`cmd_s_c_arg` のコメントも同じ理由を書いている）。
- **テストは走らせた機械の設定ファイルに左右されてはいけない。**CI にもクラウドにも設定が無いので、左右されても
  緑のまま通り、設定のある実機でだけ落ちる（v0.47.27 と #136 の 2 回）。設定を読む・一覧するところを触ったら、
  偽の設定を置いて回す: `XDG_CONFIG_HOME=<yazi/ と filer/ に空の yazi.toml などを置いたフォルダ> cargo test`
  （Linux では `XDG_CONFIG_HOME` が設定フォルダの親になる）。
- **Xvfb でマウスを動かすときは `xdotool`（v0.80.16 で確かめた）。**ドラッグは `mousedown 1` のあと `mousemove` を 0.1 秒ずつ数段に分け、
  `mouseup 1` で離す（18.7a の 4 つがどれも通った）。窓マネージャーが無いのでキーボードのフォーカスはどの窓にも無く、`xdotool keydown shift` などの
  修飾キーも `xdotool key` も届かない。先に `xdotool windowfocus <窓>`（`xdotool search --name .` で引く）を打つ。キーだけなら `--keys` のほうが確か。
- **テスト用の一時ディレクトリは `util::test_dir("ラベル")` を使う。**自分で
  `env::temp_dir().join(...)` を組まないこと。名前はテストのスレッド名から取るので、
  2 つのテストが同じディレクトリを渡されることがない（手書きしていた頃、同名を共有した
  3 テストが並列実行で互いのツリーを消し合うバグが出た）。
  **呼ぶたびに中身を消す**ので、1 つのテストで複数のファイルが要るときは
  1 回呼んでから `join` すること。名前は `filer-test-` で始まり、プロセスごとの最初の呼び出しで、
  ほかのプロセスが 1 時間以上前に残した `filer-test-…-<pid>` を消す（v0.73.70、#227）。
