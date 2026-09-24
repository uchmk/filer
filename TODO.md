# TODO

## 完了

- [x] 0. ファイル上の → / `l` でアウトラインにフォーカスする。Enter / Shift+Enter で起動し、アウトライン選択中はその行で開く。
- [x] 0.1. `Tab` で spot パネル（ファイル情報）を開く。Windows のプロパティは provider を 1 つ足せば追加できる構造。

## 行番号付き起動の対応エディタを増やす

今の `exec::at_line` が対応しているのは nvim / vim 系（`+N`）、VS Code 系（`-g path:N`）、hx / subl / zed（`path:N`）だけ。それ以外のエディタは行番号なしで開く。

- [x] Windows の定番エディタを追加する。

  | エディタ | 実行ファイル | 引数 | 例（123 行目） |
  | --- | --- | --- | --- |
  | 秀丸エディタ | `hidemaru.exe` | `/jN path` | `hidemaru.exe /j123 "C:\path\file.txt"` |
  | サクラエディタ | `sakura.exe` | `-L=N path` | `sakura.exe -L=123 "C:\path\file.txt"` |
  | EmEditor | `emeditor.exe` | `/l N path` | `emeditor.exe /l 123 "C:\path\file.txt"` |
  | Notepad++ | `notepad++.exe` | `-nN path` | `notepad++.exe -n123 "C:\path\file.txt"` |
  | メモ帳 | `notepad.exe` | なし | 行番号なしで開く（今の動作のまま） |

  - `exec::at_line` の `LineArg::Plus` を `LineArg::Flag(&str)` に一般化し、上の 4 つを追加した。
- Mikan は対象外にした（Q1 の回答）。必要になったら filer.toml の `[line_args]` で書ける。
- [ ] 各エディタの実機で行ジャンプを確認する（人の作業）。
- [x] 引数の書式を設定で変えられるようにする。
  - filer.toml に `[line_args]` を足した。キーは実行ファイル名（小文字・拡張子なし）、値は `"-l {line} {path}"` のような引数テンプレート。組み込みの表より優先する。
  - テンプレートは opener の `%s` / `$@` の位置に入るので、`nvim -O %s` のような他の引数は残る。`{path}` が無い・2 つあるテンプレートは警告を出して捨てる。
  - 引用は `{path}` と同じ語に続く文字までをまとめて囲む（`{path}:{line}` → `"C:\a b\x.txt:123"`）。スペース入りのパスはテスト済み。
- [x] README の Outline 節の対応エディタ一覧を更新する（`[line_args]` の節も追加した）。

## ロードマップ

### 1. ディレクトリ・ファイル操作と UI の拡張

- [ ] 2 分割ペイン（Split View / Dual Pane）
  - [x] 左右 2 ペインの表示とフォーカス移動。
    - キーは `<C-w>`（Q2 の回答）。押すたびに分割 → 反対のペインへ移る。`<C-S-w>` で閉じる。
      コマンドは `split [open|close]` と `pane_focus [left|right]`。
    - 2 つ目のペインは「もう 1 つのタブ」を横に並べたもの（`App::split` は相手のタブ番号だけ持つ）。
      キーを持つペインは常に `App::active` なので、cd・yank・paste・filter などの既存コマンドは
      ペインを知らないまま動く。相手へのコピーは `y` → `<C-w>` → `p` でできる。
    - 親ディレクトリの列と入れ替わりで出る（ペイン・ペイン・プレビューの 3 列のまま）。
    - タブが 1 つだけのときは同じディレクトリで 2 つ目を作る。複数あるときは次のタブを借りる。
      タブの追加・削除・入れ替えでも相手のタブを見失わない（`split_after_remove` / `split_after_swap`)。
      相手のタブを閉じると分割も閉じる。
    - キーを持つ側は枠で囲み、反対側はカーソルを暗くする。暗い側のクリックはまずキーを取る。
    - 受動側のペインも監視・再スキャンの対象（`scan_low` なので前面のディレクトリを待たせない）。
  - [ ] F5 / F6（または `yy` / `p`）で相手のペインへコピー・移動する。
    - 今は `y` → `<C-w>` → `p` の 3 手。相手ペインを宛先にする 1 キーを足すか決める（要確認になりそう）。
  - [ ] 左右ペインの間でファイルをドラッグ＆ドロップできるようにする。
- [x] キーボードとマウスの複数選択を統合する
  - Shift+クリックで範囲選択、Ctrl+クリック（macOS は Cmd）で 1 件ずつ選択を切り替える。
    egui の `Modifiers::command` を使うので OS の差は自動で吸収される。
  - Space / Visual モードとマウスは同じ `Tab::selected`（`BTreeSet<PathBuf>`）を使う。
    `Tab::click` / `ctrl_click` / `shift_click` が入り口で、Shift の起点は `Tab::mouse_range`
    が持つ。起点はプレーンクリック・キー操作（`sync_visual`）・選択解除・cd で落ちる。
  - 修飾キーを押したクリックはダブルクリック扱いにせず、開く動作に化けないようにした。
