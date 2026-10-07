# filer — 版・タグ・リリースの詳細

CLAUDE.md の「ブランチ・バージョン・変更ログ」から分けた（2026-10-07）。リリースを切るとき、タグや成果物を触るときに読む。

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
  - **前のリリースが完了してから次を投げること。**「前のタグ」は
    `git describe --tags --abbrev=0 HEAD^` で探すので、**タグができる前に次を投げると、
    さらに前のタグまで遡って、前回と丸ごと重複したノートになる。**
    タグは workflow の最後に作られるため、投げた直後にはまだ無い。
  - 成果物は 6 つ。Windows は `.zip`（v0.71.0 から `filer.com` も入る。`src/bin/filer-com.rs` のコンソール用の前段で、
    `filer env` などを PowerShell が待てるようにする。Q44）、macOS / Linux は `.tar.gz`
    （リリースのアセットは**実行ビットを保持しない**ので、生のバイナリだと
    `chmod +x` が要る。tar は保持する）。
  - **Windows の zip には、新しい ConPTY（`conpty.dll` と `OpenConsole.exe`）を同梱する**
    （v0.49.0 から）。Windows 標準の ConPTY は古く、ペインで動かす lazygit などを壊す。
    取得は `scripts/fetch-conpty.ps1` の 1 か所で、版と SHA-256 はそこで固定している。
    **版を上げるときは、実機で新しい版を試してから、版とハッシュを一緒に変えること。**
    手元でビルドしたときも、このスクリプトで `target\release` に置かないと古い ConPTY で動く。
    ビルドの後に回すと、`filer-com.exe` を `filer.com` に写すのもこのスクリプト（v0.73.66、#225）。
    **`filer.exe` の横に無いとき、作業フォルダや `PATH` の `conpty.dll` は読まない**（v0.70.3、#184。`main` の最初で
    `SetDefaultDllDirectories` を呼び、名前で読む DLL を exe のフォルダと System32 に限っている）。
  - **最後に `sums` ジョブが、全成果物と Windows の zip の中身（ファイルごと）の SHA-256 表をノートの末尾に足す**
    （v0.64.0、`scripts/release-sums.sh`。`<!-- checksums -->` から後ろを置き換えるので再実行しても重ならない）。
    手元で試すときは、成果物を置いたフォルダでこのスクリプトを回す。
    **出たあとのリリースに表を載せ直すときは、Actions タブから `release-sums.yml` を `tag` を渡して回す**（v0.64.5。
    v0.64.2 は `mkdir assets` がリポジトリの `assets/` とぶつかって表が載らず、これで後から載せた）。
  - ノート末尾の「Platforms」の節は `release.yml` が固定で付ける。**Windows 以外は
    ビルドが通ることしか分かっていない**ことを、毎回そこで明示している。
