# QA report

Findings from the QA session (`.claude/qa-role.md`). Nothing here is fixed by that
session: it writes tests, and everything else is a proposal for whoever owns the
code.

---

## TESTING.md section 10 — audited against c08b2d2

Every row of section 10 now has a test in `ui::yank_frame` (`src/ui/mod.rs`), and
all eight matched the program. No row was wrong. Two notes came out of writing
them:

| # | 見つけたこと | 根拠 | どちらの間違いか |
| --- | --- | --- | --- |
| 10.2 | `<Space>` はカーソルも 1 行下げるので、押したあとカーソルは「その同じファイル」ではなく次の行にいる。バーが黄色に変わるのは行のほうで、行は動かない | `keymap.toml` の `<Space>` は `run = [ "toggle", "arrow 1" ]` | どちらでもない（行の文言は結果として正しい）。気になるなら「その行」と書くほうが正確 |
| 10.6 | **反映済み (v0.45.4: 10.9 として項目を追加)** 「レジスタが空になる」のは `p` が**成功したから**ではなく、`paste()` がジョブを投げた時点で無条件に空にするから。コピー先が同名で衝突して失敗しても、切り取りのレジスタは戻ってこない | `src/app.rs` の `paste()`：`submit_op` の直後に `self.yank.paths.clear()` | プログラム側の設計判断。実機では「貼り付けに失敗した直後の `p`」が効かないことを一度確かめる価値がある（本文には書かれていない） |

### 「The keys」節の件数が古い  — 反映済み (v0.45.4: `226` に修正)

TESTING.md の冒頭「The keys」は **193 of them across nine layers** と書いているが、
`make-keycheck --check` は **202 / 226 checked** と答える。層の数は 9 で合っている。

- 根拠: `cargo run --example make-keycheck -- --check` → `in sync ... (202 / 226 checked)`、
  および TESTING-KEYS.md の `**202 / 226 checked.**`
- どちらの間違いか: TESTING.md（キーが増えたあと本文の数字が追従していない）
- 番号を動かさない修正なので、本文の `193` を `226` に直すだけで済む。
  この役割はドキュメントの数字も触らない方針なので提案にとどめる。

### TESTING-KEYS.md は同期している

`cargo run --example make-keycheck -- --check` は exit 0。再生成の必要なし。

---

## TESTING.md sections 6 / 18 / 36 — audited against 25c055e

3 節をまとめて `ui::harness::Screen` のテストにした（`src/ui/mod.rs`、テストモジュール
`panes` / `split_panes_frame` / `quick_look_frame` / `max_preview_frame`、25 件）。
どれもペインの割り付けの話で、下準備を `panes` で共有している。

### プログラムと食い違う項目

| # | 見つけたこと | 根拠 | どちらの間違いか |
| --- | --- | --- | --- |
| 18.11 | 「`b` then a letter … Jumps there」が現在の keymap に無い。`b` は**管理のプレフィックス**（`b`+`b` 一覧、`b`+`s` 保存、`b`+`d` 削除、`b`+`D` 全削除）で、`b`+`m` を押すと which-key が出るだけで何も起きない。文字でジャンプするのは `'` のほう | `src/config/defaults/keymap.toml` の該当コメント「`'` jumps and `b` is the prefix everything else hangs off」と `[[mgr.keymap]]` の並び。実測: `b` → pending 1・which 4 件、続けて `m` で cwd 変わらず。`'` → 「Jump to bookmark…」→ `m` で移動 | TESTING.md（キーが移ったあと項目が追従していない）。行の後半（`'` と文字）は正しいので、前半を落とすか `b`+`b` に書き換えるだけで済む |
| 36.17 | 「**Nothing happens**」が確認プロンプトでは成り立たない。`q` は（`y` / `n` 以外のどの文字も）プロンプトを**取り消して閉じる** | `src/app.rs` の `answer_confirm()` が先頭で `std::mem::replace(&mut self.overlay, Overlay::None)` するため、文字が選択肢に無くてもオーバーレイは戻らない。`DeleteForever` は `ch == 'y'` だけを実行するので、**破壊的な動作はしない**。実測: `D` → 確認プロンプト → `q` で `Overlay::None`、`quit` は false | どちらとも言える。行の肝（「It must not quit either」）はプログラムが守っている。ピックリストのほうは行のとおり（テキストフィールドなので `q` は絞り込みに入るだけ） |