- [x] コマンドパレット（Command Palette）
  - `Ctrl+Shift+P`（macOS は Cmd+Shift+P）で開く。`palette` コマンドを足し、既定の keymap で
    `<C-S-p>` に割り当てた。`keys::from_egui` が Cmd を ctrl 扱いにするので OS 差は自動で吸収される。
  - 既存の Pick オーバーレイを使い回した（`PickAction::Command`）。項目は `mgr` の全バインドで、
    ラベルは `説明  ·  コマンド文字列` なのでどちらでも引ける。右端に実行キー、選択は fuzzy 検索。
  - 同じコマンドが複数キーにあるときは keymap で最初に出てくるキーの 1 件だけを出す。
    `noop` と unsupported なコマンドは出さない（`app::palette_items`）。
  - 実行前にオーバーレイを閉じるので、`rename` や `shell` のように入力を求めるコマンドもそのまま動く。

### 2. Git 統合

- [ ] Git の状態を一覧に表示する
  - git2 クレートを使う。Modified は黄、Untracked はグレー、Staged は緑のように色やマークで表す。
  - 状態はバックグラウンドで取得し、UI を止めない。
    - 今のワーカー＋ crossbeam 方式に合わせるか、Tokio を入れるかを検討する。

### 3. アーカイブ（圧縮・解凍）

- [ ] ワンキーで圧縮・解凍する
  - 解凍は `e` キーなど（`x` は切り取りで使用中）。
  - 圧縮は `c` や `a` キーでアーカイブのダイアログを出す（Zip / tar.gz など）。
    - `c` / `a` は既存の割り当てと重ならないか確認する。
- [ ] ライブラリ構成
  - 基本は外部依存のない Rust ネイティブのライブラリ（zip / flate2 など）で完結させる。
  - 7-Zip の CLI があれば、.7z や .rar などの特殊な形式はそちらで処理する。

### 4. 組み込みターミナルとバックグラウンドタスク

- [ ] 組み込みターミナル（PTY 連携）
  - アプリ内で PTY を起動する。カレントディレクトリに自動で追従し（cd 連携）、選択ファイルのパスを流し込めるようにする。
- [ ] バックグラウンドタスクの進捗を見せる
  - 大容量ファイルのコピー・移動・圧縮・解凍の進捗（％、速度、残り時間）をステータスバーのプログレスバーに出す。
  - タスクの一時停止・再開（Pause / Resume）と、キューの優先度変更（Priority）。

### 5. プラグインと設定の拡張

- [ ] カスタムアクションを GUI から使えるようにする（Lua プラグイン相当）
  - `keymap.toml` や `yazi.toml` にあるシェルコマンドやカスタムアクションを、右クリックメニューとコマンドパレットの選択肢に動的に出す。

### 6. ネットワークドライブ

- [x] UNC パス（`\\IPアドレス\共有名`）とマウント済みのパスを扱う
  - UNC パスやネットワーク共有をそのまま `PathBuf` として扱い、参照も操作もできるようにする。
  - `util::normalize` を直した。`..` がルート（ドライブ・共有・`/`）より上に行かないようにした。
    直す前は `C:\a\..\..` が `C:\..`、`\\host\share\a\..\..` が `\\host\share\..` になっていた。
    `normalize` は cd・コピー・移動の同一判定（`fs::ops::same_path` / `is_inside`）でも使うので、
    ドライブ側の同じバグも一緒に消えた。
  - Windows の prefix は `\` 表記に揃える（`//host/share/x` → `\\host\share\x`）。表示と比較が揃う。
  - `\\host`（共有名なし）は std が prefix と見なさず `\host` に潰れていた。先頭の 2 本を残し、
    `resolve_against` でも絶対パス扱いにしたので、タブの cwd に繋がれて別の場所へ行くことがなくなる。
    ホストの共有一覧（NetShareEnum）は未実装。
  - README に「Network paths (UNC)」の節を足した。