36.17 をどう倒すかは人の判断が要る 2 択:

1. **項目の文言を直す** — 「`<Esc>` か、選択肢に無いどの文字でも取り消される。終了はしない」。
   確認プロンプトの取り消しは安全側なので、現状の挙動をそのまま仕様と認める。
2. **`answer_confirm()` を直す** — 選択肢（`c.options`）に無い文字は無視して
   `Overlay::Confirm` を戻す。行の文言どおりになるが、`Conflict` の `_ => Cancel` と
   `<Esc>` の経路を含めて見直すことになる。**プログラムの変更なので QA では触らない。**

推奨は 1。「決断を求めるものは `q` で逃げられない」という約束の実害のある半分
（プロセスが落ちること）はすでに守られていて、残りの半分は文言のほうが後から付いた
説明に見える。

### ハーネス側の観察（テストを書いて分かったこと、提案）

- **`Painted` は `ClippedShape::clip_rect` を捨てている。**`harness::collect` は
  `out.shapes` の `shape` だけを見るので、**ペインの外にはみ出した文字も `texts` に入る**。
  `T` で一覧カラムが幅 0.5px に潰れても行の `a.txt` は `texts` に残っていて、実画面では
  クリップされて見えない。今回は「タイトルが増えないこと」を数で測る形に書き換えて回避した
  （`T` のとき 1 件、`<F3>` のとき 2 件）が、`Painted` が `clip_rect` も持てば
  「実際に見えているか」を直接聞ける。`harness` は `#[cfg(test)]` の中なので QA の範囲内で
  変えられるが、既存の `whole_frame` / `yank_frame` の意味が変わるため、今回は触らずに
  報告にとどめる。
- **`pane_focus`（`<C-w>`）の 1 回目は、分割した直後にキーを新しいペインへ渡す。**
  `open_split()` の doc コメントは "The current tab keeps the keys on the left" と書いて
  いるが、`pane_focus` が続けて `focus_pane(sp.other)` を呼ぶので、1 回目の `<C-w>` の
  あとキーは**右のペイン**にある。TESTING.md 6.1 / 6.2 はどちらの読み方でも成り立つので
  項目の問題ではない。コメントは `open_split` 単体の説明としては正しい。

### 届かなかった項目と、その理由

| # | なぜ届かないか |
| --- | --- |
| 6.15 | ジョブの進捗・速度・`w` からの取り消し。ops ワーカーは実際に走るので 1 バイトのコピーは着地まで確認できるが、**進捗バーが伸びていく様子と速度表示**は、終わるまでに何フレームも描かれる大きなコピーが要る。時間に依存するので `cargo test` には向かない |
| 18.7 | ペイン間のドラッグ。ポインタを押したまま動かして離す操作と、ポインタ脇の "copy" / "move" ラベル。`Screen` はキーとフレームだけで、`egui::Event::PointerMoved` を並べて `app.drag` を組んでも、`draw_drag` が読むのは実際のポインタ位置なので手の代わりにはならない |
| 36.10 | `prepend_keymap` に `<S-t>` を書いて `<C-F5>` で読み直す。`Screen::open` は `Config::load()` を呼ぶので、設定ファイルを置く場所を差し替える手当てが要る（`YAZI_CONFIG_HOME` / `FILER_CONFIG_HOME` を立てて `Config::load()` を通す形）。`<S-t>` が何にも当たらないことは `config::keys` の単体テストが押さえている |
| 36.18 | 同じく再バインド（`on = "Q"`, `run = "quit"`）が要る |

### TESTING-KEYS.md は同期している

`cargo run --example make-keycheck -- --check` は exit 0（`202 / 226 checked`）。
再生成の必要なし。
## TESTING.md section 34 — audited against 25c055e

14 行のうち 13 行は `ui::overlay::help_frame`（`src/ui/overlay.rs`）で自動化した。
キー・距離・停止位置はすべてプログラムと一致していた。残り 1 行と、書いている途中で
出たハーネスの穴を下に挙げる。

| # | 見つけたこと | 根拠 | どちらの間違いか |
| --- | --- | --- | --- |
| 34.14 | **`<C-->` は help パネルを開いている間、何もしない。**「下端に張り付いたままフォントを縮める」という操作自体ができない | `[help]` レイヤーに `scale` の項が無く（`keymap.toml` 835–905 行）、`App::feed_overlay_key` は `PanelLayer::of(&self.overlay)` の層だけを引いて `[mgr]` へ落ちない（`src/app.rs:1882`）。ハーネスで `Overlay::Help` のまま `<C-->` を送ると `app.scale` は 1.0 のまま | どちらとも読める。**人の判断が要る** |

34.14 の読み方は 2 つある。どちらを採るかで直す場所が変わるので、報告だけにとどめた。

1. **TESTING.md の行が間違っている** — `<C-->` は `[mgr]` のキーで、パネルを開いた
   時点でそれは効かなくなる。行を「パネルを閉じて `<C-->`、それから `~` で開き直し、
   `G`」のような手順に書き換える（節番号もチェック id も動かない書き換えで済む）。
2. **プログラムの穴** — 窓全体の見た目を変えるキーは、どのオーバーレイの上でも
   効くのが自然。`[help]`（および `[tasks]` / `[spot]` / `[diff]`）に `scale` を
   足すか、`feed_overlay_key` を層に無いキーだけ `[mgr]` へ落とす形にする。
   後者は挙動が広く変わるので、足すほうが小さい。

**行が言おうとしている再クランプ自体は本物で、自動化した。**
`a_taller_panel_comes_back_to_the_new_bottom` が `Act::Scale` を直接走らせて、
パネルが 27 行から 31 行に伸びたあと `help_scroll` が 130 から 126（= 157 − 31）へ
自分で降りてくることを確かめている。キーが届かないのはキーの問題で、
`overlay::help` の `help_scroll.min(stop)` は期待どおり働いている。

### 提案: ハーネスが `zoom_with_keyboard` を切っていない

`main.rs:171` は起動時に `cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false)` を
呼んで、egui 自身のズーム（`<C-+>` / `<C-->` / `<C-0>`）を止めている。v0.32.0 で
`<C-->` が「ハードリンク＋縮小」を両方走らせた件の対処で、CLAUDE.md にも書いてある。
**`harness::Screen::open` はこれを呼んでいない。**

- 影響: ハーネスでその 3 つの和音を押すと、`Act::Scale` に届かなくても egui が
  `end_pass` で勝手にズームする。実際、34.14 のテストを最初に `<C-->` で書いたときは
  `help_rows` が 27 → 31 に変わり、**通ったように見えた**。`app.scale` を見て初めて
  1.0 のままだと分かった（それが上の 34.14 の発見につながった）。
- 提案するパッチ（`#[cfg(test)]` の中なので QA が書いてもよい範囲だが、共有の
  ハーネスを変えるので判断を残す）:

  ```rust
  pub(crate) fn open(at: impl Into<std::path::PathBuf>) -> Self {
      let ctx = egui::Context::default();
      // What `main` sets on the real context at startup.
      ctx.options_mut(|o| o.zoom_with_keyboard = false);
      let app = App::new(crate::config::Config::load(), at.into(), ctx.clone());
      Self { app, ctx, size: Vec2::new(1280.0, 800.0) }
  }
  ```

- これが入れば、34.14 を本来の形（`G` のあと `<C-->` を押す）で書ける。上の 1. か 2.
  のどちらかが決まったあとで、そのテストを書く。
- 併せて、TESTING.md の 15 節（Window scale）で「egui からキーを取り返した」ことを
  確かめる行も、`app.scale` を読むだけで自動化できるようになる。今は egui が
  代わりにズームしてしまうので、フレームからは区別がつかない。

### 節 34 の他の行について

- キーはすべて `keymap.toml` の `[help]` 層のとおりだった（`j` / `k` / 矢印 = `arrow ±1`、
  `<A-j>` / `<A-k>` と `<C-d>` / `<C-u>` = `arrow ±50%`、`<PageDown>` / `<PageUp>` =
  `arrow ±100%`、`gg` / `G` = `arrow top` / `arrow bot`、`<Esc>` / `q` / `~` / `<F1>` で閉じる）。