- [ ] 応答の遅れと切断を安全に扱う
  - 低速な環境や応答待ちで UI が止まらないように、ネットワーク越しの操作とディレクトリ走査を非同期で行う（Tokio を検討）。
  - 接続切れやアクセス拒否でもハングやクラッシュをせず、トーストかエラーダイアログで知らせる。
  - [x] 移動（cd）を UI スレッドでブロックしないようにする。
    - `cd` の `target.is_dir()` を外した。切れた共有では `is_dir` が数十秒返らず、UI スレッドが
      その間止まっていた。今はまず移動して *Loading* を出し、スキャンの結果で判断する。
    - スキャンが失敗したら、そのタブを元のディレクトリへ戻してエラーの toast を出す
      （`Tab::cd_failed` → `CdFallout`）。存在しない・拒否された・打ち間違えたパスも同じ道を通る。
    - キャッシュにある（＝一度一覧できた）ディレクトリへの移動は取り消しの対象にしない。
      再スキャンの失敗は今までどおり `LoadState::Error` で見せる。
    - `cd` プロンプトにファイルのパスを入れたときの「親へ移って選択」も、ブロックする
      `is_dir` ではなく失敗時の 1 回だけのフォールバックで実現した（`PendingCd::fallback`）。
    - 取り消しは 1 回で打ち止め（`pending_cd` は 1 度しか答えない）なので、失敗が続いてもループしない。
  - [ ] `cd` プロンプトのパス補完（Tab キー）を非同期にする。
    - `App::complete_input` が UI スレッドで `read_dir` を呼んでいる。ネットワーク上のディレクトリで
      Tab を押すと止まる。スキャンのワーカー経由にするか、キャッシュだけから補完するかを決める。
  - [x] 起動時とリンク追跡に残るブロックする IO を外す。
    - `main.rs` の開始パス判定から `p.is_dir()` を外した。コマンドラインのパスはそのまま開き、
      `App::start_unproven` で `cd` と同じ `pending_cd`（`fallback: true`）を立てる。最初の
      スキャンが失敗したら、親へ寄せてファイルを選び、それも駄目なら作業ディレクトリへ戻す。
      `filer C:\dir\file.txt` がそのファイルを選んだ状態で開くようになった（前は無視していた）。
    - `App::follow_link` の `std::fs::canonicalize` を消した。リンクの行き先は `Entry::link_to`
      としてスキャンのワーカーが `read_link` で読む（相対リンクはリンクのあるディレクトリ基準、
      ジャンクションの `\\?\` は `util::unverbatim` で外す）。壊れたリンクは `Kind::Link.broken`
      で判定するので、UI スレッドはディスクに触らない。
    - ファイルへのリンクは親ディレクトリの `memo` にも名前を入れる。一覧が後から届いても
      カーソルがそのファイルに乗る。
  - [x] 残りのブロックする IO を外す。
    - `App::create_tab` の `target.is_dir()` を外した。`cd` と同じく、まずそのパスでタブを開き、
      スキャンの結果で決める。失敗したら開いた元のディレクトリへ戻す（`pushed: false` なので
      履歴は触らない）。`tab_create <path>` のパスは `fallback: true` で、ファイルを指していれば
      親へ寄せてそのファイルを選ぶ。キャッシュにあるディレクトリは `switch_tab` が埋めるので、
      まだ *Loading* のときだけ `pending_cd` を立てる。
    - 行き先の決定を `new_tab_target`（自由関数）に切り出してテストを 2 件足した。
    - `spot.rs` はワーカーで動くので対象外。
  - [x] `Watcher::sync` の `watch()` を UI スレッドから外す。
    - `notify` のウォッチャーを専用スレッド（`watch`）へ移した。UI スレッドの `Watcher::sync` は
      「監視したいディレクトリの集合」をチャネルへ送るだけになり、`watch()` / `unwatch()` の
      ハンドル操作はすべてそのスレッドで動く。切れた共有で止まっても UI は動き続ける。
    - `sync` は毎フレーム呼ばれるので、前回送った集合と同じなら何もしない（`Watcher::sent`）。
      送信に失敗したときは `sent` を更新しないので、次のフレームでまた試す。
    - スレッド側は溜まった依頼を捨てて最後の 1 つだけを適用する（newest-wins）。
      差分の計算は純粋関数 `plan()` に切り出してテストした。
    - `Watcher` を drop するとチャネルが閉じ、スレッドが終わってウォッチャーも落ちる。

## 環境・その他

- [ ] MCP サーバーの認証・接続
  - engineering 系（GitHub、Slack、Linear、Notion、Asana、Atlassian、Datadog、PagerDuty）は未認証。claude.ai のコネクタ設定か `/mcp` で認証する。
  - obsidian の MCP サーバーに接続できない（ECONNREFUSED）。Obsidian 側でサーバーが動いているか確認する。
- [x] clippy の never_loop を直す（`src/main.rs` の Confirm の Text 処理）。
  - `for c in text.chars() { ...; break; }` を `if let Some(c) = text.chars().next()` に置き換えた。動作は同じ（先頭の 1 文字だけで答える）。
  - これで clippy がエラーなしで終わるようになった。残りは警告 5 件（`unnecessary_sort_by` / `manual_div_ceil` / `unnecessary_min_or_max` など）。
- [x] clippy の残り 5 件の警告を潰す。
  - `extend(x.drain(..))` → `append(&mut x)`（theme.rs 2 箇所）、`sort_by` → `sort_by_key`（sort.rs）、
    手書きの切り上げ除算 → `div_ceil`、意味のない `.max(0)` の削除（overlay.rs）。
  - テスト側の `== false` も `!` に直した（glob.rs）。`--all-targets` で警告ゼロになった。