- 34.2 の「パネル自身の高さ、ファイル一覧のではない」は 1280x800 で
  **パネル 27 行・一覧 31 行**と実際に違う数字になるので、`<C-d>` が 13 行進むことと
  一覧が 15 行進むことを 1 つのテストで並べて確かめられた。
- 見出しの `(v0.34.0)` は CHANGELOG の 0.34.0 と一致している。
- 34.10 の 3 つのうち、フレームから動かしたのはタスクパネルと spot。比較ビューは
  同じ `Overlay::is_modal` の規則で、そちらは `app::help_keys` と `ui::overlay::diff_frame`
  が見ている。

### TESTING-KEYS.md は同期している（再確認）

`cargo +stable run --example make-keycheck -- --check` は exit 0
（`in sync with src/config/defaults/keymap.toml (202 / 226 checked)`）。再生成の必要なし。

---

## 倒度キーが `[mgr]` 以外のどの層にも無い — 未対応 7 層

34.14 の裏取り（QA セッションの指摘、v0.45.9）から辿ったもの。層は `[mgr]` に
fall through しないので、`<C-+>` / `<C-=>` / `<C-->` / `<C-0>` が `[mgr]` にしか
無い状態では、**何かパネルを開いた瞬間に倒度が効かなくなる。**倒度は窓の性質で
あって、上に何が載っているかとは関係がない。

- v0.46.0 で **`[help]` `[spot]` `[tasks]` `[diff]` の 4 層に足した**（読むための
  パネル）。**残る 4 層は見送り** —— 下の表の理由による。

| 層 | 足すべきか | 理由 |
| --- | --- | --- |
| `[spot]` `[tasks]` `[diff]` | **足した（v0.46.0）** | どれも読むためのパネル。help と同じ性質 |
| `[term]` | 要検討 | キーはシェルのもの。`<C-->` を奪うと通らなくなるものがある |
| `[input]` `[confirm]` `[pick]` | 要検討 | どれも一時的で、判断の途中。倒度を変える場面ではない |

足すと TESTING-KEYS.md が 1 層あたり 4 行増える（4 層で 226 → 242 になった）。
チェック印は人が実機で付けるものなので、**足した分だけ未チェックが増える**ことを
承知のうえで決めること。

---

## TESTING.md sections 9 / 27 / 43 — audited against 97dec91

3 節をまとめて `ui::harness::Screen` のテストにした（`src/ui/mod.rs`、テストモジュール
`preview_panes` / `outline_end_frame` / `preview_arrival_frame` / `csv_table_frame`、14 件）。
どれもプレビューの話なので、下準備を `preview_panes` で共有している。

### ハーネスに足したもの（`#[cfg(test)]` の内側）

プレビューは「ワーカーが答えた」ことで初めて存在するので、状態を手で組んでも
**それは「届いた」ことにならない。**そこで `harness::Screen` に実際のフレームループの
残り半分を足した。`Filer::ui` と同じ順（`drain_channels` → `kick_scans` →
`request_preview` → draw）で 1 周する `turn()` と、届くまで回す `settle()` /
`preview_arrived()` の 3 つ。**プログラムは 1 行も変えていない** ——
`drain_channels` / `kick_scans` / `request_preview` / `previewer` はすべて既に `pub` で、
本物のプレビュースレッドと本物のチャネルをそのまま通している。

- テストが自前で組んだ一覧をスキャンが上書きする危険は、この 3 節では逆に都合がよい:
  fixture は実ファイルなので、`Folder::from_entries` を使わず**実スキャンに任せた**。
- デバウンス（既定 40ms）は `cfg.ui.preview_debounce_ms = 0` にして待たない。
  27.5 は 10 ファイル連続なので、そのままだと待ち時間だけで 400ms 増える。

### プログラムと食い違う項目

| # | 見つけたこと | 根拠 | どちらの間違いか |
| --- | --- | --- | --- |
| 9.1 | `<C-o>` が keymap に無い。アウトラインへキーを渡すのは `<BackTab>`（Shift+Tab） | `src/config/defaults/keymap.toml` に `C-o` の項が 1 つも無い。`run = "toggle_outline"` の `on` は `<BackTab>`。行の後半（`l` on the file）は正しい | TESTING.md（`<C-o>` はどの版の keymap にも見当たらない） |
| 27.4 | `+` で拡大できない。ズームは `<A-i>` / `<A-o>`、等倍が `<A-1>`、フィットが `<A-0>` | 同じファイルのコメントが理由まで書いている ——「JIS 配列では `=` が Shift + マイナスキーで、別のキーとして届くので、`+` / `-` は片方のキーボードでしか動かない」 | TESTING.md（キーは最初からこの形。行が `+` と書いたのは由来不明） |
| 43.11 | **テーブルを出している間はミニマップが 1 本も描かれない。**「ファイルをマップする」以前に、マップ自体が無い | `src/ui/preview.rs` の `Payload::Markdown` 分岐が `st.render_markdown` のとき `markdown(...)` で **early return** し、`split_minimap` / `minimap` に届かない。CSV のテーブルは `Payload::Markdown` 形（`src/preview/csv.rs` の冒頭コメントがそう決めている）。実測（1920px・82 桁・`ui.minimap` は on）: テーブル表示で右端 120px の矩形が **0 個**、`M` でソース表示にすると **6 個** | **どちらとも言える。**下の 2 択 |

### 43.11 をどう倒すか（人の判断が要る）

`src/preview/csv.rs` の冒頭は、ペイロードを `Payload::Markdown` 形にした理由の 1 つとして
**「ミニマップは行の見た目の幅からバンドを作るので、パディングだらけのテーブル行を
`Payload::Text` で渡すと一様なブロックになり、ビューポート枠がファイルの行ではなく
描画行を指してしまう」**と書いている。つまり `source` と `map` を持たせたのは、
**テーブルを出したままファイルをマップできるようにするため**と読める。ところが描画側は
レンダリング済み Markdown をまとめて early return しているので、その `map` は
ソース表示のときしか使われない。

1. **項目の文言を直す** — 「テーブル表示にミニマップは出ない。`M` でソースにすると出て、
   そちらはファイルの行をマップする」。レンダリング済み Markdown に地図を出さないのは
   `ui/preview.rs` が明示している設計判断（描画行とファイルの行が違う）で、CSV も同じ扱いに
   なっているだけ、という立場。
2. **描画を直す** — `Payload::Markdown` のうち **CSV 由来のものだけ**（`doc.toc` が空で
   `map` が非空、など見分けが要る）ミニマップを出す。`map` は既に `source` から作られて
   いるので枠の位置も正しくなるが、`markdown()` の early return を割ることになる。
   **プログラムの変更なので QA では触らない。**

推奨は **1**。Markdown のレンダリング表示に地図を出さない規則は先にあって、CSV は
その規則に素直に従っている。43.11 は csv.rs のコメントから書かれた項目に見える
（コメントは「なぜ `Payload::Markdown` を選んだか」の説明で、「テーブル表示に地図が出る」
とは言っていない）。なお `ui::csv_table_frame::the_minimap_arrives_with_the_text_and_not_with_the_table`
は**現状の挙動を assert している**ので、2 を選ぶならそのテストが落ちる —— 落ちたときに
何が変わったのかが分かる形にしてある。

### 項目の文言として気になったが、間違いではないもの

| # | 見つけたこと |
| --- | --- |
| 27.1 / 27.2 | 27.1 は「cold start」、27.2 は「このセッションで開いていないファイル」だが、v0.12.0 のバグに必要だった条件は**キャッシュに無いこと**だけで、起動が cold かどうかは関係がない。27.2 の「**This is the case that was broken: not cold starts**」がそう書いているので、行としては正しい。自動化できたのは 27.2 の側で、27.1（本当に起動直後のプロセス）だけが残った |
| 43.1 / 43.7 | 「Excel が保存した `.csv`」「Excel が CSV UTF-8 で保存した（BOM 付き）」。テストは同じ中身のファイルを自分で書いているので、**Excel が実際に何を書くか**は確かめていない（行区切り、引用の付け方、BOM の有無）。実機で 1 度見る価値は残っている |
| 43.8 | 「short rows are padded」は数値列だと**左に**寄る。`ragged.csv`（`a,b,c` / `1` / `2,3`）では a 列も b 列も数値列と判定されて右寄せになるので、パディングは空欄の左側に入る。行の言う「padded」は満たしている |
