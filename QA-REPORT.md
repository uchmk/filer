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

## TESTING.md sections 33 / 38 / 42 — audited against 97dec91

3 節を `ui::harness::Screen` のテストにした（19 件）。テストモジュールは

| 節 | モジュール | ファイル |
| --- | --- | --- |
| 33 | `ui::config_warning_frame` | `src/ui/mod.rs` |
| 38 | `ui::preview::focus_rule_frame` | `src/ui/preview.rs` |
| 42 | `ui::preview::minimap_hover_frame` | `src/ui/preview.rs` |

38 と 42 は同じ「プレビューの脇に描かれるもの」なので、フィクスチャを
`ui::preview::chrome` に 1 つだけ置いて両方から使っている。

### 届く範囲の見積もり — 依頼時の見立てとの差

依頼には「38 節は 6 項目中 5 項目がターミナルを要する」「ConPTY は `#[cfg(windows)]`
なので Linux では 1 行もコンパイルされない」とあったが、**前提が違っていた。**

| 見立て | 実際 |
| --- | --- |
| ConPTY は `#[cfg(windows)]` なので Linux では出番が無い | **`src/terminal.rs` に `#[cfg(windows)]` は 1 つも無い。**PTY は `alacritty_terminal` の `tty` が持っていて、Unix では本物の PTY、Windows では ConPTY を開く（`src/terminal.rs` の冒頭コメントがそう書いている）。`#[cfg(target_os = "windows")]` は `tty::Options` の `escape_args` フィールド 1 か所だけ |
| だから 38 節はほぼ届かない | **Linux のテストから `<C-t>` でシェルが上がった（26 ms）。**`app.term` が `Some` になり、ペインの上端に accent の罫線が引かれる。38.1 / 38.2 / 38.4 / 38.5 / 38.6 / 38.9 が届いた |
| 届くのは 38.3 あたりだけ | 38.3 も届いたが、それだけではなかった |
| 33 節は自分の `keymap.toml` を置いた状態が要る | **要らない。**`Keymap::load(&[text])` が `keymap.toml` の**本文**を受け取るので、重複 binding の警告はディスクに触らずに作れる。あとは `Config { warnings, ..Config::load() }` を `App::new` に渡すだけ（`app::config_warnings` が既に同じ形を使っている） |
| 42 節はポインタのイベントが要る | 要る。そして**通った。**`Screen::feed` に `egui::Event::PointerMoved` / `PointerButton` を渡せば `rect_contains_pointer` と `dragged_id` の両方が動く |

**届かなかったのは 3 節で 6 項目だけ**だった（下の表）。

### ハーネスに足したもの（すべて `#[cfg(test)]` の内側）

3 節はどれも「どこに何色で描かれるか」なので、依頼の見立てどおり `Painted::filled` で
足りるはずだった。**足りなかった。**罫線もトーストの枠も**塗りではなく stroke** で、
`collect` は `RectShape` の `fill` しか拾っていなかった。文字色も同じで、警告の黄色は
galley の色にあり、`texts: Vec<String>` は色を落としている。

- `Painted::strokes: Vec<(Rect, Color32)>` と `Painted::stroked(color)` を足した。
  `Shape::Rect` の `stroke`（幅 > 0 のとき）と `Shape::LineSegment` を拾う。
  `Painter::vline` / `line_segment` はどちらも `LineSegment` になる。
- `Painted::inked: Vec<(String, Color32)>` と `Painted::ink(needle)` を足した。
  `TextShape` の `override_text_color`、無ければ `fallback_color`。`Painter::text` は
  渡された色で galley を組んでから同じ色を fallback に置くので、単色の文字列は
  これがその色になる。複数色の `LayoutJob`（プレビュー行、ホバーカード）は fallback
  しか答えないので、そこは塗りか stroke で見ている。
- `Screen::with_config(cfg, at)` を足した。設定の警告がトーストになるのは `App::new`
  なので、アプリを作る**前に**設定を持っている必要がある。
- `Screen::wait(secs)` と `RawInput::time` を足した。ホバーカードは
  `tooltip_delay` を egui の時計で測るので、フレームを 30 枚描く代わりに時計を進める。
  `feed` は 1 枚あたり 1/60 秒進める（egui が `time: None` のとき自分で足す値と同じ）。

既存の `texts` / `rects` / `says` / `rects_in` / `filled` は変えていない。

### 届かなかった項目

| # | なぜ届かないか |
| --- | --- |
| 33.4 | インストールされていないプログラムをオープナーに指定して**実際に起動させる**必要がある（25.8 と同じ）。色の対比自体は `app::config_warnings::warning_and_error_are_different_colours` が見ている |
| 33.6 | 明るい背景のテーマで黄色が読めるか。目の話 |
| 33.15 〜 33.18 | どれも**ファイルがディスク上にある**ことが前提。`FILER_CONFIG_HOME` で読み先を差し替えられるが、**`std::env::set_var` はプロセス全体に効く**ので、`Config::load()` を呼ぶ他のテストと並列に走ると互いの設定を壊す。1 件のために全テストを直列化する価値はないと判断した。33.16 / 33.18 の「行の文言」は `ui::overlay::help_config_rows` が `config_rows` を直接呼んで見ている |
| 38.7 の後半 | `theme.toml` を書いて `<C-F5>` で読み直す部分。上と同じ理由。両方の罫線が 1 つの `tab_active` を読んでいることは、`Theme` をメモリ上で変えて確かめた |
| 42.6 の「shrinks to the gutter」／42.7 の「no frame hitch」 | カードの幅は取れるので「空行のカードは長い行のカードより狭い」は見ているが、「gutter の幅」に当たる数はコードに無い。フレーム時間はハーネスが測るものではない |
| 38.8 の「visibly different」 | 色が 2 つとも取れて `assert_ne!` はできるが、**見分けがつくか**は目の話 |

### ターミナルを起動するテストが 2 件ある — マージ前に読んでほしい

`focus_rule_frame` の 2 件（`the_terminals_rule_follows_the_keys_and_the_mouse` と
`with_both_panes_open_exactly_one_rule_is_accent`）は `<C-t>` で**本物のシェルを起動する。**
ペインは `app.term` が `Some` でないとレイアウトされないので、これを避ける道が無い。

- Linux では 26 ms で上がり、`Terminal` の `Drop` が `Msg::Shutdown` を送るので残らない。
  415 件すべて並列で 1.67 秒。
- **Windows CI では初回になる。**`tty::new` が ConPTY を開き、既定の
  `powershell` が起動する。GitHub の windows ランナーでは動くはずだが、
  クラウドセッションからは確かめられない。
- もし赤くなったら、この 2 件を落とせば 38 節は 38.3 と 38.8 だけに戻る。
  **落とす前に他の 17 件が緑であることを見てほしい** —— どちらもシェルに 1 文字も
  送らないので、落ちるとすれば PTY が開かないところで、他には波及しない。

### 33.9 は startup では起こらない — 項目の間違い

| # | 見つけたこと | 根拠 | どちらの間違いか |
| --- | --- | --- | --- |
| 33.9 | 「**3 つの config ファイルを同時に壊す** → 箱が最大 5 つ下向きに積まれる」は、**起動時には起こらない。**警告が何件あっても `App::new` が出すトーストは **1 件**で、残りは `(+N more, see ~)` に畳まれる。3 つ壊しても箱は 1 つ | `src/app.rs` の `App::new`：`if let Some(w) = app.cfg.warnings.first()` …… `format!(" (+{more} more, see ...)")`。実測でも 3 件の警告に対し `stroked(theme.warning)` は 1 件 | TESTING.md。畳む処理は **v0.33.0**（`git log -S "more, see"`）で、33.7 〜 33.10 が書かれた **v0.33.11 より前**。箱が積まれるのは `draw_toasts` の一般の振る舞い（5 件まで）だが、**config の警告からはその状態に入れない** |

直すなら、33.9 は「config を壊して」ではなく「トーストが 5 件以上出る操作を続けて」
に書き換えるか、`draw_toasts` 側の項目として別の節へ移すことになる。**節番号が動くので
報告にとどめる。**

### 42.1 の「about 0.4 s」は 0.5 s

| # | 見つけたこと | 根拠 | どちらの間違いか |
| --- | --- | --- | --- |
| 42.1 | 「After about 0.4 s」。実際の待ちは egui の `Style::interaction.tooltip_delay` で、**既定は 0.5 s** | `src/ui/preview.rs` の `minimap_hover`：`let delay = f64::from(ui.style().interaction.tooltip_delay);`、egui 0.36.2 の `style.rs:1487` が `tooltip_delay: 0.5` | TESTING.md（数字が 0.1 秒ずれている）。テスト側は「0.2 秒ではまだ無い / 遅延後には有る」という形にしてあるので、テーマで待ちを変えても落ちない |

### 38.6 はクリックの 1 フレーム後に色が変わる

項目の間違いではない（人には見えない）が、テストを書くときに引っかかるので書いておく。
`ui::term::draw` は先頭で `let focused = app.term_focus;` を読んで罫線を引き、そのあとで
`resp.clicked()` を見て `app.term_focus = true` にする。だからクリックが載ったフレームの
罫線はまだ灰色で、accent になるのは次のフレーム。`ctx.request_repaint` が走るので実機では
見えない。

### 「The keys」節の件数がまた古い

`make-keycheck --check` は exit 0（`in sync ... (242 / 242 checked)`）だが、TESTING.md の
「The keys」節は **226 of them across nine layers** と書いている。v0.46.0 で 4 層に倒度キーを
足して 226 → 242 になったときに本文が追従していない（前回 193 → 226 で直したのと同じ箇所）。

- どちらの間違いか: TESTING.md
- 番号を動かさない修正だが、この役割は本文の数字を触らない方針なので提案にとどめる。
## TESTING.md sections 12 / 13 / 24 — audited against 97dec91

3 節をまとめて `ui::harness::Screen` のテストにした。どれもファイル一覧まわりで、
下準備は既存の `panes::listing` を使い回している。

| 節 | テストモジュール | 自動化した項目 |
| --- | --- | --- |
| 12 | `ui::undo_frame`（`src/ui/mod.rs`、3 件） | 12.6 / 12.7 |
| 13（一覧） | `ui::link_rows`（`src/ui/mod.rs`、6 件） | 13.1 / 13.2 / 13.3 / 13.4 / 13.5 / 13.6 |
| 13（spot） | `ui::overlay::spot_link_section`（`src/ui/overlay.rs`、5 件） | 13.10 / 13.11 / 13.12 / 13.13 / 13.15 |
| 24 | `ui::awkward_names`（`src/ui/mod.rs`、2 件） | 24.1（一部）/ 24.3（一部） |
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
## TESTING.md sections 5 / 11 / 21 — audited against 97dec91

3 節を `ui::harness::Screen` のテストにした（25 件）。どれもオーバーレイの話なので、
下準備は `ui::overlay::overlays` に置いて 3 つのモジュールで共有している。

| 節 | テストモジュール | 置き場所 | 自動化した項目 |
| --- | --- | --- | --- |
| 5 | `ui::overlay::compare_frame` | `src/ui/overlay.rs` | 5.1〜5.8、5.10（**5.9 を除く全部**） |
| 11 | `ui::overlay::bulk_frame` | `src/ui/overlay.rs` | 11.1〜11.11（11.9b 含む、**全部**） |
| 21 | `ui::preview::archive_frame` | `src/ui/preview.rs` | 21.1、21.6、21.12 の 3 件のみ |

### プログラムと食い違う項目

| # | 見つけたこと | 根拠 | どちらの間違いか |
| --- | --- | --- | --- |
| 12.8 | 「ファイルを新規作成すると redo が消える」は**成り立たない。**`a`（create）は undo ステップを積まないので、リネーム → `u` → `a` のあとでも `U` はリネームをやり直す（実測: `after create: undo=0 redo=1`、`U` で `Renamed to two.txt`） | `src/app.rs` の `do_create()` は `undos.land(..)` を呼ばない。`Land::Fresh` を積むのは trash / move / rename / bulk rename の 4 つだけ（`grep -n "Land::Fresh" src/app.rs`） | **どちらとも言える。**「新しい操作で履歴が分岐する」という規則そのものは正しく、例に挙がっている操作が規則の対象外。倒し方は下の 2 択 |
| 13.10 | 見出しに `(v0.46.0)` と書いてあるが、13.13 の「hardlink がアプリで見える唯一の場所」を含む Link 節は **v0.46.0 で入っている**。CHANGELOG と一致しているので**版は正しい**（作業指示にあった v0.47.0 は Git 節＝46 節のほう） | CHANGELOG.md の v0.46.0 に Link 節、v0.47.0 に Git 節 | どちらの間違いでもない（記録のため） |
| 本文 | 冒頭「The keys」節が **226 of them** と書いているが、`make-keycheck --check` は **242** と答える（v0.46.0 で 4 層 × 4 キー足したぶん） | `cargo +stable run --example make-keycheck -- --check` → `in sync with src/config/defaults/keymap.toml (242 / 242 checked)` | TESTING.md（前回 `193` → `226` に直したのと同じ箇所が、また置いていかれている） |

12.8 の倒し方（人の判断が要る）:

1. **項目の文言を直す（推奨）** — 例をリネームに替える。「リネームして `u`、そのあと
   **別のファイルをリネーム**、それから `U`」。規則は変えずに、規則が当てはまる操作を
   例に出すだけで済む。`ui::undo_frame::a_fresh_action_forks_history` がその形で
   すでに通っている。
2. **`do_create()` に `undos.redo.clear()` を足す** — 「何か操作をしたら分岐」を
   文字どおりにする。ただし `a` は取り消せない操作なので、`u` で戻せないものが
   redo を捨てるのは筋が通らない。**プログラムの変更なので QA では触らない。**

### 届かなかった項目と、その理由

| # | なぜ届かないか |
| --- | --- |
| 12.1–12.5, 12.9–12.12 | `d` はゴミ箱送りで、**ops ワーカーのジョブ**として走り、戻すほうは `restore::found()` がゴミ箱を読む。ハーネスはワーカーを回さず、`App::drain_channels` も呼ばないので、ジョブの結果が届くことがない。12.3 は「タスクパネルに `Restore` 行が出る」で、その行はジョブが動いていることが前提 |
| 12.10–12.12 | 加えて、ファイルをロックする別プロセスと、ゴミ箱を切ったドライブが要る |
| 13.7 | junction を作るには `mklink /J`（Windows）。アプリ側から見れば junction も symlink も同じ `Kind::Link` なので、**この項目が確かめているのはスキャンの側**で、実体を作らないと意味がない |
| 13.8, 13.9 | `y` → `-` / `_` はリンクを**作る**。Windows では開発者モードか昇格が要る（項目自身がそう書いている）ので、CI の Windows ランナーで通す当てがない |
| 13.14 | `Also at` は Windows 限定（Unix の `hard_links()` は本数だけ数えて名前は返さない）。**`#[cfg(windows)]` で書けるが、Linux から検証できないコードを CI に置くことになるので書かなかった** |
| 13.16 | 書き込みでファイルを掴んでいる別プログラムが要る |
| 24.2 | 名前の中略は **egui の galley の中**で起きていて、`Galley::text()` が返すのは「並べた文字列」＝元の名前そのもの。ハーネスの `Painted.texts` はそれを集めているので、**中略された行と無傷の行が区別できない**（実測: 107 文字の名前が丸ごと 1 要素として出てくる）。下の提案を参照 |
| 24.4 | `<A-t>` はターミナルペイン送り。ConPTY は `#[cfg(windows)]` で Linux では 1 行もコンパイルされない。引用そのものは `src/exec.rs` の `quote()` の話で、フレームからは届かない |
| 24.5 | 12 節と同じ理由（ゴミ箱） |
| 13.4 の「in red」 | `Painted` は塗った矩形の色と文字列を持つが、**文字の色は持たない。**メッセージが出ることは assert したが、赤いことは目の仕事 |

### 提案: ハーネスが galley の「見えている文字」も拾えるようにする

24.2 のような**中略・省略の項目が現状すべて届かない。**同じ理由で、タブの見出し
（` 1 filer-explo…e-8697 `）や spot パネルの値が縮んでいるかどうかも分からない。
なお spot パネルは**自前で真ん中を `…` にする**ので、そちらは今のままでも見える
（`spot_link_section::row` がそれを前提に、値を丸ごと読む形で書いてある）。

`harness::collect` は `#[cfg(test)]` の内側なので QA が触れる場所ではあるが、
**`Painted` は 13 のテストモジュールが共有している型**で、並行する QA セッションも
同じファイルの末尾に足している。ここで足すと衝突の当たりが増えるだけなので、
提案にとどめる。形はこれで足りる:

```rust
// harness::Painted に 1 フィールド、collect に 1 行。
/// 実際に画面に並んだ字だけ。`texts` は並べるよう渡された文字列で、
/// egui が 1 行に収めるために落とした分は入っていない。
pub glyphs: Vec<String>,

// collect() の Shape::Text の腕:
Shape::Text(t) => {
    into.texts.push(t.galley.text().to_owned());
    into.glyphs.push(t.galley.rows.iter().flat_map(|r| r.glyphs.iter()).map(|g| g.chr).collect());
}
```

これが入れば 24.2 は次の形で書ける（末尾に `…`、拡張子が残らないことまで見える）:

```rust
// 24.2: 長い名前は縮む。今の `overflow_character` は行の**末尾**を `…` にする
// ので、**項目の言う「真ん中で中略、拡張子は読める」にはなっていない**。
// どちらを直すかは人の判断（項目の文言か、`name_job` の組み立てか）。
let drawn = f.glyphs.iter().find(|g| g.starts_with("a-very-long")).unwrap();
assert!(drawn.ends_with('…'));
assert!(!drawn.ends_with(".txt"));
```

**24.2 は文言と実装が食い違っている可能性が高い。**`src/ui/list.rs` の `name_job()` は
`wrap.max_rows = 1` と `overflow_character = Some('…')` を置いているだけで、これは
egui の**末尾切り**で、真ん中を抜く処理はどこにも無い。真ん中で抜く関数は
`util::ellipsize_middle()` として**すでにある**（`ui/mod.rs:211` でタブの見出しに
使っていて、そのユニットテストは `ends_with(".txt")` まで assert している）ので、
項目の言う挙動にするなら手は足りている。どちらを直すかは人の判断。実機で 1 度
見てもらうのが早い（ハーネスでは前述のとおり見えない）。
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

### ワーカーを回すテストは「親カラム」に足を取られる（PR #25 のレビューで判明）

`turn()` / `settle()` を入れた初回に踏んだ罠。次に同じ形のテストを書くときのために残す。
**プログラムの問題ではなく、テストの側の問題。**

- **スキャン中のペインは、プレビューの待ちと同じ `…` を描く。**フレーム全体で `…` を
  数えていたので、親（`env::temp_dir()`）のスキャンが 1 フレーム目に間に合わないと
  2 個目が出て条件が崩れた。空の `/tmp` では通り、**4000 件入った `/tmp` では落ちる。**
  CI が緑だったのは Windows ランナーの一時ディレクトリが空だったから。
- **領域で絞ることはできない。**`Painted` は矩形の位置と文字列の中身しか持たないので、
  「プレビュー枠の中の `…`」は `rects_in` では聞けない。
- 倒したやり方: **一覧するディレクトリを `test_dir(label).join("room")` にして、親を
  テスト側が持つ。**親が 1 エントリなら 1 フレーム目に間に合い、かつ**他のテストが
  書き込まないので watcher が dirty を立てて `Loading` に戻すこともない**
  （一時ディレクトリを親にしていると、並列で走る他のテストが `test_dir` を作るたびに
  それが起きうる）。
- もう 1 つ: 一覧が出るまでループを回すと、**速いマシンではその間にプレビューも届く。**
  「要求は出たが何も返っていない」は待って作るものではなく、`preview.cache.clear()` と
  `key = None` で**言って作る**もの（`preview_panes::forget`）。
| 5.9 | 「Two directories → Refused with a reason」は**もう成り立たない**。v0.43.0（45 節）でフォルダ 2 つはツリー比較になった。いま拒否されるのは**片方だけがフォルダ**のときで、文言は `compare two files, or two folders — not one of each` | `src/app.rs` の `compare_pair()`: 拒否条件は `a.1.is_dir_like() != b.1.is_dir_like()` だけ。同じ内容が 45.9 にある | TESTING.md（45 節が入ったときに 5.9 が取り残された）。**5.9 の言う拒否はもう起きない**し、いま起きる拒否は 45.9 が見ている。落とすのが素直だが番号を動かす話になるので提案にとどめる |
| 21.6 | 「A toast says it was **skipped**」は、選択にアーカイブが 1 つも無いときの文言ではない。テキストファイルだけを対象に `e` を押すと出るのは `The file under the cursor is not an archive filer can read (zip, tar, tar.gz, tgz, 7z)` | `src/app.rs` の `do_extract()`。`Skipping N non-archive item(s)` は**アーカイブと非アーカイブが混ざっている**ときの別のトーストで、そちらはジョブも走る | TESTING.md（2 つのトーストが 1 項目に混ざっている）。「アーカイブでないと言う」に直すか、混在の場合を別項目に分けるかの 2 択 |

### プログラム側の指摘（QA では直さない）

#### `InputOverlay::initial_selection` はどこからも読まれていない

フィールドのコメントは「最初にフォーカスを得たときにキャレットを置く場所」と書いてあり、
`start_bulk_rename()` は `Some((0, "{name}{ext}".chars().count()))`（= 全選択）を入れている。
**が、`src/ui/` のどこも読んでいない。**`grep -rn initial_selection src/` の結果は
代入 5 か所とフィールド宣言だけで、`TextEditState` に書き戻す経路が無い。

- 実測: `R` → `holiday-{n:2}{ext}` と打つと、フィールドは
  `{name}{ext}holiday-{n:2}{ext}` になる（置き換わらない）。
- 影響する項目: **11.2**（「Type `holiday-{n:2}{ext}`」が、そのままでは意図した規則にならない）。
  11.6 / 11.7 / 11.8 / 11.9b / 11.10 / 11.11 も規則を打つので同じ。
  ほかに `rename`（`src/app.rs:3203`、拡張子を除いた部分だけ選ぶ指定）と
  補完後のキャレット末尾寄せ（`src/app.rs:3964`）も効いていない。
- テストは `<C-a>` で全選択してから打つ形で書いた（`bulk_frame::rule()` のコメントに理由あり）。
  **直ったらその 1 行を消せばよい。**直った後に書きたいテストはこれ:

  ```rust
  // The prompt opens with its rule selected, so the first keystroke replaces it.
  selecting(&mut s, &dir, &photos);
  let f = { s.typed("holiday-{n:2}{ext}"); s.draw() };
  assert_eq!(rows(&f)[0], "IMG_0001.jpg  →  holiday-01.jpg");
  ```

  `overlay::input` で `!ov.focused` のときに `ov.initial_selection` を
  `TextEditState::load` → `cursor.set_char_range` → `store` する 5 行で足りる
  （`right_click_paste` が同じことをやっているので、書き方はそこにある）。

#### バルクリネームのプレビューは 1 フレーム遅れる

`ui::draw` は `overlay::bulk` を `overlay::input` より先に呼ぶので、パネルは
**その時点の `ov.text`**（= 前のフレームの内容）から作られる。`TextEdit` が文字を
受け取るのはそのあと。

- 60fps では誰も気付かないので**バグとして直す必要はない**が、フレーム単位で見るテストは
  「打った次のフレーム」を読む必要がある（`bulk_frame::rule()` がそうしている）。
- 記録として残す: 将来 `bulk` を `input` の後ろに動かすと、このテストの `s.draw()` が
  1 回余計になるだけで落ちはしない。

### 21 節の届かない 9 件 — なぜ届かないか

ハーネスはワーカーを回さない（`ui::harness` の設計。スキャンが途中で届くとテストが
組んだ状態を上書きするので、意図的にそうなっている）。`e` / `E` はどちらも
`submit_op` / `submit_op_to` でジョブキューに渡すので、**展開も圧縮もフレームの中では
起きない。**

| # | 要るもの | 理由 |
| --- | --- | --- |
| 21.2 | 展開ジョブ | `do_extract()` → `submit_op(OpKind::Extract, …)`。フォルダができるのはワーカーの中 |
| 21.3 | 展開ジョブ 2 回 | 2 回目の名前を決めるのは `extract_dir()` だが、衝突を見るのは展開そのもの |
| 21.4 | 圧縮ジョブ | プロンプトの既定名（`to-pack.zip`）までは 21.12 のテストが見ている。「Packed, and the result opens」はジョブ完了後の `open` |
| 21.5 | 圧縮ジョブ | 形式が tar.gz になったことは**できた書庫を読まないと分からない** |
| 21.7 | 圧縮ジョブ | 同上（7z） |
| 21.8 | 展開ジョブ | 同上 |
| 21.9 | **7-Zip か Explorer** | 他のプログラムが開けることを言っている項目なので、原理的にこのリポジトリのテストの外 |
| 21.10 | 圧縮ジョブ＋タスクパネル | 数えているのは `on_entry` コールバックの呼ばれ方で、ワーカーの中 |
| 21.11 | 圧縮ジョブ 2 回 | 圧縮率の比較。書庫を 2 つ作らないと始まらない |

- **`fs::archive` 自体の往復は `fs::archive::tests::a_tree_survives_being_packed_and_unpacked`
  が 4 形式すべてで見ている。**届かないのは「キーを押すとジョブが走り、タスクパネルが数え、
  結果が開く」という配線のほうで、そこはワーカーを回すハーネス（`ops::Pool` を回して
  `OpEvent` を待つもの）が要る。**要否は人の判断**なので提案にとどめる:
  - 案 A（推奨）: 21 節はこのまま実機に残す。ジョブは進捗・速度・キャンセルまで含めて
    「見て確かめる」ものなので、6.15 と同じ扱いにする。
  - 案 B: `ops::Pool` を回してイベントを待つテスト用ヘルパを足す。**プログラム側の変更**
    （公開範囲か、完了を待つ手段）が要る。

### テスト用に足した 1 か所 — `preview::for_tests`

21.1 は「本物の zip → 本物のプレビュー → 本物の描画」で見たかったが、
`preview::render()` は `preview` モジュール内の private で、`src/ui/` からは呼べない。
**`src/preview/mod.rs` の末尾に `#[cfg(test)] pub(crate) mod for_tests` を足し、
その中から `render()` を呼ぶ形にした**（`Request` の組み立てもその中）。

- `#[cfg(test)]` の内側なので release ビルドには 1 行も入らず、プログラムの挙動は変わらない。
  役割の規則（「diff の追加行がすべて `#[cfg(test)]` の内側」）も満たしている。
- とはいえ**テスト用の入口を本番モジュールの木に足した**ことは事実なので、
  気に入らなければ `render` を `pub(crate)` にする（= プログラムの変更）か、
  21.1 を `src/preview/mod.rs` の中のユニットテストに落とすかの 2 択。判断は人に委ねる。

### TESTING.md の「The keys」の件数が**また**古い

冒頭は **226 of them across nine layers** だが、`--check` は **242** と答える
（v0.46.0 が `[help]` `[spot]` `[tasks]` `[diff]` の 4 層に倒度キー 4 つずつを足した分）。
層の数 9 は合っている。

- 根拠: `cargo +stable run --example make-keycheck -- --check` →
  `in sync with src/config/defaults/keymap.toml (242 / 242 checked)`（exit 0）
- どちらの間違いか: TESTING.md。`226` → `242` の 1 語だけで済む。
  v0.45.1 で `193` が残ったのと同じ形で、**キーを増やすたびに再発している。**
  `make-keycheck` が TESTING.md のこの数字も書き換える（あるいは `--check` で照合する）
  ようにすれば、次からは CI が捕まえる。

### TESTING-KEYS.md は同期している

`cargo +stable run --example make-keycheck -- --check` は exit 0
（`in sync with src/config/defaults/keymap.toml (242 / 242 checked)`）。再生成の必要なし。
（`242 / 242 checked`）。再生成の必要なし。

### 番号の並び

1..46 に欠番・重複は無く、各 id は `<節>.<n>` になっている。ただし
**46 節（v0.47.0、spot パネルの Git セクション）が `## Known gaps in this checklist` の
後ろに置かれている。**「A new section goes wherever it reads best」に従えば 41 節の隣だが、
移すと 42 以降が全部繰り上がるので、末尾に足したのは本文の方針どおり。
ただ「Known gaps」は**文書のまとめ**なので、節はその手前に来るほうが読める。
番号を動かさずに直せる（46 節のブロックを Known gaps の直前へ移すだけ）ので、
そこだけ人の判断で。

---

## TESTING.md section 33 — 自動化（07c0268 に対して、Job 3）

節の 18 行のうち、自動化されているのは 33.1 / 33.2 / 33.3 / 33.5（一部） / 33.7 / 33.8 /
33.10 の 7 行という前提で入ったが、**実際には 33.11〜33.14 と 33.16 / 33.18 にも既存の
ユニットテストがあり、節の注記がその一部しか挙げていなかった。**この run で足したのは
33.4 / 33.9（期待の側だけ） / 33.12 / 33.14 / 33.17 / 33.18（行の側）の 6 件。
`cargo test` は 465 → 471 件（別セッションの 1 件を含めてバイナリ全体で 472 件）、緑。

| 行 | どこで見ているか | 何を見ているか |
| --- | --- | --- |
| 33.4 | `ui::config_warning_frame::a_failure_beside_the_warning_is_the_other_colour` | 本物の失敗と設定警告を**同じフレームに並べ**、文字と枠の色が `progress_error` と `warning` に 1 つずつ分かれること |
| 33.9 | `ui::config_warning_frame::the_boxes_stack_downward_each_its_own_size` | **期待の側だけ**。箱は最大 5 つ、下へ積まれ、重ならず、各々が自分の文字数の高さ。**手順の側は下の「33.9 の手順は期待を作れない」** |
| 33.12 | `config::files::two_misplaced_sections_both_report_the_files_cost` | 1 つの `yazi.toml` に `[term]` と `[[preview]]` がある場合、2 行出て**両方が「ファイルごと読まれなかった」と言う** |
| 33.14 | `config::files::a_yazi_section_in_filer_toml_is_told_where_to_go` | `[opener]` が `filer.toml` にあるときの文面が `belongs in yazi.toml and was ignored` |
| 33.17 | `ui::overlay::help_config_rows::a_file_that_has_since_been_read_loses_its_marker` | 読み込み済みになった行から印が**外れる**（行が 2 つにならず、`raw` が空になる） |
| 33.18 | `ui::overlay::help_config_rows::a_rebound_reload_key_is_the_one_the_unread_row_names` | 行の文面そのものが `<F9>` を名乗り、`<C-F5>` を含まないこと（既存テストは `key_for` までだった） |

### 33.9 の手順は、その期待を作れない

| # | 見つけたこと | 根拠 | どちらの間違いか |
| --- | --- | --- | --- |
| 33.9 | 「設定ファイルを 3 つ壊す」と箱は**1 つ**しか出ない。5 つ積まれることはない | `src/app.rs:1120-1124`：`App::new` は `cfg.warnings.first()` だけを toast にし、残りは `(+N more, see \`~\`)` という数字にまとめる（= 33.2 がまさにその行） | TESTING.md。`draw_toasts` の 5 枠ループ（`src/ui/mod.rs:925` の `.rev().take(5)`）を**コードから読んで書いた行**で、使って書いた行ではない |

- `(+N more)` へのまとめは **v0.20.0**（`e1897b9`）、33.9 が書かれたのは **v0.33.11**
  （`637efdc`）。**書かれた日から手順と期待が食い違っている。**
- 期待の側（5 枠・下へ・重ならない・各々の高さ）はプログラムの本物の性質なので、
  上のテストが見ている。ただし**設定警告では再現できない**ので、テストは
  `app.warn` を 6 回呼ぶ形にしてある（6 回目が落ちることで上限を見る）。
- 文言の直し方の案（**節番号・id は動かさないので、Do 欄の差し替えだけ**）:
  - 案 A（推奨）: 33.9 の Do を「**失敗するコマンドを立て続けに 3 つ以上走らせる**
    （例: 存在しないプログラムを指すオープナーで `o` を 3 回）」に変える。
    期待はそのまま使える。
  - 案 B: 33.9 を「3 つ壊しても**箱は 1 つで、`~` に 3 行ある**」に変え、
    箱の積み方は 33.2 の隣に別の行として書く。こちらは id が 1 つ増える。

### 33.4 は「色が違うこと」までは機械が見られる。「黄色に見えること」は見られない

依頼の言うとおり、この 2 つは別の主張なので分けて書く。

- **見られる**: ハーネスは galley の色（`Painted::inked`）と枠の色（`Painted::stroked`）を
  そのまま記録している。だから「失敗の文字と枠は `theme.progress_error`、設定警告のそれは
  `theme.warning`、かつ 2 つは同じ値ではない」は**厳密に言える**。33.4 の中身は
  「赤のままか」なので、これで行の主張は満ちている。**片方だけを見るテストにはしていない**
  ——警告だけを見るテストは、全部が黄色くなっても通る。対にして初めて意味がある。
- **見られない**: `#e8c87a` という色が人に「注意」と読めるか。33.6（明るい背景でも
  読めるか）は**恒久的に実機の側**で、ハーネスの出来とは無関係。
  なお `warning` は `ThemeToml` に無く（`src/config/theme.rs:308` は `Theme` 側だけ）、
  33.6 の「it is a fixed default, not yet themeable」は**今も正しい**。
- **手順は置き換えた**。33.4 の Do は「インストールされていないプログラムを指す
  オープナー」だが、その失敗は**同期的に起きない**: `exec::shell` はシェルの起動には
  成功し、「そんなプログラムは無い」はプロセスの終了として後から届く（`launch` が
  `self.launches` に積んで待ち続けているのがそれ）。だから代わりに `follow` を
  リンクでない行で走らせて `error(...)` を起こしている。**色は失敗の種類ではなく
  `Level` が決めるので、行の主張には届いている**が、手順が違うことは明記しておく。

### 33.11〜33.15 は「フレームのテスト」より「ローダーのユニットテスト」が正しい — ただし継ぎ目が無い

依頼の問いへの答え: **ユニットテストを選んだ。**理由は 2 つ。

1. これらの行が問うているのは**文面そのもの**（`belongs in filer.toml, and nothing in
   this file was read` と `belongs in filer.toml and was ignored` の書き分け）で、
   その文字列を組むのは `Misplaced::warn`。描画は関係しない。フレームを 1 枚描いても
   `cfg.warnings` に入った文字列を読み返すだけで、**同じことを 100 倍遅く確かめる**ことになる。
2. 33.3 のフレームテストが「`cfg.warnings` の各行は `~` で `theme.warning` の行になる」を
   すでに見ている。**警告が画面に出る配線は 1 本しかない**ので、行ごとにフレームを
   描き直す価値が無い。

**届かなかったところ（プログラムの変更が要る、= 私のものではない提案）:**

- 33.11 / 33.12 の「**1 行だけ**。昔の `invalid type: map, expected a string` ではない」の
  **「ではない」の側**は、`Config::load` の `Err(_) if wrong.breaks_parse => {}`
  （`src/config/mod.rs:217` と `:249`）が決めている。ここは `config_dirs()` 越しに
  実ファイルを読むので、テストから入る継ぎ目が無い。
- `YAZI_CONFIG_HOME` / `FILER_CONFIG_HOME` を立てる手は取れない。プロセス全体で共有される
  変数で、`std::env::set_var` は 2024 edition では `unsafe`、しかも
  `config::dirs_tests::the_filer_layer_is_one_level_down_everywhere` は**この変数が
  立っていたら自分を飛ばす**ように書かれている。並列実行で互いを黙らせ合う。
- **提案**: `Config::load()` を `Config::load_from(dirs: &[PathBuf])` に割り、
  `load()` はそれを `config_dirs()` で呼ぶだけにする。そうすれば 33.11〜33.14 が
  「本物の `yazi.toml` を temp に書いて読ませ、警告が 1 行で、serde の型エラーが
  混ざっていない」という形で丸ごと自動化できる（33.15 の「No warnings」も同じ形で入る）。
  1 関数の分割で、挙動は変わらない。**判断は人に委ねる。**

### 節の注記が 33.16 / 33.18 を挙げていなかった

`ui::overlay::help_config_rows::a_file_on_disk_that_was_not_read_says_so` と
`the_reload_key_is_looked_up` は v0.34.0 の頃からあるが、33 節の注記は
「33.11 to 33.14 have unit tests of their own in `config::files`」までしか書いていなかった。
**注記は私の担当なので直した**（33.16 / 33.17 / 33.18 と、その置き場所を書いた）。
番号・id・行の文言は触っていない。

### `#[cfg(test)]` の中で直した 1 か所 — 手組みの temp パス

`ui::overlay::help_config_rows::a_file_on_disk_that_was_not_read_says_so` が
`std::env::temp_dir().join("filer-help-unread")` を組んでいた。CLAUDE.md が名指しで
禁じている形（「自分で `env::temp_dir().join(...)` を組まないこと」）で、
`cargo test` を 2 つ同時に回すと同じディレクトリを取り合う。
`crate::util::test_dir("help-unread")` に替えた。あわせて `App::new` に渡す cwd も
システムの temp ディレクトリからその temp ディレクトリに変えている——`kick_scans` が
`/tmp` 全体のスキャンを投げていたので、#25 が踏んだのと同じ地雷の隣にいた。
**`#[cfg(test)]` の内側なので規則の範囲内**だが、他人のテストを触ったことは書いておく。

### 実機に残るもの（節の注記にも同じことを書いた）

- **33.6** — 明るい背景で `#e8c87a` が読めるか。**恒久的に人の目**。
- **33.4 の「at a glance」の部分** — 色が違うことは上のテストが言う。人が見分けられるかは言わない。
- **33.7 の文面** — 5 行・行番号・キャレットは見ているが、`toml` の文面そのものの読みやすさは人。
- **33.15** — ファイルが本当にディスクにあること、`filer env` の答え、端末ペインが
  `[term] shell` の指すものを実際に起動すること。3 つとも実機。

### この run と関係のない作業ツリーの変更

作業を始めたときの `git status` は `TESTING-CHECKS.md` の変更と `scripts/smoke-linux.sh`、
`.github/workflows/ci.yml` を出していたが、作業中にそれらが消え、代わりに
`src/ui/mod.rs` に**私が書いていない**変更（`harness::Painted::glyphs` と
`Painted::drawn`、`ui::awkward_names::a_very_long_name_is_cut_down_to_its_column`）が
現れた。**同じチェックアウトを別のセッションが並行して触っている。**

- clippy は最終的に **host と `x86_64-pc-windows-msvc` の両方で `-D warnings` を通して
  0 件**。途中では `Painted::drawn` が未使用という警告が 1 件出ていたが（私の行ではない）、
  向こうのセッションがその後それを使うテストを入れて消えた。**つまり私が検証した木は、
  途中で 2 回中身が変わっている。**私の追加分からの警告は最初から 0 件。
- `cargo test` の件数も動いた（465 → 私の 6 件で 471 → 最終 474）。**差分の 3 件は
  別セッションのもの**なので、私のぶんとして数えないこと。
- **`git diff TESTING.md` は 3 か所出るが、私のものは 33 節の注記だけ。**12 節と 24 節の
  注記の書き換えは向こうのセッションのもの。コミットを分けるなら、そこで切ること。

---

## TESTING.md sections 12 / 24 — 2 度目の変換（07c0268 時点）

12 節と 24 節は前回（97dec91）の変換で 12.6 / 12.7 と 24.1 / 24.3 まで進んでいた。
その続きで、**残った行のうちフレームが本当に届くものだけ**をテストにした。

| 節 | テストモジュール | 足したテスト | 何を見ているか |
| --- | --- | --- | --- |
| 12 | `ui::undo_frame`（`src/ui/mod.rs`、3 → 5 件） | `capital_u_does_the_rename_again_and_says_which_way_it_went` | 12.4 が言う規則を**リネームの形で**。`U` でファイルが新しい名前に戻り、トーストが `Renamed to two.txt`（`undone_label` と別の文）、さらに `u` で往復できる |
| 12 | 同上 | `an_undo_blocked_by_a_taken_name_can_be_pressed_again` | 12.9 が言う規則をリネームの形で。`u` が `Undo: Already exists: one.txt` と**名前を挙げて**断り、ステップを積んだまま残し、邪魔をどけた 2 回目で通る |
| 24 | `ui::awkward_names`（`src/ui/mod.rs`、2 → 3 件） | `a_very_long_name_is_cut_down_to_its_column` | 24.2 の**半分**。163 文字の名前（fixtures と同じ形）が 1 行に縮められることまで。`…` の位置は assert していない（下記） |
| 24 | 同上（既存テストの強化） | `cjk_names_are_drawn_whole` に 1 行 | `f.drawn(n) == Some(n)`。**`texts` は「並べるよう渡した文字列」なので、列に収まらず切られた名前も丸ごと入っている** —— 元の assert だけでは「切られていない」ことは言えていなかった |

**12.4 / 12.8 / 12.9 の行そのものは自動化されていない。**どれも `d`（ゴミ箱）の話で、
テストが駆動しているのは「その行が述べている規則」をリネームが同じく守っていること。
節の注記にもそう書いた。なお作業指示にあった「12.9 / 12.12 は自動化済み」は誤りで、
**自動化済みの一覧は節の注記が正**（今回の前は 12.6 / 12.7 の 2 件だけ）。

### ハーネスに足したもの（`#[cfg(test)]` の内側）

前回の報告の「提案: ハーネスが galley の『見えている文字』も拾えるようにする」を実装した。
`harness::Painted` に `glyphs: Vec<String>`（`texts` と同じ順で、**実際に字になった文字だけ**）と
`Painted::drawn(text)` の 2 つ。`collect()` の `Shape::Text` の腕が 1 行増えるだけで、
**プログラムは 1 行も変えていない。**

- `drawn()` は `contains` ではなく**完全一致**で引く。ファイル名はヘッダが描く
  hovered パスの中にも含まれていて、そのパスは**それ自身の理由で末尾が縮んでいる**ので、
  `contains` だと別の galley の答えが返る（実測でそうなった）。
- これで 24.2 のような「縮んだかどうか」の項目が届くようになった。同じ理由で届いていなかった
  タブ見出し・ヘッダのパスも、これで assert できる（今回は 24 節の範囲に留めた）。

### 24.2 — 文言と実装の食い違い（前回の指摘が確認できた）

前回「可能性が高い」としていた点が、`glyphs` で**実測として確認できた**。

| # | 見つけたこと | 根拠 | どちらの間違いか |
| --- | --- | --- | --- |
| 24.2 | 「Elided **in the middle**, with the extension still readable」は成り立たない。一覧の名前は**末尾**が `…` になり、`.txt` は残らない | 実測（1280px、163 文字の名前）: 描かれた字は `very-long-long-…-long-…`（71 文字、末尾 `…`、拡張子なし）。実装は `src/ui/list.rs` の `name_job()` が `wrap.max_rows = 1` と `overflow_character = Some('…')` を置くだけで、真ん中を抜く処理はどこにも無い | **どちらとも言える。**下の 2 択 |

1. **項目の文言を直す** — 「末尾が `…` になり、拡張子は隠れる」。一覧の名前は左から読むもので、
   egui の標準の縮め方に任せるという立場。
2. **`name_job()` を真ん中抜きにする** — `util::ellipsize_middle()` が**すでにある**
   （タブ見出しがそれを使っていて、実測で ` 1 filer-frame…-20449 ` と真ん中が抜けている）。
   ただし `name_job()` は**あいまい検索のハイライト位置（文字 index）を持った `LayoutJob`** を
   組んでいるので、先に名前を縮めると index がずれる。見た目の話に見えて中は面倒。
   **プログラムの変更なので QA では触らない。**

推奨は **1**。同じフレームの中で 3 か所が縮んでいて、**タブ見出しだけが真ん中抜き、
一覧の名前とヘッダのパスは末尾切り**という状態なので、揃えるなら「どこを揃えるか」を
先に決める話になる（ヘッダのパスは末尾切りのほうが正しいはず —— 末尾はファイル名で、
真ん中を抜くとどのディレクトリか分からなくなる）。テストは**どちらに倒しても落ちない形**で
書いてあるので、決まってから 1 行足せば 24.2 は閉じられる。

### 12 節の「届かない」理由を訂正する — ops ワーカーは回せる

前回の報告は 12.1–12.5 / 12.9–12.12 を「**ハーネスはワーカーを回さず、`drain_channels` も
呼ばない**」という理由で外していた。**その理由は現在は正しくない。**同じ回に入った
`Screen::turn()` が `drain_channels` → `kick_scans` → `request_preview` → draw を回すので、
ops ワーカーのジョブも完了まで回せる。ゴミ箱の往復も Linux で通ることを確かめた
（使い捨てのテストで実測: `trash::delete` → `restore::found` が 1 件 → `put_back` → ファイルが戻る）。

それでも今回**書かなかった**のは、理由が別にあるから。判断は人に委ねる。

- **人の本物のゴミ箱／ごみ箱に書く。**`cargo test` が Recycle Bin に項目を作ることになる。
  `d` → `u` の往復だけなら出したものを戻すので後には残らないが、12.4（`U` で再削除）や
  12.5（同名 2 つ）は**テストが終わったあともゴミ箱に残る。**
- **macOS では `u` が成り立たない。**`restore::SUPPORTED` が false（Finder 以外から
  ゴミ箱を読めない）なので、`d` → `u` のテストは `cfg` で切るか、メッセージのほうを
  assert する形にしないと落ちる。
- **速さと再現性が人の環境に依存する。**`restore::found()` はゴミ箱を**全部**走査するので、
  数万件持っている機械では目に見えて遅い。加えて `/tmp` が別マウント（tmpfs）だと
  freedesktop のゴミ箱は `$topdir/.Trash-$uid` 側に行くので、**戻せるかどうかが
  マウント構成で変わる。**PR #25 で踏んだ「片方の機械の運で緑になる」形そのもの。

倒し方の案:

- **案 A（推奨）: 12 節はこのまま実機に残す。**ゴミ箱は OS の機能で、行が見たいのは
  「Explorer の undo に 1 件」「ロックされたファイルの名前が出る」など**OS 側の見え方**。
  21 節（書庫ジョブ）と同じ扱いにする。
- 案 B: `#[ignore]` を付けた opt-in のテストを 12.1 / 12.2 に置く（`cargo test -- --ignored`
  で人が回す）。CI では走らないので**腐っても気付けない**のが欠点。
- 12.3 だけは「`u` を押すとタスクパネルに `Restore` 行が出る」までなら**ワーカーを待たずに**
  書ける（`submit_op` がその場で `Task` を積むので）。ただし行は「**appears and completes**」と
  言っていて、completes は待たないと言えない。**行より弱い assert になるので書かなかった。**

### ついでに気付いたこと（間違いではない）

| # | 見つけたこと |
| --- | --- |
| 12 節全体 | どの行も「ゴミ箱がある OS」を前提にしている。macOS では `d` は通るが `u` が `restoring from the trash is not supported on this platform` と断る。**その差分の行がどこにも無い**（35 節はパスの話）。CLAUDE.md がマルチ OS を目標に挙げているので、行を 1 つ足す価値はある。番号を増やす話なので提案にとどめる |
| 24.4 | 引用そのものは `terminal::tests::a_path_reaches_the_shell_as_one_word` が見ていて、fixtures の `quote'in-name.txt` と同じ `'` 入りの入力も入っている。残っているのは「ペインに届く」ところだけ（ConPTY、`#[cfg(windows)]`）。節の注記にそう書いた |
| 12.5 | 「同名 2 つのうち新しいほうが戻る」規則は `fs::restore::tests::keeps_the_newest_of_two_by_the_same_name` が見ている。残っているのは本物のゴミ箱を通ること |

### `make-testcheck` は「行頭に折り返した id」を自動化済みと読む — 12.9 / 12.12 が人の一覧から消えていた

**今回いちばん大きい指摘。**`examples/make-testcheck.rs` の `automated_ids()` は
「`#[test]` の前の doc コメントが id で**始まっていれば**、そのテストがその行を覆っている」と
読む。コメントの日本語訳でいうと「prose を parse しないので、間違うとしても安全な側にしか
倒れない」と書いてある。**倒れていた。**`#[cfg(test)]` も `t.contains("test)")` に一致するので
**モジュールの doc コメントも同じ扱い**で、そこに置かれていたのがこの 2 行:

```rust
/// channels, so nothing a delete would produce ever arrives -- 12.1 to 12.5 and
/// 12.9 to 12.12 stay with the machine. A rename is the one undoable action that
```

2 行目が `12.9` で始まっているので、`12.9` と `12.12` が automated に入る。
**「実機に残す」と書いた文が、その 2 行を実機の一覧から外していた。**
（作業指示にあった「12.6 / 12.7 / 12.9 / 12.12 は自動化済み」は、
おそらく TESTING-CHECKS.md をそう読んだもの。**節の注記のほうが正しかった。**）

- 実測: `cargo run --example make-testcheck -- --check` の drift 報告が、コメントを
  折り返し直したあと `+ 12.9` と `+ 12.12` を出す（= 人の一覧に戻る）。
- 今回の修正は**コメントの折り返しだけ**（`#[cfg(test)]` の内側なので QA の範囲）。
  同じ罠を次に踏まないよう、なぜ id を行頭に置かないかをそのコメント自身に書いた。
- **プログラム側の提案（QA では直さない）**: `automated_ids()` が id を数えるのは
  **`#[test]` に付いた doc コメントだけ**にする。いま `is_test` は `#[cfg(test)]` にも
  立つので、モジュール全体の説明文が 100 件のチェックを外しうる。`t.contains("test]")` と
  `t == "#[test]"` 相当に絞るか、`mod` で始まる item を除くかの 2 択。
  あるいは `scripts/testcheck-ja.toml` の `manual` に書き足す（13.10〜13.12 と同じ手当て）。
  ただし**それは症状のほうを押さえる手当て**で、原因は折り返しのほうにある。

なお `--check` は現在 exit 1 で、差分は 3 種類ある。**どれも QA が書き換えるファイルではない**
（`[x]` は人のもの）ので、再生成は人に任せる:

1. `+ 12.9` / `+ 12.12` — 上記。人の一覧に戻る（正しい方向）。
2. `* 33.4` — 並行して走っていた 33 節のセッションが自動化した分。
3. `~` が 14 件 — 行末の `**` が落ちた状態で TESTING-CHECKS.md に入っている
   （`was:` 側が途中で切れている）。**今回の作業とは無関係**で、パーサが直ったあと
   再生成されていないだけに見える。

---

## TESTING.md section 25 — Windows 実機で確かめた（b9daf8a / 0.47.22）

Windows のセッション（`.claude/windows-role.md`）から。25 節の 19 行のうち 15 行を
TESTING-CHECKS.md でチェックした。証拠は PR 本文に 1 行ずつある。ここにはプログラムの
バグ 3 件（末尾の 2 件は、同じ節を実機で確かめ直したときに見つけた）と、TESTING.md の
行の食い違い 2 件を書く。どれも直していない。

### `filer env` / `--help` の非 ASCII 文字が、日本語 Windows の既定のコンソールで化ける

- **どこ**: `src/main.rs` の `say()`。`CONOUT$` を `std::fs::File` として開き、
  `writeln!` で **UTF-8 のバイト列を `WriteFile` している。**コンソールはそのバイト列を
  **現在のコードページで**解釈するので、日本語 Windows の既定（CP932）では UTF-8 として
  読まれない。
- **実測**（release の `filer.exe` を新しい `powershell` のコンソールで起動し、
  コンソールバッファを読み戻した。コンソールバッファは画面に出ている文字そのもの）:

  | 出力のどこ | CP932（既定） | `chcp 65001` のあと |
  | --- | --- | --- |
  | `Recorded by : filer 0.47.19 — an earlier run` | `filer 0.47.19 窶・an earlier run` | `—` のまま出る |
  | `--help` 1 行目 `filer — a yazi-flavored file manager` | `filer 窶・a yazi-flavored file manager` | 正しい |
  | `FILER_CONFIG_HOME=C:\tmp\f25\日本語` の Config 行 | `C:\tmp\f25\譌･譛ｬ隱杤  : nothing here` | 正しい |
  | 同じく Variables 行 | `FILER_CONFIG_HOME : C:\tmp\f25\譌･譛ｬ隱・    FILER_STATE_HOME  : …`（**改行まで食われて次の行と 1 行になる**） | 正しい |

- **なぜ重いか**: `filer env` は「バグ報告に貼るためのもの」で、仕事は**パスを正しく
  見せること**。日本語のユーザー名（`C:\Users\山田\…`）や日本語のフォルダにある設定は、
  まさにその報告を書く人の環境で**読めない形で**出る。最後の 1 バイトが次の `\r\n` と
  組になって 1 文字に化けるので、行の区切りまで壊れる。
- **提案（直していない）**: `CONOUT$` へは `WriteConsoleW` で UTF-16 を書く。
  `std::io::Stdout` がコンソールに対してやっているのと同じ経路で、コードページに
  依存しない。`AttachConsole` のあと `GetStdHandle` が無効なのは変わらないので、
  `CONOUT$` のハンドルを `CreateFileW` で開いて `WriteConsoleW` に渡す形になる。
  `SetConsoleOutputCP(65001)` で逃げる手もあるが、**親のコンソールの設定を書き換えて
  そのまま返す**ことになるので勧めない。
- 実機の行を足すなら 25 節に「CP932 のコンソールで `filer env` → `—` と日本語のパスが
  そのまま出る」。いまの 25.1 は「文字が出ること」しか見ていないので、この化け方でも通る。

### 25.4 の行が、いまの Tools 節と合っていない

- **行の文言**: `pdftoppm` `ffmpeg` `ffprobe` `pwsh` `git` が並び、入っていれば版、
  無ければ `not found`。
- **プログラム**（`src/envreport.rs` の `tools()`）: 固定で出すのは `git` と
  **実際に起動するシェル 1 つ**だけ。あとは `filer.toml` の `[[preview]]` と
  `yazi.toml` の opener が名指ししたプログラムを、**実行せずに探して**パスを出す
  （版は出さない）。doc コメントが「filer が実際に走らせるものだけ」と理由まで書いている。
- **実測**: 空の設定では Tools は `git` と `powershell` の 2 行だけ。
  持ち主の設定では `pdftoppm` と `ffmpeg` がパス付きで `(preview *.pdf)` のように出るが、
  それは `[[preview]]` に書いてあるから。**`ffprobe` はどの構成でも出てこない。**
- **どちらの間違いか**: 行のほう（コードは意図どおりで、理由も書いてある）。
  行を「`git` と実際のシェルが並ぶ。`[[preview]]` と opener が名指ししたプログラムが、
  何のためのものか付きで並ぶ。見つからなければ `not found`」に直す提案。
  **25.4 はチェックしていない。**

### 25.1 の「4 つの節」は 5 つ

`Filer` / `Config` / `Last run` / `Tools` / `Variables` の 5 つが出る。`Last run` は
v0.29.0 で足された節で、行の数字だけが古い。チェックは「文字が実際に出る」ことに対して
付けた。

### `keymap.toml` の警告にだけディレクトリが付かない

- **どこ**: `src/config/keymap.rs` の `Keymap::load()`。`config::load` は各ディレクトリの
  `keymap.toml` を**テキストだけ** `keymap_texts` に積んで渡す（`src/config/mod.rs` の
  `read(dir, "keymap.toml", …)`）ので、パースに失敗したとき `load()` の手元には
  どのファイルだったかが無く、`format!("keymap.toml: {e}")` と**名前だけ**を出す。
  `yazi.toml` / `theme.toml` / `filer.toml` は `at(dir, …)` でフルパスを付けている。
- **実測**（`FILER_CONFIG_HOME=C:\tmp\t25\filer` に壊した `yazi.toml` と
  壊した `keymap.toml` を置いて `filer env`）:

  ```text
  Warnings    : C:\tmp\t25\filer\yazi.toml: TOML parse error at line 3, column 1
                keymap.toml: TOML parse error at line 4, column 1
  ```

  `keymap.toml` を yazi 側のディレクトリ（`YAZI_CONFIG_HOME=C:\tmp\t25\typo`）に
  置いても、出るのは同じ `keymap.toml:` だけだった。
- **なぜ困るか**: `keymap.toml` は **yazi 側と filer 側のどちらにも置ける**
  （両方あれば両方読む）。どちらが壊れているかは、この行からは分からない。
  25.15 が「直すファイルを名指すのが仕事の唯一のメッセージ」と書いているのと同じ話で、
  同じ Warnings 欄の中で 1 行だけその仕事をしていない。
- **提案（直していない）**: `keymap_texts` をパスと組にして渡し（`Vec<(String, String)>`
  など）、`load()` は `at(dir, "keymap.toml")` で作った文字列を前に付ける。
  実機の行を足すなら 25 節に「yazi 側の `keymap.toml` を壊して `filer env` →
  Warnings の行がそのファイルのフルパスで始まる」。

### ボールドの兄弟フォントのパスが `/` と `\` の混在になる

- **どこ**: `src/main.rs` の `bold_siblings()`。ユーザーが書いたフォントのパスの
  `parent()` に `dir.join("consolab.ttf")` などで**ファイル名を `\` で継ぐ。**
  `[ui] fonts` を `/` で書いていると、ディレクトリ部分は `/` のまま残る。
- **実測**（`filer.toml` に `[ui] fonts = ["C:/Windows/Fonts/consola.ttf"]`、
  `FILER_STATE_HOME=C:\tmp\t25\run\st` で filer を開いて閉じ、`filer env`）:
  - `last-run.toml` に `'C:/Windows/Fonts\consolab.ttf'` が記録された。
  - `filer env` の Last run 節は `Fonts : C:/Windows/Fonts/consola.ttf`（書いたとおり）、
    `Bold : C:/Windows/Fonts\consolab.ttf`（混在）。
  - フォント自体は読めている（Windows は両方の区切りを受け付ける）。
    壊れているのは**見せ方**だけ。
- **なぜ書くか**: 25.15 で直したのと同じ種類の混在で、場所も同じ `filer env` の出力。
  Last run 節の仕事は「どのファイルからボールドが来たか」を読み手が見に行ける形で
  見せることなので、25.15 と同じ基準で見れば直す対象になる。
- **提案（直していない）**: Windows では設定から来たフォントのパスの `/` を `\` に
  揃えてから `bold_siblings()` に渡す（または `used.fonts` / `used.bold` に入れる前に揃える）。
  `Fonts` 行も書いたとおりの `/` で出ているので、揃えるならそちらも同時に。
  実機の行を足すなら 25 節に「`[ui] fonts` を `/` で書いて開閉 → `filer env` の
  Bold 行に `/` と `\` が混ざらない」。

### `walking_off_the_file_and_back_needs_no_worker` が Windows で時々落ちる

- **どこ**: `src/ui/mod.rs` の `preview_arrival_frame::walking_off_the_file_and_back_needs_no_worker`
  （27.3）。
- **いつ**: PR #40 の CI（`7fb1481`）で 1 回。`476 passed; 1 failed` で、
  `one turn was enough: the cache answered` が落ちた。**同じ日の同じ Windows ランナーで
  #38 / #39 / #41 は通っている。**その PR の diff は Markdown 2 ファイルだけで、
  ソースは 1 行も変わっていない。再実行では緑。**4 回に 1 回。**
- **テストの主張**: `alpha.txt` → `beta.txt` → 戻る、と歩いたとき、**キャッシュが答えるので
  ワーカーは要らない**。これ自体は正しい主張で、弱めるべきではない。
- **検証の書き方**: その主張を `s.turn()` **1 回**に賭けている。`turn()` は
  `drain_channels` → `kick_scans` → `request_preview` → `draw` の順に回るので、
  **非同期な出来事が 3 つ挟まった 1 回**になっている。ウォッチャの通知が遅れて届いて
  フォルダが `LoadState::Loading` に戻れば、その 1 回はスキャンに使われて
  `preview_arrived()` は false になる。Windows の変更通知は Linux の inotify より
  遅れて届くので、**Linux で何度通っても Windows でだけ時々落ちる**形になる。
- **まだ言えないこと**: 上は機序の候補であって、**確かめていない。**
  キャッシュのキー（`src/app.rs:1566`）は `len` と `mtime` を持つので、
  再スキャンが `alpha.txt` に別の mtime を報告した場合も同じ落ち方をする。
  どちらなのかは、落ちた瞬間の `preview.state` と `current.state` を見ないと分からない。
- **提案（直していない）**: 主張はスキャンと無関係なので、`turn()` ではなく
  `s.app.request_preview(false)` を直接呼んでから `s.draw()` すれば、
  **テストを弱めずに**非同期を 1 つも挟まずに書ける。ただし上の 2 つ目の機序
  （mtime が動く）だとこれでは直らないので、**先に機序を確かめること。**
- **なぜ書くか**: 再実行して緑になったので、このまま黙っていると記録が残らない。
  **通った回数を数えて判断し始めると、5 回目の失敗が黙って通る。**

## TESTING.md section 8 — Windows 実機で確かめた（eb80fc9 / 0.47.25）

Windows のセッション（`.claude/windows-role.md`）から。ConPTY のペインを実機で初めて
起動した。試したシェルは `powershell`（5.1.26100）、`pwsh`（7.6.6）、`cmd`。**どのシェルでも
落ちず、固まらず、文字化けもなかった**（`cmd` のペインに `dir /b` で出した `日本語テスト.txt`
もそのまま読めた）。7 行のうち 5 行（8.1 / 8.3 / 8.4 / 8.5 / 8.6）を TESTING-CHECKS.md で
チェックし、証拠は PR 本文に 1 行ずつ書いた。8.2 と 8.7 は、**書いてある手順どおりに
やると期待どおりにならない**のでチェックしていない（下の 1 件目）。

試したバイナリは 0.47.24 のソースから作った release ビルド。`src/` は eb80fc9 と同じ。
設定は `FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` / `FILER_STATE_HOME` で一時ディレクトリに
隔離した（人の設定には `[term] shell = "pwsh"` が入っていて、8.1 の前提を満たさないため）。

### 8.2 / 8.7 と 8 節の注記: `<C-S-t>` → `<C-t>` では `[term]` を読み直さない

- **書いてあること**: 8.2 は「`shell = "pwsh"` を足して `<C-S-t>`、`<C-t>`」、8.7 は
  「`[term]` を消して `<C-S-t>`、`<C-t>`」。TESTING-CHECKS.md の 8 節の注記も
  「設定を変えたら `<C-S-t>` でシェルを終わらせてから `<C-t>` で開き直すこと」と書いている。
- **実際**: その手順では**前のシェルがまた起動する**。
  - 8.2: `shell = "pwsh"` を書いて `<C-S-t>` → `<C-t>` のあとのフィラーの子は
    `powershell.exe  cmd=[powershell]`（5.1 のまま）。
  - 8.7: `filer.toml` を消して `<C-S-t>` → `<C-t>` のあとの子は
    `cmd.exe  cmd=[cmd /k R:\Temp\w8\probe.cmd]`（消す前の設定のまま）。
  - どちらも `<C-S-t>` → `<C-F5>` → `<C-t>` にすると期待どおりになる
    （`pwsh.exe cmd=[pwsh]` / `powershell.exe cmd=[powershell]`）。
- **なぜ**: シェルは `self.cfg.term.shell` から取っていて（`src/app.rs:3994`）、`self.cfg` が
  変わるのは `reload_config`（`<C-F5>`）だけ。`<C-S-t>` はシェルを終わらせるだけで、
  設定を読み直さない。**プログラムはこの設計どおりに動いているので、直すのは行の文言。**
- **提案（直していない）**: 8.2 / 8.7 の Do を「`<C-S-t>`, `<C-F5>`, `<C-t>`」にし、
  注記も同じにする。順番に意味がある: **ペインにフォーカスがあるあいだは `<C-F5>` が
  シェルに渡る**（`[term]` 層は `<C-F5>` を持っていない）ので、`<C-S-t>` でリストに
  戻ってから押すこと。この「ペインにいると `<C-F5>` が効かない」も設計どおりだと思うが、
  知らないと「読み直したのに変わらない」に見えるので、注記に一言あるとよい。

### シェルの起動に失敗するたびに `OpenConsole.exe` が 1 つ残る

- **再現**: `[term] shell = "nosuchshell-w8"`、`<C-F5>`、`<C-t>`。8.6 の期待どおり
  トーストが出てペインは開かない。**そのたびに**フィラーの子として
  `OpenConsole.exe --headless --width 80 --height 24 --signal 0x… --server 0x…` が 1 つ増え、
  5 秒以上たっても消えない。3 回やって 3 つ（pid 52832 / 44300 / 40916、どれも
  ppid = フィラー）。フィラーを閉じると 3 つとも消えた。
- **原因**: `alacritty_terminal` 0.26.0 の `src/tty/windows/conpty.rs`。
  `new()` は `CreatePseudoConsole` が成功したあと（122 行）、`CreateProcessW` の失敗
  （233 行）などで `return Err(Error::last_os_error())` する。そのとき `Conpty { handle, api }`
  （241 行）はまだ作られていないので、`ClosePseudoConsole` を呼ぶ `Drop for Conpty` が
  走らない。**疑似コンソールの HPCON が閉じられず、そのホスト（OpenConsole）が残る。**
  157 / 183 / 200 行の早期 return も同じ形。
- **影響**: フィラーが生きているあいだだけ、失敗 1 回につきプロセス 1 つ。設定を
  書き間違えて何度か `<C-t>` を押す、という普通の操作で溜まる。
- **提案（直していない）**: 根はクレート側なので上流への報告が本筋。フィラー側でできる
  手当ては、spawn の前にシェルを `PATH` で解決して、見つからなければ ConPTY を作らずに
  エラーにすること（`envreport.rs` がすでに同じ解決をしている）。これで「インストール
  されていない」場合は漏れなくなる。見つかったが起動できない場合は残る。

### 8.6 のメッセージがシェルの名前を言わない

- 出たトーストは `Terminal failed: 指定されたファイルが見つかりません。 (os error 2)`。
  何が見つからないのかが書いていない。8.6 の期待（「その旨が出る」）は満たしているので
  チェックは付けた。
- **提案（直していない）**: `Terminal failed: nosuchshell-w8: …` のように `[term] shell` の
  値を入れる。設定を書いた本人が読むメッセージなので、自分が書いた文字列が見えれば
  綴りの間違いにすぐ気づける。

### ConPTY が WezTerm の `conpty.dll` / `OpenConsole.exe` から来ている

- 上の `OpenConsole.exe` は `C:\Program Files\WezTerm\OpenConsole.exe`。
  `ConptyApi::load_conpty` が `LoadLibraryW("conpty.dll")` を**名前だけで**呼ぶので、
  `PATH` 上にある WezTerm の `conpty.dll` が選ばれ、それが隣の `OpenConsole.exe` を起動する。
  WezTerm が無い機械では `kernel32` の `CreatePseudoConsole`（conhost）に落ちるはず。
- **バグではない**が、**機械によって別の ConPTY 実装が動く**ことになる。この機械の結果は
  WezTerm 同梱版のもの。「この機械では出ない／出る」が起きたら、まず `PATH` に
  `conpty.dll` があるかを見ること。

### `cargo test` が開発者本人の設定を読んで 5 件落ちる

- **再現**: この機械で、環境変数を何も立てずに `cargo test`（eb80fc9）。
  `472 passed; 5 failed`。`FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` / `FILER_STATE_HOME` を
  空のディレクトリに向けると `477 passed`。
- **落ちるもの**:
  - `envreport::tests::it_reports_what_is_actually_there`（`src/envreport.rs:447`）:
    `pdftoppm is not used yet`。人の `yazi.toml` に `pdftoppm` / `ffmpeg` を使う previewer が
    あるので、Tools 節に出る。**テストの主張（コードに無いツールを名指ししない）は、
    設定がツールを名指ししている場合には成り立たない。**
  - `ui::csv_table_frame::the_table_relays_out_when_the_window_is_resized`（`src/ui/mod.rs:4140`）、
    `ui::whole_frame::a_long_file_gets_a_strip_and_a_narrow_window_does_not`（`src/ui/mod.rs:1693`）、
    `ui::overlay::help_frame::the_wheel_turns_the_panel_and_leaves_the_list_alone`（`src/ui/overlay.rs:1705`）、
    `ui::overlay::help_frame::a_panel_owns_the_wheel_and_a_prompt_leaves_it`（`src/ui/overlay.rs:1759`）:
    列数や行数を数えるフレームのテスト。人の設定のフォント（HackGen35 Console NF）や
    レイアウトの設定で、テストが前提にしている幅・高さが変わる。
- **なぜ書くか**: CI は設定の無いランナーなので緑になり、**手元でだけ赤い**。
  CLAUDE.md の「CI が緑でもマージ前に手元で全部回す」は、この機械では 5 件の既知の失敗を
  抱えた状態になる。それは CLAUDE.md が戻すなと言っている「既知の N 件」の形そのもの。
- **提案（直していない）**: `Config::load()` を通るテストが、設定の場所を隔離した
  ディレクトリに向けるようにする（`util::test_dir` と同じ考え方で、テスト全体で 1 回）。
  環境変数はプロセス全体に効くので、テストごとではなく**テストバイナリの始めに 1 回**
  立てる形になる（246 行・661 行で書いた並列実行の問題を避けるため）。

## TESTING.md section 35 — Windows 実機で確かめた（084fa53 / 0.47.24）

Windows のセッション（`.claude/windows-role.md`）から。35 節の 10 行のうち Windows で
確かめられるのは 35.1 / 35.2 / 35.9 / 35.10 の 4 行で、そのうち 3 行（35.1 / 35.9 /
35.10）を TESTING-CHECKS.md でチェックした。証拠は PR 本文に 1 行ずつある。
35.3〜35.8 は macOS / Linux の行なので触っていない。プログラムのバグは見つからなかった。

### 35.1 の突き合わせ — v0.34.0 を実際にビルドして比べた

「v0.34.0 から動いていない」を読むために、`356241c`（v0.34.0）を `git archive` で
取り出して release ビルドし、3 つの変数を未設定にした新しいコンソールで両方の
`filer env` を走らせた。Config 節の 7 行は**1 文字も違わなかった**（`Compare-Object`
で差分 0）。

```text
Config
    C:\Users\yuu06\AppData\Roaming\yazi\config\ : yazi.toml 2.6 K   keymap.toml 0 B
                                                  not here: theme.toml, filer.toml
    C:\Users\yuu06\AppData\Roaming\filer\       : filer.toml 1.9 K
                                                  not here: yazi.toml, keymap.toml, theme.toml
    State                                       : C:\Users\yuu06\AppData\Roaming\filer
    Warnings                                    : none
```

### 35.10 の「確かめ直す」には、期待する結果が書かれていない

- **行の文言**: 「リンクのパス経由で、保存時に改名するエディタで編集したあと
  もう一度確かめること（それをするとシンボリックリンクが普通のファイルに置き換わる）」。
  置き換わることは書いてあるが、**そのあと何が起きれば合格なのか**が書かれていない。
- **実測**: filer を開いたまま、`nvim --headless --clean -c 'set backupcopy=no'` で
  **リンクのパス**を編集した（`backupcopy=no` は改名で保存する設定）。
  - リンクのパスは `LinkType` が空の普通のファイルになり、新しい内容を持つ。
  - **元のリンク先は古い内容のまま**残る。
  - `<C-F5>` のあと `last-run.toml` のフォントは新しい内容（`georgia.ttf`）になった。
    つまり filer は置き換わった普通のファイルを読む。2 回とも同じ（3 回目は前面が
    取れずキーを送っていない）。
- **提案（直していない）**: Expect に「`<C-F5>` で新しい内容が読まれる。ただし
  リンク先のファイルはもう更新されない — リンク先で設定を管理している人は、
  編集がそちらに届かなくなったことに気付かない」と書く。後半が、この行が注意を
  促したい本当の点だと思う。

### 実機に残るもの

- **35.2** は、`%APPDATA%\yazi\config\yazi.toml` に `sort_by = "mtime"` を書く行で、
  持ち主の設定そのものを書き換えることになるのでしていない（この機械ではその
  ファイルが `C:\dev\obsidian-notes\…` へのシンボリックリンクで、書けばノートの
  リポジトリまで変わる）。**読まれていること自体は文字で確かめられた**:
  変数を未設定にした `filer env` の Tools 節に、`yazi.toml` の `[opener]` にだけある
  `sakura.exe` / `Hidemaru.exe` / `i_view64.exe` が並んだ（`filer.toml` にはコメント行が
  1 行あるだけ）。並び順が mtime になるかは画面を見る行なので、持ち主が確かめる。

## TESTING.md section 21 — Windows 実機で確かめた（eb80fc9 / 0.47.24）

Windows のセッション（`.claude/windows-role.md`）から。21 節の 9 行のうち 8 行を
TESTING-CHECKS.md でチェックした。証拠は PR 本文に 1 行ずつある。**21.4 はチェックして
いない**（下の 1 つ目）。ここには行の食い違い 2 件、プログラムの気になる点 2 件、
テストの隔離の穴 1 件、ハーネスの注記を書く。どれも直していない。

作業場所は `R:\Temp\ar`（RAM ディスク）。filer は `FILER_CONFIG_HOME` /
`YAZI_CONFIG_HOME` / `FILER_STATE_HOME` を `R:\Temp\ar\cfg\{f,y,s}` に向けた空の設定で
起動した。**`R:` は消えるので、ディスクの状態はここに写してある。**

### 展開先・圧縮先が既にあるとき（21.3 と、行に無い圧縮側）

- **展開（`e`）: 別名。上書きも拒否もしない。**`ops.rs` の `extract()` は
  `unique_name(archive::extract_dir(src, dest_dir))` で、使われている名前は
  `sample` → `sample_1` → `sample_2` と飛ばす。**既存のフォルダへ混ぜることもしない。**
  - 実測: `sample.zip` で `e` を 3 回。1 回目のあと `sample\file1.txt` を書き換えて
    `sample\MARKER.txt` を足し、2 回目のあと `sample_1\file1.txt` も書き換えた。
    3 回目が終わっても**どちらの書き換えも MARKER.txt も残っていた。**元の `sample.zip` は
    676 B・23:29:42 のまま。

    ```text
    R:\Temp\ar\w21
    <DIR>           23:37:58 sample
    <DIR>           23:38:04 sample_1
    <DIR>           23:38:26 sample_2
                676 23:29:42 sample.zip
                 14 23:38:20 sample\file1.txt        "EDITED-BY-TEST"
                  8 23:37:58 sample\MARKER.txt
                 16 23:38:20 sample_1\file1.txt      "EDITED-BY-TEST-1"
                 12 23:38:26 sample_2\file1.txt      "contents 1"
    （各フォルダに file2〜5.txt 12 B と nested\deep.txt 6 B）
    ```

- **圧縮（`E`）: 聞く。**`resolve_dest()` が「File already exists」を出し、
  o / a / s / S / r / q を受ける。`R:\Temp\ar\w21c` で `to-pack.zip` を 12 B の
  テキスト（`OLD-SENTINEL`）に差し替えて試した。

  | 押したキー | ディスク |
  | --- | --- |
  | `s`（skip） | `to-pack.zip` は 12 B の番兵のまま |
  | `q`（cancel） | 番兵のまま。新しいファイルもできない |
  | `r` → Enter | 提案どおり `to-pack_1.zip`（970 B）ができる。番兵のまま |
  | `o`（overwrite） | `to-pack.zip` が 970 B の zip になる。`7z t` で Everything is Ok、Folders 2 / Files 6 |

  上書きした `to-pack.zip` と `to-pack_1.zip` の SHA-256 は同じ
  （`9AB2E22F…691D9D`）。**同じ入力からは同じバイト列の zip ができる**
  （下の 1980 年の件の裏返し）。

### 21.4 の「the result opens」が何を指すのか決まらない — チェックしていない

- **実測**: `to-pack\` で圧縮 → Enter。`R:\Temp\ar\w21\to-pack.zip`（970 B）ができ、
  `7z l` で `to-pack\` の下に file1〜5.txt と `nested\deep.txt`（6 files, 2 folders）。
  **それ以外には何も起きなかった**（アプリも Explorer も開かず、カーソルも動かない）。
- **コード**: `app.rs` の `OpEvent::Finished`（3147 行）はタスクの状態を変え、
  エラーを出し、undo を積むだけ。**圧縮の結果を開く・選ぶ・見せる処理はどこにも無い。**
- **既存の報告の誤り**: 上の「21 節の届かない 9 件」の表の 21.4 の行が、
  「Packed, and the result opens」を**ジョブ完了後の `open`** と書いているが、そういう `open` は無い。
- **どちらの間違いか分からない**ので、行の文言は直さず、チェックもしていない。
  - 「結果の書庫が 7-Zip などで開ける（壊れていない）」の意味なら、上の `7z l` で満たす。
  - 「filer が結果を開く／カーソルを置く」の意味なら、機能が無い。
  行を「→ `to-pack.zip` ができ、7-Zip で中身が見える」に直すか、完了時に結果へ
  カーソルを置く機能を足すか、どちらかを決めてほしい。

### filer が書く zip は、全エントリの日付が 1980-01-01 になる

- **どこ**: `src/fs/archive.rs` の `write_zip()`（484 行）。
  `SimpleFileOptions::default().compression_method(Deflated)` だけで、
  `last_modified_time` を渡していない。zip クレートの既定は DOS 時刻の最小値。
- **実測**（同じ `to-pack\` から作った 3 形式）:

  ```text
  to-pack.zip     1980-01-01 00:00:00  to-pack\file1.txt
  to-pack.7z      2026-09-28 23:29:42  to-pack\file1.txt
  to-pack.tar.gz  9 28 23:29           to-pack/file1.txt
  ```

  **7z と tar は元の更新日時を持っているのに、zip だけ持っていない。**
- **展開側も戻さない。**7z を展開した `to-pack_1\to-pack\file1.txt` の更新日時は
  展開した時刻（23:40:56）で、書庫の 23:29:42 ではない。zip・7z とも同じ。
- **なぜ書くか**: 他のマシンで zip を開いた人には、全ファイルが 1980 年に見える。
  21.9 が言う「持って行ける」ことの一部だと思う。上の「同じ入力なら同じバイト列」は
  この副作用で、それが欲しいなら意図として doc コメントに書いておくべき。
- **提案（直していない）**: `m.path` の mtime を `zip::DateTime` にして
  `last_modified_time` に渡す。展開側で mtime を戻すかは別の判断。

### 往復するとフォルダが 2 段になる（`to-pack_1\to-pack\…`）

- **実測**（21.8）: `to-pack\` を `to-pack.7z` に固め、それを `e` すると、
  `to-pack\` が既にあるので `to-pack_1\` ができ、その中に `to-pack\` がある。

  ```text
  R:\Temp\ar\w21t
  <DIR>           23:40:11 to-pack
  <DIR>           23:40:56 to-pack_1
                404 23:40:23 to-pack.7z
  <DIR>           23:40:56 to-pack_1\to-pack
  <DIR>           23:40:56 to-pack_1\to-pack\nested
                 12 23:40:56 to-pack_1\to-pack\file1.txt   （file2〜5 も 12 B）
                  6 23:40:56 to-pack_1\to-pack\nested\deep.txt
  ```

  中身は `Get-FileHash` で 6 ファイルとも元と一致した（差分 0）。
- **なぜ書くか**: `extract_dir()` は書庫ごとに必ずフォルダを作るので、
  「中にフォルダが 1 つだけある書庫」（filer 自身がフォルダを固めたもの）では必ず 2 段になる。
  yazi など他のツールには、最上位がフォルダ 1 つならそれを剥がすものがある
  （yazi がそうだったと記憶しているが、確かめていない）。
- **バグではなく設計の問題**なので、直すかどうかは人の判断。直すなら
  「最上位がフォルダ 1 つだけなら、そのフォルダ名で（`unique_name` を通して）展開する」。

### 持ち主の設定があると `cargo test` が 5 件落ちる

- **実測**: 7d6197f（`src` は eb80fc9 と同じ）で、`FILER_*` / `YAZI_*` を
  **設定しない**まま（この機の持ち主の yazi / filer 設定が読まれる状態）`cargo test` →
  `472 passed; 5 failed`。空の設定ディレクトリに向けると 477 件すべて通る。

  ```text
  envreport::tests::it_reports_what_is_actually_there
  ui::csv_table_frame::the_table_relays_out_when_the_window_is_resized
  ui::overlay::help_frame::a_panel_owns_the_wheel_and_a_prompt_leaves_it
  ui::overlay::help_frame::the_wheel_turns_the_panel_and_leaves_the_list_alone
  ui::whole_frame::a_long_file_gets_a_strip_and_a_narrow_window_does_not
  ```

  1 件目は `pdftoppm is not used yet` で落ちる（持ち主の `filer.toml` の `[[preview]]` が
  `pdftoppm` を名指ししている）。
- **なぜ書くか**: CI のランナーには設定が無いので通る。**開発者の手元でだけ赤くなる**ので、
  「手元で全部回してからマージする」（CLAUDE.md）がこの機では成り立たない。
  失敗件数を数えて判断し始める危険も、CLAUDE.md が書いているとおり。
- **提案（直していない）**: テストの実行中は設定ディレクトリを空の一時ディレクトリに
  向ける（ハーネスと `envreport` のテストが、環境変数ではなく明示の引数で設定の場所を
  受け取る形）。

### 行の文言についての小さな注記

- **21.2 の進捗は書庫の数を数える。**展開のタスクパネルは
  `Extract 1 item(s) [done] 1/1 files · 676 B / 676 B` で、「files」は中のエントリでは
  なく書庫の数。圧縮のほうはエントリを数える（`6/6 files · 66 B / 66 B`）。
  コードどおりで、行も「進捗が出る」としか言っていないので食い違いではないが、
  同じ「files」が別のものを数えている。
- **21.10**: `tree\`（ファイル 300、サブフォルダ 25 と空フォルダ 1、525,900 B）を
  `tree.7z` に固めて `300/300 files · 514 K / 514 K`。`7z t` で Folders 26 / Files 300。
  **フォルダを数えていれば 326 になるはずで、ならなかった。**
- **21.11**: `to-pack.7z` 404 B < `to-pack.zip` 970 B、`tree.7z` 13,833 B <
  `tree.zip` 45,920 B。

### ハーネスの注記（再現する人向け）

- キーは `PostMessage` で **filer 自身のウィンドウにだけ**送った。`SendKeys` は前面の
  ウィンドウに届くので、**同じ機で他のセッションの filer が動いていると、そちらのキーが
  こちらの filer に入る**（実際、こちらのウィンドウで勝手に `sample` が展開された）。
- `PostMessage` では Shift を押した状態を作れないので、`E`（Shift+e）の代わりに、
  分離した `keymap.toml` で `<F9>` を `compress`、`<F10>` を `tasks_show` に割り当てた。
  **`E` の既定の割り当て自体は確かめていない**（それは TESTING-KEYS.md の仕事）。
- 空の設定ディレクトリの名前は、セッションごとに分けること（`R:\Temp\emptycfg` を
  他のセッションと共有していた時間がある）。

## TESTING.md section 41 — Windows 実機で確かめた（52507ef / 0.47.25）

Windows のセッション（`.claude/windows-role.md`）から。41 節の 14 行のうち 9 行（41.1〜41.4 /
41.7 / 41.9〜41.11 / 41.13）を TESTING-CHECKS.md でチェックした。証拠は PR 本文に 1 行ずつある。
走らせたのは `084fa53`（0.47.24）から作った release の `filer.exe` で、`52507ef` との差分に
ソースは無い。

**どう読んだか**: filer を空の隔離した設定で開き、キーは filer 自身のウィンドウにだけ
`PostMessage` で送った（前面を取り合わないので、並行して動く他のセッションの窓に
キーが入らない）。`<Tab>` のあと、`y` で各行を 1 つずつクリップボードに写し、センチネルと
置き換わったかで「その行に何が出ていたか」を文字で読んだ。行の移動は、ハーネスの
keymap で `n` に割り当てた `arrow 1`（既定の `<A-j>` と同じコマンド）を使った。41.13 だけは
本物の `<A-j>` で歩いた（`AttachThreadInput` で filer の入力状態を共有して Alt を立て、
`WM_SYSKEYDOWN` を送る）。窓が止まっていないかは、`<Tab>` のあと 20 ms ごとに
`SendMessageTimeout(WM_NULL)` を投げ、返事までの最長時間で測った。

### 見つけたもの（どれも直していない）

#### `cargo test` が実機の設定を読み、5 件落ちる

- **実測**: この機械で、変数を何も設定せずに `cargo test` を回すと `472 passed; 5 failed`。
  - `envreport::tests::it_reports_what_is_actually_there`（`src/envreport.rs:447`、
    `pdftoppm is not used yet`）
  - `ui::csv_table_frame::the_table_relays_out_when_the_window_is_resized`（`src/ui/mod.rs:4140`）
  - `ui::whole_frame::a_long_file_gets_a_strip_and_a_narrow_window_does_not`（`src/ui/mod.rs:1693`）
  - `ui::overlay::help_frame::the_wheel_turns_the_panel_and_leaves_the_list_alone`（`src/ui/overlay.rs:1705`）
  - `ui::overlay::help_frame::a_panel_owns_the_wheel_and_a_prompt_leaves_it`（`src/ui/overlay.rs:1759`）
- 空のディレクトリを `YAZI_CONFIG_HOME` / `FILER_CONFIG_HOME` / `FILER_STATE_HOME` に
  渡すと `477 passed; 0 failed`。**つまり、テストが持ち主の `%APPDATA%\yazi` と
  `%APPDATA%\filer` を読んでいる。**この機械の設定には `[[preview]]` の `pdftoppm` や、
  レイアウトを変える `[ui]` の値がある。
- **なぜ困るか**: CI のランナーには設定が無いので緑になる。**設定を持っている人の手元でだけ
  落ちる**ので、「手元で全部回してからマージする」（CLAUDE.md）という手順が、設定を持つ人に
  とっては毎回赤から始まる。そのうち「いつもの 5 件」として数えるようになると、6 件目を
  見落とす。CLAUDE.md が Linux の「既知の 4 件」で戒めているのと同じ形になる。
- **提案**: テストのハーネスが `App` を作るときは、設定を読む場所をテスト用の空の
  ディレクトリ（`util::test_dir`）に固定する。`envreport` のテストも同じ。
  環境変数を書き換えるのはテストの並列実行とぶつかるので、読む場所を引数で受け取る形が安全。
  **このセッションは、以降のテストを空の設定で回した。**

#### 41.8: 6 つのうち `macos-x64` が出たことがない

- **実測**: 最新のリリース `v0.47.10` のアセットは 5 つ（`linux-arm64` / `linux-x64` /
  `macos-arm64` / `windows-arm64` / `windows-x64`）。そのリリースを作った run
  `36327051962` は、`macos-x64` のジョブ（`runs-on: macos-13`）が
  **2026-09-27T15:12:12Z から queued のまま**で、run 全体も `queued` のまま。
  `build.yml` の直近 100 回の run でも、`macos-x64` が成功したものは 1 つも無い。
- **推測（確かめていない）**: GitHub は `macos-13` のイメージを廃止すると告知していた。
  ラベルに合うランナーが無いので、ジョブが拾われずに待ち続けている形に見える。
  `macos-15-intel` など、今もある Intel のラベルに移すのが候補。
- **食い違い**: CLAUDE.md の「成果物は 6 つ」、41.8 の「six release binaries」。
  **いまのリリースには Intel Mac 用のバイナリが無い。**
- 残りの 5 つは、`Architecture` の値が、ヘッダを直接読んだ値と一致した（PR 本文）。
  **41.8 は 6 つそろっていないのでチェックしていない。**

#### 暗号化した zip では、Preview に名前が 1 つも出ない

- 41.2 の zip（ZipCrypto / AES-256 のどちらも）で、spot の Archive 節は
  `6 files, 1 folders` と数えられるのに、Preview は
  `Cannot list: unsupported Zip archive: Password required to decrypt file` だけを出す。
- zip は、中身を暗号化してもエントリ名は平文のまま（spot が数えられるのはそのため）。
  `7z l` もパスワードなしで名前を全部出す。**見せられる一覧を、中身が読めないという理由で
  出していない。**41.3（名前ごと暗号化した 7z）なら `Cannot list: PasswordRequired` が正しい。
- **提案**: 一覧は中央ディレクトリから作り、パスワードが要るのは中身を開くときだけにする。
  行を足すなら「暗号化した zip の Preview に名前が並び、`(encrypted)` の印が付く」。

#### `Longest line` は、同じ長さの行があると**最後の**行を指す

- **どこ**: `src/spot.rs:373` の `.max_by_key(|&(_, chars)| chars)`。
  `Iterator::max_by_key` は、最大が複数あるとき**最後の**要素を返す。
- **実測**: 41.7 の 2 GB のログは、どの行も 100 文字。パネルは
  `100 chars (line 10,381)` と出した。これは、読んだ最初の 1.0 M のうち最後の完全な行。
  「最長の行」と聞いて人が期待するのは、たぶん最初の行（line 1）。
- 単体テスト（`src/spot.rs:873`）は最長の行が 1 つだけのデータしか使っていないので、
  この挙動を固定も否定もしていない。
- **提案**: 最初の行を指すなら `max_by_key(|&(i, c)| (c, Reverse(i)))` にする。
  どちらを採るにしても、同じ長さの行が並ぶケースをテストに入れる。

#### Mime 行が OOXML にも古い型を出す（軽い）

- `src/mime.rs:92-94` は `docx` / `xlsx` / `pptx` を `doc` / `xls` / `ppt` と同じ行に並べている。
  そのため 41.10 のパネルには、`.docx` で `application/msword`、`.xlsx` で
  `application/vnd.ms-excel`、`.pptx` で `application/vnd.ms-powerpoint` が出た。
- 正しい型は `application/vnd.openxmlformats-officedocument.wordprocessingml.document` など。
  長いので、パネルの幅で切れるかは 41.12 と同じく見た目の話になる。

### チェックしなかった行

- **41.5 / 41.6**: 値はどちらも期待どおりだった。
  - CRLF のファイルは `CRLF (5)`、LF のファイルは `LF (5)`。
  - UTF-16 のファイルは `UTF-16 LE` / `UTF-16 LE (FF FE)` / `CRLF (3)`。Text 節が出ており、
    バイナリ扱いにはなっていない。
  - **ただし、ファイルはメモ帳で保存していない。**同じバイト列を PowerShell で書いた。
    この機械には持ち主の分からないメモ帳の窓が開いていて、Win11 のメモ帳は開いたファイルを
    その窓のタブに足す。他のセッションの作業に手を出すことになるので、使わなかった。
    行が「Notepad で保存した」と言っているので、持ち主が保存して確かめる。
- **41.8**: 上の節。5 / 6。
- **41.12**: 見た目の行。
- **41.14**: 遅いネットワークドライブが無い。`net use` に接続は無く、FileSystem のドライブは
  `C:` と `R:`（RAM ディスク）だけ。

## TESTING.md sections 32 / 37 — Windows 実機で確かめた（ebac1fc / 0.47.26）

Windows のセッション（`.claude/windows-role.md`）から。32 節 12 行と 37 節 8 行、計 20 行のうち
13 行（32.1 / 32.3 / 32.4 / 32.6 / 32.7 / 32.8 / 32.8c / 37.1〜37.6）を TESTING-CHECKS.md で
チェックした。証拠は PR 本文に 1 行ずつある。「正しいプログラムが起動したか」は、キーの前後で
`Get-CimInstance Win32_Process` を比べて**増えたプロセスのコマンドライン**と、そのウィンドウの
タイトルで示した。起動したものは毎回閉じてから次の行へ進んだ。設定は README の `[opener]` /
`[open]` の例をそのまま写したもの（`YAZI_CONFIG_HOME` で切り替え）と、行ごとに最小にしたもの。

### サクラエディタの行指定が `-L=` になっていて、効かない（32.9）

- **場所**: `src/exec.rs:223` `"sakura" => LineArg::Flag("-L="),`。同じ綴りが
  `src/exec.rs:187` の doc コメント、テスト（`src/exec.rs:565` / `:572`）、README の
  Outline の節（`-L=N` to Sakura）にもある。
- **実測**: サクラエディタ 2.4.2.6048。アウトラインから開くと、filer が走らせた行は
  `cmd /S /C ""C:\Program Files (x86)\sakura\sakura.exe" -L=6 "R:\Temp\op\cases\md\outline.md""`
  で、キャレットは 1 行目のまま（覚えていた位置に戻るだけ）だった。filer を通さず直接
  起動して比べた結果:
  - `sakura.exe -Y=6 outline.md` → 6 行目に着地。`-Y=3` → 3 行目。
  - `sakura.exe -L=6 outline.md`、`-L=11` → **無視される**（前回の位置が復元される）。
- **直すなら**: 表の 1 行を `-Y=` にし、テスト 2 本と doc コメント、README の 1 語を揃える。
  それまでの回避策は `filer.toml` の `sakura = "-Y={line} {path}"`（`[line_args]`）の
  はずだが、**これは実機で試していない**。
- 32.9 はこのためチェックしていない。秀丸は入っていないので、半分はそもそも試せない。

### 32.9 のキーが `<C-o>` になっている

9.1 について 416 行目で報告済みのものと同じ誤り。`<C-o>` はどの keymap にも無く、実際の
経路は `<BackTab>`（アウトラインへ）→ 行を選んで `<Enter>`。この節ではその経路で試した。

### README の例では、`.docx` / `.pptx` の `<Enter>` が Excel に行く（37.2）

- README の例は `office = [excel, winword, powerpnt]` を 1 つのリストにし、
  `*.{docx,docm,doc}` と `*.{pptx,pptm,ppt}` のルールもそのリストを先頭に置いている。
  先頭項目が `start "" excel %*` なので、**`.docx` で `<Enter>` すると Excel が起動し、
  「ファイル形式または拡張子が正しくありません」のダイアログで止まる**（実測）。
- 37.2 の文言（「Office のアプリがファイルを開く」）は Word / PowerPoint を前提にしている。
  チェックは、Word と PowerPoint を先頭にした最小の設定（`start "" winword %*` /
  `start "" powerpnt %*`）で `WINWORD.EXE "…\probe.docx"`（タイトル `probe.docx - Word`）と
  `POWERPNT.EXE "…\probe.pptx"`（`probe.pptx - PowerPoint`）を見て付けた。
- **提案（直していない）**: README の例を `excel` / `word` / `powerpoint` の 3 リストに分け、
  ルールごとに合うものを先頭にする。貼って使う人がそのまま踏む。

### `cargo test` のうち 5 本が、持ち主の実際の設定を読んで落ちる

この機械で素の `cargo test` を回すと 472 通過 / 5 失敗。`YAZI_CONFIG_HOME` /
`FILER_CONFIG_HOME` / `FILER_STATE_HOME` を空のディレクトリに向けると 477 本すべて通る。
1 つずつ向けて切り分けた:

- `FILER_CONFIG_HOME`（持ち主の `filer.toml`）が効いて落ちる 4 本:
  - `ui::csv_table_frame::the_table_relays_out_when_the_window_is_resized`（`src/ui/mod.rs:4140`）
  - `ui::overlay::help_frame::the_wheel_turns_the_panel_and_leaves_the_list_alone`（`src/ui/overlay.rs:1705`）
  - `ui::overlay::help_frame::a_panel_owns_the_wheel_and_a_prompt_leaves_it`（`src/ui/overlay.rs:1759`）
  - `ui::whole_frame::a_long_file_gets_a_strip_and_a_narrow_window_does_not`
    （`src/ui/mod.rs:1693`、`a pane too narrow for the map draws none of it`）
- `YAZI_CONFIG_HOME`（持ち主の `yazi.toml`）が効いて落ちる 1 本:
  - `envreport::tests::it_reports_what_is_actually_there`（`src/envreport.rs:447`、
    `pdftoppm is not used yet:` のあとに持ち主の環境の Tools 節が出る）
- CI のランナーには設定が無いので緑になる。**設定を持っている人の機械でだけ赤になる**
  ので、手元で回した人は「既知の失敗」として数え始めかねない（CLAUDE.md が戒めている形）。
  テストのほうで設定ディレクトリを `util::test_dir` に向けるのが筋だと思う。

### 実機に残るもの・この機械の事情

- **秀丸と IrfanView が入っていない。**そのため 32.2（先頭項目が秀丸）、32.5（「上の
  それぞれ」に秀丸を含む）、32.8a / 32.8b（秀丸の半分）、32.9、37.7 は持ち主の行。
  - 32.2 では、先頭の秀丸が無いので `Open failed: exit code 1 — "C:\Program Files\Hidemaru\Hidemaru.exe" "…\probe32.txt"`
    のトーストになった（先頭項目が選ばれたこと自体は読める）。
  - サクラの半分は確かめた: 先頭項目の `<Enter>` からも `<S-Enter>` からも
    `sakura.exe "…\probe32.txt"` が起動し、タイトルは `probe32.txt - サクラエディタ`。
  - 32.5 は Excel（`space book.xlsx - Excel`）、サクラ（`space name.txt - サクラエディタ`）、
    Edge（`space name.pdf`）で 1 引数を確かめた。
- **37.8** は Edge / サクラ / VS Code / Neovim の 4 項目を選んで、それぞれ表示どおりのものが
  起動したことを見た。Chrome と既定アプリ（この機械では `.pdf` → Chrome）は、持ち主が
  使用中の Chrome に混ざるので押していない。秀丸は無い。
- **37.3 の `.csv`**: この機械の `.csv` の関連付けが壊れている（`UserChoiceLatest` の
  ProgId が `Applications\sakura.exe` で、Windows が受け付けない）。filer を通さず
  `cmd` から `start "" probe.csv` しても「アプリの選択」ダイアログになるので filer の
  問題ではない。37.3 のチェックは `.txt`（関連付け先のサクラが `"…\space name.txt"` を
  1 引数で受け取った）で付けた。
- **32.8c の文言**: 日本語版 Windows では期待どおり `exit code 1` になった
  （`Open failed: exit code 1 — "C:\Program Files (x86)\sakura\sakurra.exe" "…\probe32.txt"`）。

## TESTING.md section 46 — Windows 実機で確かめた（71f09e1 / 0.47.29）

Windows のセッション（`.claude/windows-role.md`）から。46 節で残っていた 8 行のうち 7 行（46.2 /
46.4 / 46.7〜46.11）を TESTING-CHECKS.md でチェックした。証拠は PR 本文に 1 行ずつある。
走らせたのは `71f09e1` から作った release の `filer.exe`、git は 2.52.0（`C:\Program Files\Git`）。
**46.6 はバグのためチェックしていない**（下）。

**どう読んだか**: 41 節と同じハーネス（キーは filer の窓にだけ `PostMessage`、`y` で行を 1 つずつ
クリップボードに写してセンチネルと比べる、行の移動は隔離した keymap で `n` に割り当てた
`arrow 1`）。ファイルの選択は `f` のフィルタで行った。fixtures の repo は 1 コミットしか無いので、
作業ツリーの状態（`D` / `M` / `A` / `??`）を壊さないよう pathspec 付きで 4 コミット足した:

| hash | author | date | subject | 触ったファイル |
| --- | --- | --- | --- | --- |
| `42e9175` | Fixtures | 2026-09-29 07:39 | fixtures | clean.txt ほか（元からの 1 つ） |
| `5513305` | Alice | 2026-09-20 10:00 | sub: add second | sub/second.txt |
| `0450b3f` | Bob | 2026-09-21 11:00 | history: first | history.txt |
| `14a47bb` | Bob | 2026-09-22 12:30 | history: second | history.txt |
| `2e0a020` | 山田 太郎 | 2026-09-23 13:45 | 日本語の件名 🎉 絵文字つき | jp.txt |

日付を過去にずらしてあるので、repo の最新（`2e0a020`）と「そのパスを触った最新」が必ず違う。
期待値は毎回 `git log -1 --format="%h %an %ad %s" -- <path>` で取った。ハーネスの出力と
スクリーンショット、スクリプト（`spot46.ps1` / `winwatch.ps1`）は `C:\dev\filer-evidence\46` に置いた。

### 見つけたもの（どれも直していない）

#### 46.6: ディレクトリには、そこを触った最新ではなく repo 全体の最新が出る

- **実測**: fixtures の `sub` ディレクトリで `<Tab>`。Git 節は
  `2e0a020  2026-09-23 13:45` / `日本語の件名 🎉 絵文字つき` / `山田 太郎` / Commits `5`。
- **期待**: `git log -1 -- sub` は `5513305 Alice … sub: add second`、`sub` を触ったコミットは 2 つ。
  出たのは `sub` の外（jp.txt）しか触っていないコミットで、数は repo 全体の 5。
- **原因**: `src/fs/git.rs:192` の `if let Some(name) = name.filter(|_| !path.is_dir())`。
  ディレクトリのときは pathspec を付けずに `git -C <dir> log` を走らせる。`-C` は作業場所を
  変えるだけで履歴を絞らないので、**どのディレクトリでも repo 全体の履歴になる**。
  - 実機で確かめた: `git -C sub log -1` → `2e0a020`、`git -C sub log -1 -- .` → `5513305`。
  - repo の直下（worktree の根）だけは、たまたま正しい。
- **提案**: ディレクトリのときも pathspec に `.` を渡す（`git -C <dir> log … -- .`）。
  テストは `the_last_commit_on_a_path_is_read_back_whole`（ファイルだけ）の隣に、
  「サブディレクトリの外だけを触る新しいコミットがあっても、サブディレクトリはそれを返さない」
  を足すと固定できる。
- **46.5 について**: 46.5（`50+`）は前のセッションでチェック済み。行の文言はファイルだが、
  もしディレクトリで確かめていたなら、このバグ（repo 全体を数える）で `50+` になっていた
  可能性がある。このセッションでは 46.5 を確かめ直していない。

#### CP932 の化けは、この経路では起きなかった（仮説を否定、バグではない）

- 25 節の CP932 の化けを受けて、`encoding` ヘッダ無しで件名を CP932 の生バイトにした
  コミットを作った（`R:\Temp\sjis46b`、その上に UTF-8 の件名のコミットを 1 つ）。
- git 2.52 は、そのバイト列を Latin-1 とみなして UTF-8 に変換して出す（`c2 8c c3 82 …`）。
  **出力は常に正しい UTF-8 になる**ので、`String::from_utf8(...).ok()` が `None` になって
  Git 節ごと消える、ということは起きない。実際パネルには `f2ebc35` / `new subject in UTF-8` /
  `Ada` / Commits `2` が出た。
- 古いコミットの件名が CP932 のままなら、そこが表示されるときは Latin-1 として化ける
  （git 自身の `git log` と同じ化け方）。filer 側で直せる話ではないので 1 行だけ残す。

#### 46.10 の絵文字は単色で描かれる（見た目、持ち主に）

- `Subject` の行をクリップボードに写した値は `日本語の件名 🎉 絵文字つき` とバイト単位で一致、
  `Author` は `山田 太郎`。**文字としては無傷**なので 46.10 はチェックした。
- スクリーンショット（`jp-open.png`）では、日本語は正しく描かれ、🎉 は豆腐でも化けでもなく
  egui の単色のグリフで出ている。カラーで出ないことを許すかは見た目の判断なので持ち主に。

### チェックの方法（46.8 / 46.9 / 46.11）

- **46.8**: repo でない `R:\Temp\nonrepo46`（`git log` は 31 ms で exit 128）。ファイルでも
  ディレクトリでも File / Preview / Text（またはディレクトリの節）だけで Git 節は無い。
  `<Tab>` のあと 2 秒間、20 ms ごとに `SendMessageTimeout(WM_NULL)` を投げ、最長の返事は
  5 ms（ファイル）/ 2 ms（ディレクトリ）。
- **46.9**: ハーネスが `PATH` から `git.exe` を含むディレクトリをすべて外し、`git` が
  見つからないことを確かめてから filer を起動した。clean.txt と history.txt の行は、git が
  あるときの行から Git 節の行だけを抜いたものと一致。2 秒後のスクリーンショットにエラーの
  トーストは無い。UI の最長の返事は 5 ms / 1 ms。
- **46.11**: `SetWinEventHook(EVENT_OBJECT_SHOW)` で、表示されたウィンドウをすべて記録し、
  あわせて git / conhost / OpenConsole / WindowsTerminal の新しいプロセスを 100 ms ごとに拾った。
  - **陽性対照**: 普通に `Start-Process git --version` すると、`WindowsTerminal` の
    `CASCADIA_HOSTING_WINDOW_CLASS` と `git` の `PseudoConsoleWindow` の SHOW が記録された
    （`winwatch-control.log`）。つまりこの方法は、コンソールの窓が出れば捕まえる。
  - filer で clean.txt を開いた 25 秒間（`clean-watch.log`）: git のプロセスが 3 つ、conhost が
    2 つ起動した（Git 節が出たので git は確かに走っている）が、SHOW は filer 自身の窓と
    無関係な窓だけで、コンソールの窓は 1 つも出なかった。

## TESTING.md section 1 — Windows 実機で確かめた（0f0d36b / 0.47.33）

Windows のセッション（`.claude/windows-role.md`）から。ターミナルペインを実機の
release ビルド（0.47.33、`src/` は 0f0d36b と同じ）で動かした。シェルは pwsh 7.6.6。
設定は `FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` / `FILER_STATE_HOME` で `R:\Temp\t1cfg` に
隔離し、`[term] shell` には OSC 7 を報告するプロンプトを入れた pwsh を指定した。
画面は 150%（dpi 144）。

**先に DPI に決着を付けた**（下の 1 件目）。窓は正しい大きさで、何も切れていない。
そのうえで 1 節の 30 行のうち 16 行をチェックし、25.8a / 25.8b もチェックした。
証拠は PR 本文に 1 行ずつ書き、生の出力とキャプチャは `C:\dev\filer-evidence\1\` に置いた
（`R:` は RAM ディスクなので、そこだけには置いていない）。

**いちばん重いのは 2 件目: ペインの中で `<C-c>` を押すとフィラーが終了し、シェルも道連れになる。**

### DPI: 窓は正しい。25.8a / 25.8b をチェックした

- `filer env` の Window 行（v0.47.33）:
  - 既定の大きさ: `2040 x 1290 px (1360 x 860 pt @ 1.5)`（`env-plain.txt`）
  - 最大化: `3840 x 2126 px (2560 x 1417 pt @ 1.5)`（`env-max.txt`）
- 同じ窓を、**スレッドを per-monitor v2 にしてから**（`SetThreadDpiAwarenessContext(-4)`、
  `dpi.ps1`）`GetClientRect` で測ると、2040 x 1290 / 3840 x 2126、`GetDpiForWindow` は 144。
  Window 行と 1 ピクセルも違わない。
- 画面そのものを `BitBlt` で写したキャプチャ（`dpi-plain.png` / `dpi-max.png`）では、
  ステータスバーの右端の `1/15` と下端の `15 items` が読める。**右端も下端も切れていない。**
- **朝の「切れている」は測り方の問題だった。**以前の値 `1360 x 860 px` / `2560 x 1417 px` は、
  DPI を認識していないプロセス（PowerShell 既定）から測ったので、OS が 96 dpi 相当に
  仮想化した値だった。ちょうど pt の値と同じになっている。

#### これからの測り方（人への提案）

- **`PrintWindow` のキャプチャは寸法の証拠にならない。**論理座標のビットマップが返る
  ことがあり、実際より小さく写って「切れている」ように見える。今朝の 46 節のキャプチャにも
  同じ切れ方が出ていたが、**46 節の判定は文字列（clean.txt / history.txt の行）で行って
  いるので影響しない。**
- **`GetClientRect` の値は、測る側のプロセスの DPI 認識で変わる。**外から測った数字は、
  測った側が per-monitor v2 だと示せない限り信用できない。
- **この種の問いには、今後 `filer env` の Window 行で答える。**filer 自身が知っている
  ピクセルと pt と倍率が並ぶので、測る側の事情が入らない。25.8a の文言がすでにそう言っている。

### `<C-c>` をペインで押すとフィラーが終了する（重い）

- **再現**: ペインにフォーカス（クリックか `<C-t>`）、行に `echo marker-cc` と打ってある
  状態で `<C-c>`。
- **実測**: フィラー（pid 26216）とシェル（pid 57920、親 = フィラー）が**両方とも消えた**。
  `bookmarks.toml` / `history.txt` が同じ時刻に書き出されていて、異常終了ではなく通常の
  終了処理が走っている。最初は 15:26:54 に偶然踏み、フィラー 57384 とシェル 56900 を失った。
- **期待**: シェルに `^C`（0x03）が届き、行が捨てられる／走っているコマンドが止まる。
  Ctrl+C はシェルの割り込みとして一番よく押されるキーで、**押した人は必ずシェルを相手に
  しているつもり**でいる。
- **原因**: egui-winit は `Ctrl+C` をキーではなく `egui::Event::Copy` に変える。
  `src/main.rs:537` はそれを `app.feed_key(Key::ctrl('c'))` に戻すが、ガードが
  `matches!(app.overlay, Overlay::None)` だけで **`term_focus` を見ていない**。
  `feed_key` は一覧の keymap を引くので、`[mgr]` の `<C-c>` = `close`（「最後のタブなら終了」、
  `keymap.toml:18`）が走る。ペインのキー経路（`main.rs:640` の `Overlay::None if app.term_focus`）
  には届かない。すぐ下の `Paste` は `if app.term_focus` の腕を持っているので、Copy / Cut に
  だけそれが無い。
- **同じ形のもの（未確認）**: `Event::Cut` → `<C-x>`（`main.rs:540`）。`[mgr]` の `<C-x>` が
  何をするかによっては同じ種類の事故になる。実機では押していない。
- **TESTING.md に行が無い。**1 節は `<C-c>` を一度も押さないので、この事故は手順どおりに
  やっていると見つからない。
- **提案（直していない）**: Copy / Cut の腕に `term_focus` の場合を足し、ペインに `0x03` /
  `0x18` を送る（選択があるときのコピーをどうするかは設計判断）。あわせて 1 節に
  「ペインで `<C-c>` → シェルに割り込みが届き、フィラーは終わらない」の行を足す。

### `quote()` の `'\''` は PowerShell では読めない

- **再現**: `awkward names` で `quote'in-name.txt` を選んで `<A-t>`。
- **実測**: 行に `'R:\Temp\filer-fixtures\awkward names\quote'\''in-name.txt'` が打ち込まれた。
  そのまま Enter すると pwsh は `>>`（続きの入力待ち）になる。同じ文字列を `pwsh -c` に
  渡すと ParserError「The string is missing the terminator: '.」。
- **期待**: 1 語として届く。PowerShell の単引用符の中で `'` を書くには `''`（2 つ重ねる）。
  `\` は PowerShell ではエスケープではないので、`'\''` は「閉じる・`\`・空の開き」になり、
  最後の `'` が閉じられない。
- **原因**: `src/terminal.rs:549` の `quote()` が POSIX の形だけを出す。doc コメント
  （544–548 行）の「PowerShell reads them the same way; … which both understand」は
  **PowerShell については誤り**。テスト `terminal::tests::a_path_reaches_the_shell_as_one_word`
  （1214 行）は `quote("it's") == r"'it'\''s'"` を固定しているので、この誤りを守る側にいる。
- **1.17 との関係**: 1.17 の手順は「2 つ選ぶ」で `'` 入りの名前を指定していないので、
  `a file with spaces.txt` と `name.txt` でやり直してチェックした。`'` 入りの名前が
  壊れるのはこの別件として報告する。
- **提案（直していない）**: シェルの種類で引用を分ける（PowerShell / pwsh なら `'` → `''`、
  cmd なら `"…"`）。`[term] shell` の値か、起動した実行ファイル名で判定できる。

### `'` 入りのディレクトリへ追従すると、pwsh が `>>` で固まる

- **再現**: 一覧で `it's here` に入る（ペインが開いていて、`follow()` が `cd` を打つ場面）。
- **実測**: ペインに `cd 'R:\Temp\filer-fixtures\it'\''s here'` が打たれ、pwsh は `>>` で
  止まった。一覧で親に戻ると、次の自動の `cd R:\Temp\filer-fixtures` が**その続きとして
  飲み込まれ**、`>>` のまま（`bugs-raw.txt`、`t1-quote-cd.png`）。
- **原因**: 上と同じ `quote()`。`follow()`（`terminal.rs:507`）がそれを使う。
- **影響**: 以後、一覧を動かすたびに `cd` が `>>` の中へ積まれていく。`<Esc>` で抜けられるが、
  **`<C-c>` で抜けようとするとフィラーが終わる**（上の件）。2 件が重なると、普通の操作
  だけで作業中のフィラーを失う。
- **提案（直していない）**: 上の `quote()` の修正で直る。

### 新しい `<C-S-f>` の検索が、前の検索の位置から続く（1.9f をチェックしていない）

- **再現**（`t1-search.txt` の Seq B）: `L1`〜`L500` を出し、`needle` が L10 と L495 の行にある
  状態。`<C-S-f>` `needle` Enter、`<C-S-n>`（L10 へ）、`<S-End>`（L491..L500 が見えていて、
  L495 が画面にある）、そこで**新しく** `<C-S-f>` `needle` Enter。
- **実測**: L1..L11 へ飛んだ（L10 の次、つまりコマンド行の `needle`）。画面に出ている L495 を
  飛ばした。
- **期待**（1.9f）: 画面に出ているものが先に見つかる。
- **原因**: `Terminal::search`（`src/terminal.rs:434`）は起点を
  `self.found.unwrap_or_else(|| search_origin(..))`（439 行）で決める。`found` を消すのは
  `end_search()`（466 行）だけで、それを呼ぶのは `term_find`（`app.rs:4038`）の
  「回り込んだ」経路だけ。検索欄の決定（`app.rs:3448`）は `term_find(&text, true)` を
  `end_search` なしで呼ぶので、**新しい検索が前の `found` を起点にする**。
- **なぜ 1.9f を付けないか**: セッションで最初の検索なら成り立つ（Seq A の最初の `find` は
  L495 に留まった）が、行は「最初の検索なら」とは言っていない。
- **提案（直していない）**: 検索欄の決定で `end_search()` を呼んでから `term_find` する。

### 向きを変えると、今の一致をもう一度見つける（1.9g をチェックしていない）

- **実測**（`t1-search.txt` の Seq A、きれいな状態から）:

  | 押したもの | 見えている範囲 |
  | --- | --- |
  | `<C-S-f>` needle | L491..L500（L495） |
  | `<C-S-n>` #1 | L10..L21（L10） |
  | `<C-S-b>` #1 | **L10..L21（動かない）** |
  | `<C-S-b>` #2 | L491..L500（L495） |
  | `<C-S-n>` #2 | **L491..L500（動かない）** |
  | `<C-S-n>` #3 | L10..L21 |
  | `<C-S-n>` #4 | L1..L11（コマンド行） |
  | `<C-S-n>` #5 | L491..L500（回り込み） |

- **期待**（1.9g）: `<C-S-b>` で下へ戻る。1 回で。
- **原因**: 見つけたあと `found` を「戻る向きならその一致の 1 つ前、進む向きなら一致の
  末尾の次」に置く（`terminal.rs:458–461`）。**置いた向きと逆に探すと、起点から最初に
  当たるのが今の一致そのもの**になる。
- **提案（直していない）**: `found` を一致の範囲そのもので持ち、探す向きに応じて
  範囲の前か後ろから始める。
- **1.13 はチェックした**: `<C-S-n>` だけを続けて押す手順なので、上の表の #3〜#5 のとおり
  次々に移動し、末尾で回り込む。

### 1.18 の文言（提案）

- 一覧側のディレクトリを変えると、シェルがまだそこにいなければ `cd` が**打たれる**
  （README の設計どおり。`follow()`、`terminal.rs:507`）。行の「余計な `cd` が打ち込まれない」
  は、**要らない `cd`（シェルが OSC 7 ですでに報告した場所への `cd`、`<A-Up>` のあと一覧が
  シェルを追いかけ返す `cd`）が無い**という意味に読んでチェックした。実測は `<A-Up>` のあとも
  `<C-t>` のあとも、ペインの `> cd ` の数が 2 のまま。
- 文言を「**要らない** `cd`（シェルがすでにいる場所への `cd`）が打ち込まれない」にすると、
  `cd` が 1 つでも出たら失敗と読む人が迷わない。

### 見た目の行（チェックしていない）

1.2 / 1.3 / 1.3a / 1.4 / 1.8 / 1.9c / 1.9d / 1.9e / 1.9h / 1.11 / 1.12 / 1.14。期待が
「揃っている」「塗られる」「強調される」「滑らか」「描かれる」「上に開く」など、目で見て
判断するもの。1.9c / 1.9h / 1.14 は操作の一部（飛ぶ先・トースト・`<Esc>` で戻る）は
読めるが、行の核が見た目なので付けていない。

### 1.9 の埋め方

1.9 は「`many\` で `dir`」だが、`dir` の出力は行が似ていて、どこまで動いたかを文字で
判定できない。代わりに `1..500 | % { "L$_" }` で番号付きの行を出し、見えている範囲を
ドラッグのコピーで読んだ。行の意図（画面を埋めて `<S-PageUp>` で本文が動くか）は同じ。

### ハーネスについて（次に 1 節をやる人へ）

- **ドラッグは `PostMessage` だけでは効かない。**実カーソルが窓の外にあると、winit の
  `TrackMouseEvent` が `WM_MOUSEMOVE` のたびに `WM_MOUSELEAVE` を返し、egui がポインタを
  見失う。`SetCursorPos` で実カーソルも一緒に動かす（`drag2.ps1`）。ボタンの
  メッセージはフィラーにだけ送るので、他の窓をクリックすることはない。
- **ペインをクリックするとキーがペインに行く**（`term_focus = true`）。ドラッグで読んだ
  あと一覧を操作するなら `<C-t>` で戻すこと。
- **PSReadLine の予測（灰色の文字）もコピーされる。**`q` だけ打った行が `qwinsta` として
  読めた。行末の判定に使うときは予測を切るか、打った文字の前方一致で見ること。
- **`<C-S-f>` の入力欄には前の語が入っている。**そのまま打つと `needleneedle` になる。
  Backspace で空にしてから打つ。
- **`filer env` はパイプに出ない。**release は GUI サブシステムで `CONOUT$` に書くので、
  コンソールのバッファを読む（`envdump.ps1`）。
- スクリプトはすべて `C:\dev\filer-evidence\1\` と `scripts\` に置いた。

## TESTING.md section 1 — v0.47.34 の答え合わせ（ac1a34c / 0.47.36）

Windows のセッション（`.claude/windows-role.md`）から。上の節（0.47.33）で報告した
`<C-c>` と `quote()` の 3 件を v0.47.34 が直したので、実機の release ビルド
（0.47.36、ac1a34c。`src/terminal.rs` / `main.rs` / `app.rs` は v0.48.0 の main と同じ）で
1.19 / 1.20 / 1.21 を確かめた。設定は `R:\Temp\t1bcfg` に隔離、画面は 150%（dpi 144）。
生の出力・スクリプト・キャプチャは `C:\dev\filer-evidence\1b\` にある。

**3 行ともチェックした。上の節の 3 件（`<C-c>` で終了、PowerShell の `'\''`、`it's here` で `>>`）は
4 シェルすべてで再現しない。**ただし bash で**別の**引用の穴が見つかった（下の 1 件目）。

| シェル（`[term] shell`） | 1.20: `<A-t>` で行に入った文字列 → Enter の結果 | 1.21: filer が打った `cd` → 着いた場所 | 1.19: `<C-c>` |
| --- | --- | --- | --- |
| `pwsh`（7.6.6） | `'R:\…\awkward names\quote''in-name.txt'` → `n=1`、`exists=True` | `cd 'R:\Temp\filer-fixtures\it''s here'` → `PWD=R:\Temp\filer-fixtures\it's here` | `sleep 30` が 3.0 秒で `Stopped` |
| `powershell`（5.1） | 同じ文字列 → `n=1`、`exists=True` | 同じ `cd` → 同じ `PWD` | `sleep 30` の後の `'NOT-INTERRUPTED'` が出ずにプロンプト |
| Git Bash（`C:/Program Files/Git/bin/bash.exe`） | `'R:\…\awkward names\quote'\''in-name.txt'` → `n=1`、`[…quote'in-name.txt]`、`ls -d` がファイルを見つけた | `cd 'R:\Temp\filer-fixtures\it'\''s here'` → `PWD=/r/Temp/filer-fixtures/it's here` | `sleep 30; echo NOT-INTERRUPTED` が止まり、後続も出ない |
| `cmd` | `"R:\…\awkward names\quote'in-name.txt"` → `dir /b` が `quote'in-name.txt` | `cd "R:\Temp\filer-fixtures\it's here"` → `cd` が `R:\Temp\filer-fixtures\it's here` | `ping -n 30` が 4 回で `Ctrl+C`、プロンプトが戻る |

- どのシェルでも `>>`（や bash の `>`）の継続プロンプトにはならなかった。`<A-t>` の文字列は
  **Enter の前に**ペインをドラッグで読んだもの。
- 1.19 は依頼どおり 2 つに分けて見た。**(a) シェルに 0x03 が届いた**: pwsh で
  `$t0=Get-Date; try { sleep 30; 'NOT-INTERRUPTED' } finally { $t1=Get-Date }` の 2.5 秒後に `<C-c>`、
  `SLEPT=3.0s STATUS=Stopped`（`s19c.txt`）。ネイティブの子（`ping -n 30`）も統計を出して
  `Ctrl+C` で止まった（`s19.txt`）。**(b) filer は開いたまま、タブも減っていない**: filer の pid
  （59520）とシェルの pid が `<C-c>` の前後で同じ。タブ 2 を開いた状態で押し、タイトルは
  `Filer: …\awkward names`（タブ 2）のまま。`[` で `Filer: R:\Temp\filer-fixtures`、`]` で
  `…\awkward names` に戻れて、2 枚とも残っている。4 シェルの通し（`s20-*.txt`）でも、
  最後の pid は起動したときの pid と同じ。
- **`<C-x>`（`Event::Cut`）は押していない。**

### Git Bash で、`'` の無いパスの `\` が消える（`cd` が効かない）

- **再現**: `[term] shell = "C:/Program Files/Git/bin/bash.exe"`（`args = ["--noprofile", "--norc", "-i"]`）。
  ペインを開いたまま、一覧で親ディレクトリへ移動する。
- **実測**: ペインに `cd R:\Temp\filer-fixtures` が打たれ、
  `bash: cd: R:Tempfiler-fixtures: No such file or directory`。シェルは `it's here` に残り、
  **一覧とペインがずれたまま**になる（`s20-bash.txt`、`shots\s20-bash-cd.png`）。
  `'` を含む `it's here` へは入れたのに、そこから出られない。
- **期待**: filer の `cd` が着く（1.21 と同じ「filer 自身が打つパス」）。
- **原因**: `src/terminal.rs:621` の `quote()` は、全部の文字が英数字か `_-./:\` なら
  **引用せずに返す**。`\` は PowerShell と cmd では普通の文字だが、POSIX シェルでは
  エスケープなので、引用の外の `R:\Temp` は `R:Temp` になる。テスト
  `terminal.rs:1303` が `quote(r"C:\dev\filer", how) == r"C:\dev\filer"` を **`Posix` を含む
  3 つすべてで**固定しているので、この誤りを守る側にいる。
- **影響**: Windows で bash を使うと、**普通のパスでは `follow()` の `cd` が毎回失敗する。**
  `quote()` を共有する `<A-t>` も、`notes.md` のような普通の名前で `R:Tempfiler-fixturesnotes.md`
  を渡すはず（**未確認**: 同じ関数からの推定で、押してはいない）。引用されたパス
  （`'R:\…'`）は Git Bash が `\` のまま受け取って正しく着く。
- **提案（直していない）**: `Quoting::Posix` のときは `\` を安全な文字から外す（引用させる）。
  あるいは Posix では常に `'…'` で囲む。
- **TESTING.md に行が無い。**1.20 / 1.21 は `'` のある名前しか指定していないので、bash で
  `'` の**無い**パスを通す行が無い。「bash で普通のディレクトリへ一覧を移動 → `cd` が着く」を
  1.21 の隣に足すことを提案する。

### ハーネスについて

- **`[term] shell = "bash"` は WSL の bash に行く**（`where bash` では Git の `usr\bin\bash.exe` と
  `WindowsApps\bash.exe` が並ぶ）。Git Bash を試すならフルパスを書くこと。
- **`<A-t>` はキーをペインに移す**（`term_focus = true`）。そのあと一覧を操作するなら
  `<C-t>` で戻す。これを忘れて、一覧に送ったつもりの `gg` `cc` `j` がシェルの行に積もった。
- **PowerShell 5.1 には `LocationChangedAction` が無い。**OSC 7 のフックがエラーになるので、
  5.1 では filer が毎回 `cd` を打つ（今回の判定には影響しない）。
- **1 回だけ `<A-t>` が何も入れなかった**（最初の pwsh の通し、`<C-t>` の 500 ms 後に `<A-t>`）。
  同じ間隔で 4 回繰り返した `probe5.txt` では 4 回とも入り、再現しなかったので不具合としては
  報告しない。スクリプトは `<A-t>` の前に一覧のホバーをコピーで読むようにした（その結果も
  証拠になる）。
- `filer.exe --version` はパイプに何も出さない（GUI サブシステム、上の節の `filer env` と同じ）。
  版は exe のファイルバージョン `0.47.36` とビルド時刻（HEAD ac1a34c より後）で確かめた。

## TESTING.md section 1 — v0.48.1 と最大化の答え合わせ（51e6fb0 / 0.49.1）

Windows のセッション（`.claude/windows-role.md`）から。担当は 1.22 / 1.23 / 1.27 / 1.28 / 1.30。
実機の release ビルド（0.49.1、51e6fb0）に `scripts\fetch-conpty.ps1` で同梱の ConPTY
（1.24.260710001）を置いて動かした。ペインの親子は `OpenConsole.exe` → シェルで、同梱の
ConPTY が使われている。設定は `R:\Temp\t1bcfg` に隔離、画面は 150%（dpi 144）、窓は
クライアント 2040 x 1290。生の出力・スクリプト・キャプチャは `C:\dev\filer-evidence\1d\` にある。

**1.22 / 1.23 / 1.27 / 1.28 にチェックを付けた。1.30 は測った値だけで、チェックは付けていない。**
新しい不具合は見つからなかった。

| 行 | やったこと | 読んだもの |
| --- | --- | --- |
| 1.22 | `[term] shell = "C:/Program Files/Git/bin/bash.exe"`（`--noprofile --norc -i`）。fixtures の直下でペインを開き、`<C-t>` で一覧に戻して `l` で `repo` へ | filer が打った行は `cd 'R:\Temp\filer-fixtures\repo'`、エラーは出なかった。続けて `pwd > /r/Temp/t1d/w122.txt` を実行 → **`/r/Temp/filer-fixtures/repo`**（`w122.txt`、`s22.txt`） |
| 1.23 | 同じシェルで `h` で fixtures の直下へ戻り、`notes.md` の上で `f() { echo "n=$#"; printf "[%s]\n" "$@" \| tee …; ls -l -- "$@"; }; f ` と打ってから `<A-t>` | **Enter の前に**ドラッグで読んだ行の末尾が `'R:\Temp\filer-fixtures\notes.md'`。Enter で `n=1`、`[R:\Temp\filer-fixtures\notes.md]`、`ls -l` がファイルを見つけた（`w123.txt`、`s22.txt`） |
| 1.28 | pwsh のペインを開いて `<C-t>` で一覧に戻し、**一覧側から** `<C-S-Enter>`、そのまま `echo W128 > R:\Temp\w128.txt` と Enter | `R:\Temp\w128.txt` ができて中身は `W128`。打鍵がペインのシェルに届いた証拠 |
| 1.27 | 最大化したまま `lazygit`、プロセスに `lazygit.exe` がいる状態でペインの中から `<C-S-Enter>`、`q` で抜ける | 行数は下の表。`<C-S-Enter>` の**直前と直後**の両方で `lazygit.exe` が pwsh の子にいた。つまり TUI がキーを持っている間に押している |
| 1.30 | もう一度最大化して `<C-S-t>`、`<C-t>` で開き直す | `<C-S-t>` の後、filer の子プロセスは空（シェルが終わった）。開き直した後は 12 行 |

`$Host.UI.RawUI.WindowSize.Height` をペインの pwsh からファイルに書いた値:

| 時点 | 行数 |
| --- | --- |
| 通常（開いた直後） | 12 |
| 一覧側から最大化（1.28） | 35 |
| 最大化のまま lazygit → ペインから `<C-S-Enter>` → `q` の後（1.27） | **12** |
| ペインから再び最大化 | 35 |
| `<C-S-t>` → `<C-t>` で開き直した後（1.30） | **12** |

- 1.27 は 3 つ目が 1 つ目と同じなので、lazygit が動いている間の `<C-S-Enter>` で 3 分の 1 に
  戻っている。`q` のほうに窓を戻す働きは無いので、戻したのはこの 1 回の押下。
- 1.28 は「隠れた一覧**ではなく**」の側を直接には見ていない。見たのは、打鍵がペインに**届いた**ことだけ。
  キーの行き先は `term_focus` の 1 つだけで決まるので、両方に届くことは無いと読んだ。
- **1.30 はチェックしていない。**「最大化が残らない」の半分は、開き直した後の 12 行
  （最大化の 35 行ではない）で示せた。**「一覧が全高で描かれる」の半分は見た目**なので
  オーナーの判断に残す。`<C-S-t>` の直後の画面は `shots\f-closed.png`。
- 1.27 の lazygit 実行中の画面は `shots\c-lazygit-max.png`（最大化中）と
  `shots\d-lazygit-after-cse.png`（`<C-S-Enter>` の後）。見た目の判定はしていない。

### `a_send_does_not_disturb_what_is_yanked` が 1 回だけ落ちた

- 準備の 1 回目の `cargo test`（TEMP = `R:\Temp`）で
  `app::send_pane_and_the_register::a_send_does_not_disturb_what_is_yanked` が
  `src\app.rs:6134:69` で panic した（`Entry::from_path(sent.clone()).unwrap()`）。
  すぐ回し直すと 492 件すべて通り、再現しなかった。
- 同じ時刻に `cargo build --release` と `fetch-conpty.ps1` を並べて走らせていた。
  この機械では他の Claude Code のセッションも複数動いていた。
- このテストは一時ディレクトリを `std::env::temp_dir().join("filer-send-pane")` と**手で組んでいる**。
  CLAUDE.md の「`util::test_dir("ラベル")` を使う」に反していて、同名のディレクトリは
  プロセスを跨いで共有される。ほかに `filer-spot-follow`（`app.rs:5864`）、
  `filer-follow-msg`（`app.rs:6180`）、`filer-parent-click`（`ui/mod.rs:1220`）も同じ形。
  原因だとは確かめていない。**直していない**（QA の書ける範囲ではあるが、このセッションの担当外）。

### ハーネスについて

- **1 回目の通しで filer が起動直後に消えた。**`Geo` が dpi 144 を返した（窓はあった）直後、
  まだ何もキーを送っていない時点で、窓が見つからなくなった。その間にしたのは
  `& filer.exe --version` だけ。同じ手順を切り出した `vcheck.ps1` では消えず
  （`vcheck.txt`）、2 回目の通しも最後まで同じ pid だったので、**不具合としては報告しない。**
  なお `& filer.exe --version` をパイプで受けると何も出ない（原因は調べていない）。
  版は 51e6fb0 の `Cargo.toml`（0.49.1）で見ている。
- 高さを測るヘルパーを `H` と名付けたら、PowerShell の既定エイリアス（`Get-History`）に負けた。
  関数よりエイリアスが先に解決される。`Hgt` に変えた。

## TESTING.md section 13 / 15 — ジャンクション・ハードリンク・拡大縮小（281e433 / 0.49.1）

Windows のセッション（`.claude/windows-role.md`）から。担当は 13 の残り（13.7 / 13.16）と 15 の全 7 行。
実機の release ビルド（`C:\dev\filer-win13\target\release\filer.exe`、51e6fb0 でビルド、0.49.1）を
動かした。設定は `R:\Temp\w13cfg` に隔離（`[spot]` に `n` = `arrow 1` だけ足してある。パネルの
カーソルを素のキーで動かすため）、画面は 150%、キーボードは JIS。キーは filer の窓に
PostMessage で送り、Ctrl / Shift は filer の入力スレッドに attach してキーボード状態に立てた。
spot パネルの行は `y` でクリップボードに写して読んだ。ハーネスとキャプチャ、読んだ行の
写しは `C:\dev\filer-evidence\13\` にある（`lib.ps1`、`13-spot-readouts.txt`、`15.5-spot.txt`、`*.png`）。

**13.7 / 13.16 / 15.1 / 15.2 / 15.3 / 15.5 / 15.7 にチェックを付けた。15.4 は測った値だけで
チェックは付けていない（「滑らかに」が見た目）。15.6 は見た目なので触れていない。**
不具合を 1 件見つけた（下の「RAM ディスク上のジャンクションで `Resolves` が `no`」）。

| 行 | やったこと | 読んだもの |
| --- | --- | --- |
| 13.7 | `R:\Temp\w13` に `mklink /J j-target target-dir`。一覧で見て、ホバーして `<Tab>`、閉じて `g`+`f` | 一覧の行が `j-target  ->`（`13.7-list.png`）。spot は `Link to a directory` / `Symlink` / Target `R:\Temp\w13\target-dir`。`g`+`f` の後、ホバーのパスが **`R:\Temp\w13\target-dir\inside.txt`**（`13.7-after-gf.png`、`13-spot-readouts.txt`）。C: の NTFS 上に同じ形で作ったジャンクションでも `Symlink` と Target・Resolves が出た |
| 13.16 | `fsutil hardlink create locked-2.txt locked.txt`、別の pwsh で `locked.txt` を `FileShare.None` の書き込みで開いたまま `<Tab>` | ロックが効いていた証拠に、同じ spot の別の行が `…別のプロセスが使用中です。 (os error 32)`。その下に `Hardlink` / **`2`** / Also at `R:\Temp\w13\locked.txt`。`locked.txt` 側から見ても `Hardlink` / `2` / `R:\Temp\w13\locked-2.txt`。`fsutil hardlink list` は `\Temp\w13\locked.txt` と `\Temp\w13\locked-2.txt` の 2 つで、自分を除いた集合と一致（`13.16-spot.png`、`13-spot-readouts.txt`） |
| 15.1 | `src\a.txt` を `y` し、空の `dst1` に入って `<C-->`（JIS で `Ctrl` + VK 0xBD） | トースト `Scale 90%`、ヘッダは `1 copied · 0 items`。`dst1` の中身は押す前・押した後・最後まで **0 件**（`15.1-yanked.png`、`15.1-ctrl-minus.png`） |
| 15.2 | 90% から `<C-+>`（`Ctrl`+`Shift`+VK 0xBB = JIS の `+`）、続けて `<C-=>`（`Ctrl`+`Shift`+VK 0xBD = JIS の `=`） | トーストが `Scale 100%`、次に `Scale 110%`（`15.2-ctrl-plus.png`、`15.2-ctrl-equals.png`） |
| 15.3 | 110% から `<C-0>` | トースト `Scale 100% ×2`（直前の 100% のトーストと合流した）（`15.3-ctrl-0.png`） |
| 15.4 | 100% から `<C-->` を auto-repeat 付きで 41 回の keydown、20% から `<C-+>` を 71 回 | 下の表。20% と 500% で止まることは読めた。**チェックは付けていない** |
| 15.5 | 同じドライブ: `src\a.txt` を `y`、`dst1` で `=`。ドライブ跨ぎ: `R:` の `a.txt` を `y`、`C:` の一時ディレクトリで `=` | 同じドライブ: `R:\Temp\w13\dst1\a.txt` ができ、`fsutil hardlink list` が `\Temp\w13\src\a.txt` と `\Temp\w13\dst1\a.txt`。spot は `Hardlink` / `2` / `R:\Temp\w13\src\a.txt`（`15.5-spot.txt`）。跨ぎ: トースト `Hardlink: a.txt: ファイルを別のディスク ドライブに移動できません。 (os error 17)`、tasks は `Hardlink 1 item(s) [failed]`、C: 側のディレクトリは空のまま（`15.5-cross-toast.png`、`15.5-cross-tasks.png`） |
| 15.7 | `~` でヘルプを開いてスクロール | `= Hardlink the yanked files → hardlink`、`<C-+>` / `<C-=>` `Make everything bigger → scale in`、`<C--> Make everything smaller → scale out`、`<C-0> Back to the original size → scale reset` が他の行と同じ形で並んでいる（`15.7-help-scrolled.png`） |

15.4 で読んだトースト（同じ文言のトーストは `×N` で 1 つにまとまる）:

| 操作 | トースト | 読み方 |
| --- | --- | --- |
| 100% から `<C-->` を 41 回 | `Scale 20% ×34` | 100 → 20 は 10% 刻みで 8 回。残り 33 回は 20% のまま → 1 + 33 = 34 で勘定が合う |
| そのまま `<C-+>` を 1 回ずつ 4 回 | `Scale 30% ×2` … `Scale 60% ×2` | 床から普通に戻れる（`15.4-floor-then-4-up.png`） |
| 20% から `<C-+>` を 71 回 | `Scale 500% ×24` | 20 → 500 は 48 回。残り 23 回は 500% のまま → 24 で勘定が合う（`15.4-hold-plus.png`） |

### RAM ディスク上のジャンクションで `Resolves` が `no (… os error 1)` になる

- `R:`（RAM ディスク）の `j-target`（`mklink /J`、中身は普通に辿れる）の spot が
  `Resolves  no (ファンクションが間違っています。 (os error 1))`。**同じ画面で `g`+`f` は
  `target-dir\inside.txt` に着地している**ので、リンク自体は生きている。
- 同じ形のジャンクションを C: の NTFS に作ると `Resolves` は正しく出た（`…\c--dev-filer\…\w13c\target-dir`。
  パスの `c--dev-filer` が小文字になって返るのは `canonicalize` の結果そのまま）。
- `Resolves` は `std::fs::canonicalize`（`src/spot.rs:109`）の結果で、エラーなら `no ({e})`。
  Windows の `canonicalize` は `GetFinalPathNameByHandleW` を使うので、それを実装していない
  ボリューム（このドライバの RAM ディスク）では `ERROR_INVALID_FUNCTION` になる、と読んだ。
  ドライバ側を確かめたわけではない。
- 困ること: 壊れたリンク（13.12）と**同じ `no (…)` の見た目**になるので、生きているリンクを
  壊れていると読ませる。RAM ディスク、一部のネットワークドライブ、仮想ドライブで起こりうる。
- **直していない。**案としては、`canonicalize` が失敗したら `read_link` の Target を
  リンクの親に対して解決して `metadata()` で存在を見る、という退路がある
  （成功したら、正規化していないことが分かる書き方で出す）。

### TESTING.md 15.4 は 2 行に分けたほうがいい

- 15.4 は「20% / 500% で止まる」（トーストで読める）と「滑らかに縮む」（見た目）が 1 行に
  入っているので、前半を確かめても `[x]` を付けられない。上の表のとおり前半は数えられた。
- 「押しっぱなしで 20% / 500% で止まる（トーストの `×N` が勘定に合う）」と「押しっぱなしの
  間の描き替えが滑らか」の 2 行に分ければ、前半は実機のセッションで付けられる。
  文言の変更なので、ここでは報告だけにする。

### Proposals

1. **ジャンクションは `Kind: Junction` と出すべき。**
   - 起きたこと: 13.7 で `mklink /J` のジャンクションの spot が `Kind: Symlink`。`dir` は
     `<JUNCTION>`、Explorer のプロパティも別物として扱う。
   - 変えるべきこと: 再解析タグ（`IO_REPARSE_TAG_MOUNT_POINT`）を見て `Junction` と出す。
     一覧の `->` はそのままでいい（「追える」ことを言う印なので）。
   - 理由: ジャンクションは相対にできず、別のマシンの共有を指せず、消したときの振る舞いも
     ツールによって違う。「これはどちらか」を確かめるためだけに `dir` を開くことになる。
   - 大きさ: `link()` の中の 1 関数（`#[cfg(windows)]`）。
2. **ハードリンクがドライブを跨いだときは、filer の言葉で言うべき。**
   - 起きたこと: 15.5 の跨ぎで、トーストが `ファイルを別のディスク ドライブに移動できません。 (os error 17)`。
     **移動はしていない**のに「移動できない」と言われる（OS の `ERROR_NOT_SAME_DEVICE` の文言）。
   - 変えるべきこと: os error 17 のときは `Hardlink: a.txt: hardlinks can't cross drives (R: → C:). Use p to copy instead.`
     のように、理由と代わりの手を言う。13.8 の `os error 1314` で既にやっている形と同じ。
   - 理由: 何が悪かったかは分かっても、「移動」の文字で自分の押したキーを疑う。
   - 大きさ: エラーの写像に 1 腕。
3. **拡大縮小が端で止まったら、トーストがそう言うべき。**
   - 起きたこと: 15.4 で床に着いてからも押し続けると `Scale 20% ×34` になるだけ。
     `×34` が「34 回縮んだ」なのか「止まっている」なのかは、数を数えないと分からない。
   - 変えるべきこと: 端では `Scale 20% (minimum)` / `Scale 500% (maximum)` と出す。
   - 理由: 押しっぱなしにする人ほど「まだ効いているのか」を知りたい。
   - 大きさ: 1 行。
4. **`g <Space>`（`cd --interactive`）の入力欄は、打ち始めたら中身を置き換えるべき。**
   - 起きたこと: 15.5 の跨ぎを作るとき、`g <Space>` の欄（今のディレクトリが入っている）に
     `C:\tmp\w13-cross` と打ったら、トーストが `R:\TeC:\tmp\w13-crossmp\w13: … (os error 123)`。
     打った文字が既存のパスの**途中**（`R:\Te` の後）に差し込まれた。補完が途中で
     カーソルを動かしたように見えるが、原因は確かめていない。
   - 変えるべきこと: 開いた時点で中身を全選択にして、最初の 1 文字で置き換える
     （Explorer のアドレスバーと同じ）。少なくともカーソルは末尾に固定する。
   - 理由: 絶対パスを打ちたいとき、今のパスは邪魔でしかない。消す手間が毎回かかり、
     消し損ねると上のような存在しないパスに飛ぼうとする。
   - 大きさ: 入力欄の初期化の数行。補完がカーソルを動かしているなら、そちらは不具合。

### ハーネスについて

- **クリップボードは他のセッションと共有している。**spot を `y` で写す読み方をしている間に
  別のセッションのハーネスが同じことをしていて、1 回の読み取りの 8 行目以降が
  `R:\Temp\filer-46-15\unmerged-46-15.txt` に化けた。他の filer が動いていないときに
  取り直したものだけを使った。**並行して走るなら、クリップボードで読む手順は同時に回せない。**
- **この画面は 150% で、DPI を意識しない pwsh は窓の矩形を 1.5 で割った値で受け取る。**
  キャプチャが左上 3 分の 2 だけになり、トースト（右上）もステータスバー（下）も写らなかった。
  `SetThreadDpiAwarenessContext(-4)` を先に呼べば 3862 x 2182 の実寸で取れる（`lib.ps1`）。
- `ToastShot` は右上 1400 x 420 の切り出し。トーストは 6 秒で消えるので、押してからすぐ撮る。
- 起動直後の `MainWindowHandle` は短命の窓を指すことがあり、そこへ送ったキーは全部消えた。
  タイトルが `Filer` で始まるまで待ってから取る。
- `Hover 'dst'` は `dst1` に着地した（`f` のフィルタの先頭）。同じドライブのハードリンクの
  試験としては同じなので、そのまま使った。

## TESTING.md section 46 (46.12–46.16) — Windows 実機で確かめた（51e6fb0 / 0.49.1）

v0.48.0 で入った `Came in via` / `From branch` の行。Windows 11 Pro 10.0.26200、
`target\release\filer.exe`（`scripts\fetch-conpty.ps1` で同梱 ConPTY 1.24.260710001 を配置済み）。
ハーネスは `C:\dev\filer-evidence\46b\spot46.ps1`（46 節の初回のものに `-Tcp` を足しただけ）。
キーは filer の窓に `PostMessage` で送り、spot の各行で `y` → `Get-Clipboard` で値を読む。
**`y` は値だけをコピーする**ので、ラベルとの対応は行の順（`src/spot.rs:215-235`）と
スクリーンショット（`*-open.png`、同じフォルダ）で取った。どの実行も全行がスクリーンショットと一致した。

| 行 | 結果 | 根拠 |
| --- | --- | --- |
| 46.12 | 合格 | `src\terminal.rs`: 行 11 `88128c8  2026-09-29 16:07`、行 15 `#71  48b6c9c`、行 16 `claude/task-09i0cs`。git 側の期待値 `git rev-list --merges --ancestry-path 88128c8..HEAD` の最古 = `48b6c9c Merge pull request #71 from uchmk/claude/task-09i0cs`、`88128c8` はその第 1 親から届かない |
| 46.13 | 合格 | `gh pr view 71 --json number,title,headRefName,mergeCommit,files` → `number 71`、`headRefName claude/task-09i0cs`、`mergeCommit 48b6c9c…`、`files` に `src/terminal.rs` |
| 46.14 | 合格 | `src\preview\csv.rs`: 行 11–14 `dbcedad  2026-09-27 03:26` / 件名 / `Claude` / `2`、行 15 は Text 節の `UTF-8`（`Came in via` も `From branch` も無い）。対象の探し方は下 |
| 46.15 | 合格 | `R:\Temp` の使い捨て clone、ブランチ `topic-unmerged` のコミット `02ed675`（`unmerged-46-15.txt`）: 行 10–12 `02ed675  2026-09-30 08:38` / `46.15: committed on a branch, not merged` / `QA`、行 13 は `UTF-8`。本物のチェックアウトにはコミットしていない |
| 46.16 | **未チェック（人が行う）** | 回線を切るとこのセッションが切れるので実行していない（この run の指示。main の 281e433 で行の文言と役割定義に「ファイアウォールで `filer.exe` / `git.exe` の外向きを塞ぐ」形が足されたが、この run では使っていない。管理者権限も要る）。補助の証拠だけ: 46.12 と同じ画面で Git 節を出したまま `Get-NetTCPConnection -OwningProcess 75548`（filer）を 2 回 → **どちらも 0 件**。取れたのは filer 本体の接続だけで、filer が一瞬起動する `git.exe` の接続は見ていない |

46.14 の対象の探し方（main の first-parent の線上にある非 merge コミットが、最後に触れたファイル）:

```bash
git rev-list --first-parent --no-merges HEAD > /r/Temp/fp.txt
for f in $(git ls-files); do c=$(git log -1 --format=%H -- "$f"); grep -q "^$c$" /r/Temp/fp.txt && echo "$(git log -1 --format='%h %ad %s' --date=short $c) :: $f"; done
```

`src/preview/csv.rs` を選んだ理由は、**v0.48.0 のバグ（後から来ただけの merge に帰属させる）を
再現できる形だから。**`dbcedad` の後で最も古い merge は `0fee23c`（件名は `v0.45.6: a symbol
reached with shift never matched its binding, on any keyboard`）で、`git merge-base --is-ancestor
dbcedad 0fee23c^1` が真になる。第 1 親の判定が無ければ `Came in via  0fee23c` と出ていたはず。

### 並行していたセッションとクリップボードの取り合い

- 最初の 46.12 は、別の実機セッションのハーネス（`R:\Temp\t1d\s22.ps1`、08:36 起動）と重なった。
  `y` のたびに**こちらが入れていない `<<s 1022932342>>` のような値**がクリップボードに入り、
  フィルタも `<Tab>` も効かなかった。人の指示でそのプロセスを止めてから取り直した。
- 46.15 の 1 回目も、`C:\dev\filer-win13` の filer（09:01 起動、13 節）と重なり、行 0–9 に
  `Hardlink` などの他人の値が入った。こちらは止めずに取り直し、2 回目（`unmerged2.txt`）が
  スクリーンショットと 1 行ずつ一致したものを証拠にした。
- **このハーネスの方式（クリップボード経由）は、同時に 1 セッションしか成り立たない。**
  役割定義の「one session at a time」は作業ディレクトリだけでなく、クリップボードにも効く。

### 手で組んだ一時ディレクトリのテストが、もう 1 本落ちた

上の節（section 1、`a_send_does_not_disturb_what_is_yanked`）と同じ形の 2 件目。

- 準備の 1 回目の `cargo test`（TEMP = `R:\Temp`、`C:\dev\filer`）で
  `app::follow_says_what_it_is_for::an_ordinary_file_is_told_that_it_is_not_a_link` が
  `src\app.rs:6190:62` で panic した（491 通過 / 1 失敗）。6190 行は
  `crate::fs::Entry::from_path(plain).unwrap()` で、直前に書いた `plain.txt` を読めなかった。
- 単独で 1 回、全体を続けて 3 回回し直して、どれも通った（4 回の全体実行で 1 回）。
- 一時ディレクトリは `std::env::temp_dir().join("filer-follow-msg")`。上の節が挙げた 4 つのうちの 1 つ。
- **同じ朝に、4 つのうち 2 つが別々のセッションで 1 回ずつ落ちた。**この時間帯には
  `C:\dev\filer-win13`（section 13）と `test/win-1d`（section 1）のセッションも、同じ
  `TEMP=R:\Temp` で `cargo test` を回していた。どちらも名前にプロセス ID が入らないので、
  **別のチェックアウトのテストプロセスと同じディレクトリを取り合える。**これが原因だとは
  確かめていない（同時刻に走っていたかは記録が無い）。
- `util::test_dir` と、`src/app.rs:5430`（`filer-move-undo-{pid}`）・`src/preview/external.rs:131`
  （`filer-preview-{pid}-{n}`）は、名前にプロセス ID を入れている。**直していない。**

### Proposals

3 件。

1. **spot の中身を、ラベル付きで丸ごとコピーできるようにする。**
   - 何が起きたか: 46.12〜46.15 では各行で `y` を押して値を読んだが、`y` は**値だけ**を
     コピーする。どの値が `Came in via` なのかは、行の順とスクリーンショットで照らすしかなかった。
     しかも `Commits` の行は件数が 1 なら出ない（46.15）し、Git 節より上の行数もファイルで変わるので、
     同じ `UTF-8` が 46.14 では 15 行目、46.15 では 13 行目に来る。**行番号で読むと、ファイルごとに意味がずれる。**
   - どう変えるか: `[spot]` に `Y`（または `c` `a`）で、パネル全体を
     `Label<TAB>value` の行として 1 回でコピーするコマンドを足す。
   - なぜ: バグ報告やチャットに「spot には何と出ていたか」を貼るとき、今はスクリーンショットになる。
     テキストなら検索でき、差分も取れる。このセッションのハーネスも、20 回の `y` が 1 回で済む。
   - 大きさ: 関数 1 つとキー 1 つ（行の組み立ては `spot.rs` に既にある）。
2. **「直接 push された」と「まだどこにも入っていない」を見分けられるようにする。**
   - 何が起きたか: 46.14（`csv.rs`、main に直接）と 46.15（未マージのブランチ）は、
     **spot の見た目が同じ**になる。どちらも履歴の行だけで、`Came in via` が無い。
     行の期待どおりだが、spot だけを見た人は「このコミットは main にあるのか」を答えられない。
   - どう変えるか: 既定ブランチ（`origin/HEAD`）に届いていないコミットにだけ、
     `Not merged` のような 1 行を出す。
   - なぜ: レビュー中のブランチでファイルを眺めるとき、一番知りたいのは「これはもう入ったか」。
   - 大きさ: 設計の判断が要る。既定ブランチをどう決めるか（`origin/HEAD` が無い clone もある）と、
     `merge-base --is-ancestor` が 1 回増えること。
3. **`Came in via` の `#71` から、その pull request を開けるようにする。**
   - 何が起きたか: 46.13 では `#71` を読んで、手で `gh pr view 71` を打った。値は `#71  48b6c9c` と
     ハッシュ付きでコピーされるので、そのまま貼っても使えない。
   - どう変えるか: その行で `<Enter>`（または `o`）を押すと、`remote.origin.url` から
     `https://github.com/<owner>/<repo>/pull/71` を組んで既定のブラウザで開く。
   - なぜ: 「どの PR で入ったか」を知った次にしたいのは、たいていその PR を読むこと。
     URL を組むだけなので、46.16 の「何も外に出さない」は開くまで守られる。
   - 大きさ: 関数 1 つ。GitHub 以外のホストをどうするかは判断が要る。

## TESTING.md section 1 — 1.30 の「全高で描かれる」半分をページ移動で測る（ade8588 / 0.50.1）

Windows のセッション（`.claude/windows-role.md`、無人実行）から。担当は順番表の先頭に残っていた
1.30 だけ。実機の release ビルド（0.50.1、ade8588）に `scripts\fetch-conpty.ps1` で同梱の ConPTY
（1.24.260710001）を置き、`[term] shell = pwsh` の隔離した設定（`R:\Temp\t1bcfg`）で動かした。
画面は 150%（dpi 144）、窓はクライアント 2040 x 1290（既定の大きさ、最大化していない）。
スクリプトと生の出力・キャプチャは `C:\dev\filer-evidence\1e\` にある（`s30.ps1`、`run.txt`、`shots\`）。

**1.30 にチェックを付けた。これで section 1 の順番表の行は無くなる。**新しい不具合は見つからなかった。

### 測り方

`<C-f>` は `arrow 100%` で、動く量は `page_rows`。`page_rows` は `ui::draw_pane`（`src/ui/mod.rs:617`）が
**毎フレーム、一覧を描いた矩形の高さから**決めている。だから `many\`（`item-001`〜`item-500`）の
先頭から `<C-f>` で着く名前は、一覧がその時に何行ぶんの高さを貰っていたかをそのまま表す。
読むのは `c` `f` と `Get-Clipboard`（センチネルを置いてから）。1 回の filer の中で 4 つの状態を順に測った。

| 状態 | `g` `g` → `<C-f>` → `c` `f` | ペインの pwsh の `WindowSize.Height` | filer の子プロセス |
| --- | --- | --- | --- |
| A. まだペインを開いていない | `item-034.txt`（33 行） | — | — |
| B. `<C-t>` で開き、`<C-t>` でキーを一覧へ（3 分の 1） | `item-021.txt`（20 行） | 12 | `OpenConsole.exe, pwsh.exe` |
| 最大化（一覧側から `<C-S-Enter>`） | — | 35 | — |
| **C. 最大化のまま `<C-S-t>`** | **`item-034.txt`（33 行）** | — | `<none>` |
| D. `<C-t>` で開き直し、キーを一覧へ | `item-021.txt`（20 行） | 12 | `OpenConsole.exe, pwsh.exe` |

- **C が A と同じ 33 行**なので、`<C-S-t>` の後の一覧は、ペインを一度も開いていないときと同じ高さを
  貰っている。隙間が残っていれば B の 20 行（またはそれ以下）になる。A と B が 13 行違うので、
  このプロキシは「ペインのぶん縮んだ」を実際に見分けられる。
- 「隙間の**上に**一覧が重なって描かれているだけ」なら `page_rows` は 33 のままでも下が隠れる。
  それは `<C-S-t>` から 2 秒後、何も押す前のキャプチャ `shots\d-closed.png` で読んだ:
  一覧は `item-009.txt`〜`item-041.txt` の 33 行で、`item-041.txt` のすぐ下がステータスバー
  （`NORMAL ... 21/500`）。A の `shots\a-fresh.png` と同じ並び・同じ最下行。ペインもトーストも無い。
- 「最大化が残らない」の半分は前回（0.49.1）と同じ結果を繰り返した: 開き直した D が 12 行（最大化の 35 ではない）で、
  一覧のページも B と同じ 20 行。

### Proposals

2 件。

1. **`<C-S-t>` でシェルが終わったことを、画面で言う。**
   - 何が起きたか: 最大化したまま `<C-S-t>` を押すと、ペインが消えて一覧に戻る。それだけで、
     **シェルが終わったのか、ペインが隠れただけなのかは画面からは分からない**（`<C-t>` なら隠れるだけで
     シェルは残る、1.5）。今回は filer の子プロセスが `<none>` になったことで確かめた。2 秒後の
     `shots\d-closed.png` にトーストは無い（トーストは 6 秒出るので、あれば写っている）。
   - どう変えるか: `<C-S-t>` で終わらせたとき `Shell ended` のトーストを出す。シェルの下に
     別のプロセス（lazygit など）が動いているときは、終わらせる前に確認を挟む。
   - なぜ: `<C-t>` と `<C-S-t>` は Shift 1 つしか違わず、結果の見た目も同じ。1.27 の run では
     lazygit を動かしたまま最大化していたので、そこで Shift を押し間違えると作業中の TUI ごと
     黙って消える（これは今回は試していない。推測）。
   - 大きさ: トーストは 1 行。確認を挟むのは子プロセスの見方が OS ごとに違うので関数 1 つ以上。
2. **一覧の行数（`page_rows`）とペインの行数を、外から読めるようにする。**
   - 何が起きたか: 一覧が何行の高さを貰っているかを知るのに、`g` `g`、`<C-f>`、`c` `f` の 5 打鍵と
     クリップボードの往復が 1 回の測定ごとに要った。クリップボードは他のセッションと取り合う
     （section 46 の run で実際に混ざった）ので、この測り方は 1 台で 1 セッションしか回せない。
   - どう変えるか: `filer env` の出力（すでに Window の行がある）に、最後に描いたフレームの
     `list rows` / `pane rows` を足す。起動中の filer から取るのが難しければ、
     `[mgr] title_format` に `{pane}` のような置換を足してタイトルで読めるようにする。
   - なぜ: レイアウトに関する行（1.26〜1.30、1.8 の組み直し）は、今はどれも見た目か、手間のかかる代理でしか確かめられない。
     読める値が 1 つあれば、人に残る行が減る。
   - 大きさ: 設計の判断が要る（起動中のプロセスからどう取り出すか）。タイトルの置換なら関数 1 つ。

### 順番表を更新できなかった

役割定義は「同じ PR で `.claude/windows-role.md` の順番表から自分の節を外す」ことを求めているが、
このセッションではそのファイルの編集が権限で拒否された（保護されたファイル扱い）。回避はしていない。
**マージする側で、次の変更を入れてからマージしてほしい。**入れないままマージすると、次の無人実行が
また section 1 を取る。

- 表から `| **1. the terminal pane** | 1.30 | ... |` の行を消す。
- 「Worked through before」の行を `... 13 / 15, 46 and 1.` にする。

## TESTING.md section 26 — ARM64 実機で確かめた（0a2209f / 0.51.1、ARM64 レーン）

ARM64 レーンの 1 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
担当は順番表の先頭にあった 26.5 と 26.6 だけ。

**26.5 にチェックを付けた。26.6 は落ちた** —— これがこの run の中身で、**プログラムの不具合**である。

### この機械

| | |
| --- | --- |
| 機械 | `(Get-CimInstance Win32_ComputerSystem).SystemType` = `ARM64-based PC`、`PROCESSOR_ARCHITECTURE=ARM64` |
| OS | `Microsoft Windows 11 Home` / `10.0.28000`、レジストリは `EditionID=Core` `DisplayVersion=26H1` `CurrentBuild=28000` `UBR=2956` |
| rustc | 1.98.1 (aarch64-pc-windows-msvc) —— CI と同じ stable |
| 昇格 | 無し（`IsInRole('Administrators')` = False）。26 のどの行も昇格を要らない |
| ネイティブ build | `cargo build --release` → `target\release\filer.exe`、PE machine **0xAA64**（ARM64） |
| x64 build | リリース v0.49.1 の `filer-v0.49.1-windows-x64.zip`、`filer.exe` の PE machine **0x8664**（AMD64） |
| ConPTY | `scripts\fetch-conpty.ps1 -Dest target\release` → `ConPTY 1.24.260710001 (arm64)`、`conpty.dll` と `OpenConsole.exe` はどちらも 0xAA64。**arm64 を正しく選んでいる** |
| `cargo test` | **493 passed; 0 failed**（ネイティブ ARM64、1.68s）。x64 ランナーに無い失敗は無い |

`src\bugreport.rs` は **v0.49.1 から 1 行も変わっていない**（`git log v0.49.1..HEAD -- src/bugreport.rs` が空）。
だから下の x64 の観測は 0.49.1 のものではなく、**いまの main のもの**である。この機械には x64 の
MSVC ツールチェーンが無い（`rustup target list --installed` は aarch64 だけ、vswhere も無し）ので、
x64 は配布物を動かす形で試した。

生の出力は `C:\dev\filer-evidence\arm-26\`（`arm64-env.txt`、`x64-env.txt`、`arm64-f12-url.txt`、
`x64-f12-url.txt`、および測定に使った `grab.ps1` / `input.ps1`）。

### GUI バイナリが `CONOUT$` に書いた文字を読む方法

release build は GUI サブシステムなので、`say()`（`src/main.rs:51`）が `AttachConsole` して
`CONOUT$` へ書く。**だから `filer env > out.txt` でも `$v = filer env` でも 1 文字も取れない**
（実際に両方とも空だった）。代わりに、実行したあと**コンソールのスクリーンバッファを
`ReadConsoleOutputCharacterW` で読み返した**（`grab.ps1`）。これで `filer env` の全文がテキストとして取れる。
無人で `filer env` を測る後続の run はこれを使えばよい。

### 26.5 —— ネイティブ ARM64 ビルド（合格、チェック済み）

行の期待は「OS arch と Process arch がどちらも `aarch64`」。**`<F12>` の実物**で確かめた:
ネイティブ release build（0.51.1）を起動し、`SetForegroundWindow` で前面に出したことを
`GetForegroundWindow` の pid で確認してから `SendInput` で `<F12>` を送り、開いた Chrome の
アドレスバーを `<C-l>` `<C-c>` で読んだ（クリップボードにはセンチネルを先に置いた）。

```
https://github.com/uchmk/filer/issues/new?template=bug_report.yml&version=filer%200.51.1%20%28aarch64%29&os=OS%3A%20Windows%2011%20Core%2026H1%20%28build%2028000.2956%29%0AOS%20arch%3A%20aarch64%0AProcess%20arch%3A%20aarch64
```

`os=` を復号すると:

```
OS: Windows 11 Core 26H1 (build 28000.2956)
OS arch: aarch64
Process arch: aarch64
```

`filer env` も同じことを言う（`arm64-env.txt`）: `OS arch : aarch64` / `Process arch : aarch64`。
2 つが一致するのは偶然ではなく、`envreport.rs:49` が `bugreport::os_line()` を行に割っているだけだから。
`filer --version` は `filer 0.51.1 (aarch64)`。

### 26.6 —— x64 ビルドをエミュレーションで（**落ちた**）

行の期待は「OS arch は `aarch64`、Process arch は `x86_64` —— **食い違うことが、まさに報告したい事実**」。
実際に出たのは**食い違わない 2 行**である。

`filer env`（`x64-env.txt`、PE machine 0x8664 の binary、`Path` で走っている exe を確認した）:

```
    Version      : 0.49.1
    OS           : Windows 11 Core 26H1 (build 28000.2956)
    OS arch      : x86_64      <-- aarch64 のはず
    Process arch : x86_64
```

`<F12>` の URL も同じ（`x64-f12-url.txt`。version が `0.49.1 (x86_64)` なので、
26.5 の URL の読み直しではなく x64 の binary が組んだものだと分かる）:

```
OS: Windows 11 Core 26H1 (build 28000.2956)
OS arch: x86_64
Process arch: x86_64
```

#### 不具合: `GetNativeSystemInfo` はエミュレーションに影響される

`native_arch()`（`src/bugreport.rs:106`）は `GetNativeSystemInfo` を呼び、コメントはこう書いている:

> the machine underneath is what `GetNativeSystemInfo` answers, and it is
> **unaffected by the emulation**. When the two lines disagree, the disagreement
> is the finding.

**この前提が成り立っていない。**ARM64 上の x64 エミュレーションは、互換性のために
`GetNativeSystemInfo` にも `PROCESSOR_ARCHITECTURE_AMD64` を返す（Microsoft の文書どおりの動作で、
Windows の不具合ではない）。`GetNativeSystemInfo` が本当に効くのは x64 ホスト上の 32bit WOW64 だけ。

結果として、**この節がわざわざ 2 行に分けて防ごうとしていた誤解を、いまは自分で作っている**:
ARM64 機からエミュレーションの x64 build で出した報告は、`OS arch: x86_64` と、
**この機械は x64 だと主張する**。26.6 の「quietly misleads everyone」がそのまま起きる。
番号の付いた行が 1 つ落ちただけでなく、**節の目的そのものが逆向きに働いている。**

どちらの間違いか: **プログラム。**行の文言は正しく、行が正しいからこそ落ちたと分かった。

修正の候補（QA / 実機のロールは直さないので、提案として）: `IsWow64Process2` の
`NativeMachine`。同じ機械で、走っているエミュレーションの x64 filer をネイティブ ARM64 の
PowerShell から問うと、正しく ARM64 が返る:

```
emulated x64 filer (pid 2604, ...\filer-v0.49.1-windows-x64\filer.exe):
  IsWow64Process2 -> ProcessMachine=0x0000 NativeMachine=0xAA64   (0xAA64 = ARM64)
```

**ただしこれは native の呼び出し元から問うた結果でしかない。**`NativeMachine` が
エミュレーションの側から呼んでも偽られないことは（文書はそう述べているが）この run では
確かめていない。直す人はこの機械で必ず取り直すこと —— それが取れれば 26.6 と 25.5 が同時に閉じる。
なお `ProcessMachine` が `0x0000`（= WOW64 ではない）なのも意外で、この OS（build 28000）の
x64 エミュレーションは古典的な WOW64 として報告されない。**`ProcessMachine` で分岐する実装は
書かないこと。**アーキテクチャは `NativeMachine` と `std::env::consts::ARCH` の 2 つで足りる。

### 25.5 も同じ理由で落ちる（チェックは付けていない）

25.5 は 26.6 と同じ事実（`filer env` の 2 行が食い違うこと）を見る行で、上の `x64-env.txt` が
そのまま証拠になる。**節が違うので 1 run 1 節の規則に従ってチェックは付けていない**が、
取り直す run は要らない: 直したあとに 2 行同時に確かめられる。順番表への提案は下。

### 付随して見つけたこと: エディションが `Core` と出る

26.4 はもう x64 機（Pro）で `[x]` だが、この機械では `windows_name()`（`src/bugreport.rs:67`）が
**`Windows 11 Core 26H1`** と出す。`Core` はレジストリの `EditionID` の綴りで、
**Windows 自身はどこでも「Home」と呼ぶ**（`Win32_OperatingSystem.Caption` = `Microsoft Windows 11 Home`、
winver も About も「Windows 11 Home」）。`Professional` → `Pro` の読み替えは既にあるのに、
Home の側が無い。報告に貼られると、読む人が知らないエディション名になる。

- 根拠: `EditionID : Core` / `Caption : Microsoft Windows 11 Home` / filer は `Windows 11 Core 26H1 (build 28000.2956)`
- どちらの間違いか: プログラム。1 行で直る（`Core` → `Home`、`CoreSingleLanguage` → `Home Single Language` など）
- なお、同じコメントが `ProductName` を使わない理由として挙げている「Windows 11 でも `Windows 10 Pro` と読める」は
  **この機械でも本当だった**（`ProductName : Windows 10 Home`、実際は Windows 11）。そこの判断は正しい。

### Proposals

3 件。

1. **`filer env` を GUI バイナリのまま、パイプでも読めるようにする。**
   - 何が起きたか: この run の測定は全部 `filer env` から始まるのに、`filer env > out.txt` も
     `$v = & filer env` も**空**だった。`say()` が `AttachConsole(ATTACH_PARENT_PROCESS)` に成功して
     `CONOUT$` へ書くので、リダイレクトされた stdout には何も行かない。コンソールの
     スクリーンバッファを `ReadConsoleOutputCharacterW` で読み返す 40 行の PowerShell を書いて回避した。
   - どう変えるか: `say()` で、`GetStdHandle(STD_OUTPUT_HANDLE)` が**コンソール以外**
     （`GetFileType` が `FILE_TYPE_DISK` / `FILE_TYPE_PIPE`）を指しているときは `AttachConsole` より先に
     そちらへ書く。人が打ったときは今までどおり `CONOUT$`、リダイレクトされたときはそのファイルへ。
   - なぜ: `filer env` は「バグ報告に貼るテキスト」なのに、**スクリプトから集められない。**
     無人の実機テストは全部スクリプトで、版・アーキテクチャ・警告を読むのはここしかない。
     人にも効く: `filer env | clip` や `filer env > env.txt` が黙って空を作るのは驚く。
   - 大きさ: 関数 1 つ（`say()` の分岐 1 つ）。
2. **`filer env` に、アーキテクチャの根拠を言わせる。**
   - 何が起きたか: x64 build が `OS arch : x86_64` と言ったとき、それが**嘘だと分かったのは
     `Win32_ComputerSystem.SystemType` を別に見たから**で、filer の出力の中には矛盾が無い。
     報告を受け取る側にも同じことが起きる: 一貫して x64 だと書かれた報告が届く。
   - どう変えるか: エミュレーションを検出できる実装（上の不具合の修正）とセットで、
     `Process arch : x86_64 (emulated on aarch64)` のように 1 行で言う。
   - なぜ: この節の存在理由は「どのバイナリが動いているか」を報告に必ず載せることで、
     いま**そこだけが信用できない。**ARM64 機の報告は今後増える。
   - 大きさ: 上の不具合の修正に乗るので数行。ただし文言は持ち主の判断。
3. **`<F12>` の URL を、ブラウザに渡すだけでなくクリップボードにも置く。**
   - 何が起きたか: 26.5 / 26.6 の期待値はフォームに入る**テキスト**なのに、それを読む手が無かった。
     Chrome が既に動いていたので新しいプロセスは生まれず、`Win32_Process` の `CommandLine` には
     何も出ない（`CommandLine LIKE '%issues/new%'` を 20 秒ポーリングして 0 件）。結局
     ブラウザを前面に出して `<C-l>` `<C-c>` を送り、アドレスバーをコピーした —— **ブラウザの
     キー割り当てに依存する測り方**で、Chrome 以外だと書き直しになる。
   - どう変えるか: `bug-report` が URL をクリップボードにも入れ、トーストでそう言う。
   - なぜ: 26.8（ブラウザが開けないとき）の実害もこれで小さくなる —— 開けなくても URL は手元にある。
     テストの側では、フォームに入る値が**ブラウザ抜きで**読めるようになる。
   - 大きさ: 1 行（`exec::set_clipboard` を `Act::BugReport` に足す）＋トーストの文言。
     既にクリップボードにあるものを踏むので、持ち主の判断が要る。

### 順番表（`.claude/windows-role.md`「The ARM64 lane」）を更新できなかった

役割定義どおり、無人実行はこのファイルへの書き込みを権限で拒否される。回避していない。
**マージする側で次を入れてからマージしてほしい。**入れないと、次の ARM64 の run がまた 26 を取る。

- `**26. the architecture rows**` の行を消す。26.5 は `[x]`、26.6 は**プログラムが直るまで
  付けられない**（この報告の不具合）。
- `**25. filer env**`（25.5）の行も消す。25.5 は 26.6 と同じ 1 つの不具合で落ちる行で、
  別の run を立てても同じ `x64-env.txt` が出るだけ。
- 代わりに、両方を**修正後の再確認**として 1 行にまとめるのを勧める:
  「26.6 / 25.5 の再確認 | 2 | `native_arch()` を直したあと、リリースの `windows-x64.zip` を
  この機械で走らせて 2 行が食い違うことを見る。直るまで待ち」。
- `cargo test` の行はこの run で緑（493 / 0）だった。残すか消すかは判断だが、
  **ネイティブ ARM64 で落ちるテストは無い**と記録しておく。

## TESTING.md section 12 — undo と redo をごみ箱で確かめる（0a2209f / 0.51.1）

Windows のセッション（`.claude/windows-role.md`、無人実行）から。担当は順番表の先頭の section 12（10 行）。
実機の release ビルド（0a2209f、0.51.1）を、隔離した設定（`R:\Temp\w12cfg`）で動かした。画面は 150%、
キーは filer の窓に PostMessage で送り、ホバー中の名前は `c` `f` とクリップボードで読んだ。
ごみ箱は Shell.Application の `NameSpace(10)` で、項目ごとの `System.Recycle.DeletedFrom` と
`DateDeleted` を読んだ。スクリプト・生の出力・キャプチャは `C:\dev\filer-evidence\12\` にある
（`h12.ps1`、`run*.ps1` / `run*.log`、`*.png`）。昇格はしていない。

**12.1 / 12.2 / 12.3 / 12.4 / 12.5 / 12.9 / 12.10 にチェックを付けた。**12.8 は行の期待値が
間違っている（既存の報告を実機で確かめた）、12.11 は条件を作れなかった、12.12 は Explorer の取り消し
履歴を読めない、でそれぞれ付けていない。不具合 1 件と、提案 6 件。

**フィクスチャは R: ではなく C: に置いた。**R:（RAM ディスク）には**ごみ箱が無い**
（シェルの「ごみ箱へ」で試すと、黙って完全に消えた）ので、12.1〜12.10 は既定の一時ディレクトリ
（`%LOCALAPPDATA%\Temp\filer-12`、`scripts/make-fixtures.ps1` の既定）で回した。

| 行 | やったこと | 読んだもの |
| --- | --- | --- |
| 12.1 | `many\item-007.txt`（中身 `section12-A`）で `d` | ディスクに無い。ごみ箱に `…\fx\many\item-007.txt`、削除 01:42:01、11 バイト（`runA.log`） |
| 12.2 | `u` | 元の場所に中身 `section12-A` で戻り、ごみ箱から消えた。トースト `Restored item-007.txt`（`a2-toast-after-u.png`） |
| 12.3 | `u` の直後に `w` | `Trash  Trash 1 item(s)  [done]` と `Restore  Restore 1 item(s)  [done]`、`1/1 files`（`a3-tasks-after-u.png`） |
| 12.4 | `U` | ディスクから消え、ごみ箱に削除 01:42:06 の新しい項目。トースト `Trashed item-007.txt`（`a4-toast-after-U.png`）。もう一度 `u` で戻した |
| 12.5 | `twin1\dup.txt`（`older, twin1`）を `d`、4 秒あけて `twin2\dup.txt`（`newer, twin2`）を `d`、`u` | `twin2\dup.txt` が中身 `newer, twin2` で戻り、`twin1\dup.txt` は削除 01:42:47 のままごみ箱に残った。トースト `Restored dup.txt`（`runB.log`、`b1-toast-after-u.png`） |
| 12.9 | `item-020.txt`（`section12-D-original`）を `d`、外から同名の `blocker` を作って `u` | `blocker` はディスクに残り、元のファイルはごみ箱のまま。メッセージは `Restore: item-020.txt: Error during a `trash` operation: RestoreCollision { path: "…\\many\\item-020.txt", … }`（`c1-full-after-blocked-u.png`）。`blocker` をどけてもう一度 `u` → 中身 `section12-D-original` で戻り、ごみ箱は空、トースト `Restored item-020.txt`（`c2-toast-after-second-u.png`） |
| 12.10 | 別の pwsh が `item-032.txt` を `FileShare.None` で開いたまま（開けないことを確かめた）、`item-030`〜`034` の 5 件を選んで `d` | 030 / 031 / 033 / 034 はごみ箱に入り、032 だけディスクに残った。トースト `Trash: item-032.txt: Error during a `trash` operation: Unknown { description: "Some operations were aborted" }`（`h1-after-d.png`）。タスクは `Trash 5 item(s) [failed]`、**`4/5 files`**、エラー行は `item-032.txt` だけ（`h2-tasks.png`、`runE3.log`） |

12.9 の「名前が使われていると言う」は、`RestoreCollision` と衝突したパスが出ていることで読んだ。
言い方が生の Debug 表示なのは提案 1。

### 12.8 は書いてあるとおりには起きない（既存の報告を実機で確かめた）

- `item-010.txt` を `r` で改名 → `u` で戻す → `a` で `new12.txt` を作る → `U`: トーストは
  **`Renamed to item-010renamed12.txt`** で、redo は残っていた（`c7-toast-U-after-create.png`、`runC2.log`）。
  作成は取り消しの手順を積まないので、履歴は分岐しない。上の section 12 の既存の報告のとおり。
- 同じ規則を改名で: `u` で戻す → `item-011.txt` を `r` で改名（新しい操作）→ `U`: トーストは
  **`Nothing to redo`**（`c8-toast-U-after-rename.png`）。分岐はする。
- だから TESTING.md の 12.8 を「新しいファイルを作る」→「別のファイルを改名する」に直せば、
  この 2 つ目の結果でそのまま付けられる。文言の変更なので、ここでは報告だけにする。

### RAM ディスクでは `d` が一度も効かない

- `R:\Temp\f12r\keep-me.txt` で `d`: トースト `Trash: keep-me.txt: Error during a `trash` operation: CanonicalizePath { original: "R:\\Temp\\f12r" }`、
  タスクは `Trash 1 item(s) [failed]`、`0/1 files`。ファイルはディスクに残り、ごみ箱にも無い（`f1-r-after-d.png`、`f2-r-tasks.png`、`runE.log`）。
- **失敗しているのはシェルに渡す前**で、trash クレートが親ディレクトリを `canonicalize` した段階。section 13 の
  報告（RAM ディスクのジャンクションで `Resolves` が `os error 1`）と同じ根で、このボリュームは
  `GetFinalPathNameByHandleW` に答えないと読んだ。
- 困ること: このドライブでは `d` で消せるものが 1 つも無く、メッセージは理由（「このドライブは正規化できない」）も
  代わりの手（`D` で完全に消す）も言わない。ファイルが残るので、データは失われない。
- **直していない。**

### 12.11 と 12.12 を付けなかった理由

- **12.11**: この機械にあるのは C: と R: だけ。R: は「ごみ箱を無効にした」ドライブではなく「ごみ箱が無い」ドライブで、
  しかも上のとおり**シェルに届く前に**落ちるので、12.11 が試したい経路（シェルが「ごみ箱に入れられない」と
  言う）を通っていない。メッセージの形（`Trash: <名前>: …`、ファイル名入り）だけなら合っている。
  C: のごみ箱を切る（`HKCU\…\BitBucket\Volume\{…}\NukeOnDelete = 1`）と、その間は持ち主の機械の
  **すべての削除が完全削除になる**ので、無人ではやらなかった。USB メモリを 1 本挿してそのドライブだけ切れば、
  次の実行で付けられる。
- **12.12**: 前半（ロックが無ければ以前のまま）は、5 件をまとめて消して `Trash 5 item(s) [done]`、`5/5 files`、
  `u` で 5 件とも戻ったことで読めた（`g2-tasks.png`、`runE2.log`。選択がフィルタのせいで別の 5 件に
  なった回だが、ロックの無いまとめ削除としては同じ）。後半の「Explorer の取り消し履歴に**1 つ**」は外から読めない。
  代わりに Explorer で `Ctrl+Z` を押して 5 件がまとめて戻るかを見る手はあるが、動いている机の上で押すと
  **持ち主自身の直前の操作を取り消しかねない**ので、無人ではやらなかった。人に残す。

### 改名の欄で、打った文字が語幹を置き換えずに後ろに入った（原因未確認）

- `r` の欄は語幹を選んだ状態で開く（`start_rename`、`RenameCursor::BeforeExt` → `(0, stem)`）はずだが、
  `renamed12` と打つと `item-010renamed12.txt` になった（`c6-rename-typed.png`）。開いた直後の
  `c5-rename-open.png` にも選択の色は写っていない。
- section 13 / 15 の提案 4（`g <Space>` の欄で、打った文字が途中に入る）と同じ形。どちらも
  PostMessage で WM_CHAR を送っていて、**窓が前面に無い**状態で動かしているので、ハーネスのせいの
  可能性を消せていない。人が 1 度 `r` を押して打てば分かる。12 の結果には影響しない（改名は改名なので）。

### Proposals

6 件。

1. **`RestoreCollision` は人の言葉で言うべき。**
   - 起きたこと: 12.9 で `u` が断るとき、メッセージが `RestoreCollision { path: "C:\\…\\item-020.txt", remaining_items: [TrashItem { id: "C:\\$Recycle.Bin\\S-1-5-21-…\\$R…txt", … time_deleted: 1790732640 }] }`。
     SID と `$R` のファイル名と UNIX 時刻が出る。
   - 変えるべきこと: `Restore: item-020.txt: a file by that name is already there. Move it away and press u again.`
     のように、何が邪魔をしていて次に何をすればいいかを言う。取り消しの手順は残っているので、それが言える。
   - 理由: 押し直せば通る、という一番大事なことがメッセージから読めない。
   - 大きさ: エラーの写像に 1 腕。
2. **一部だけ失敗した削除でも、消えたぶんの取り消しの手順を積むべき。**
   - 起きたこと: 12.10 で 5 件中 4 件がごみ箱に入ったが、直後の `u` は **`Nothing to undo`**（`h3-after-u.png`）。
     4 件はごみ箱に残ったまま（`runE3.log`）。失敗した `Fresh` の手順は `Undos::keep` で捨てられる。
   - 変えるべきこと: 失敗した項目を除いた残りで手順を積む（ジョブは何が通ったかを知っている。タスクの `4/5` がそれ）。
   - 理由: 一番取り消したくなるのは、思ったとおりに行かなかった操作。今は Explorer のごみ箱を開いて 4 件を探して戻すしかない。
   - 大きさ: 削除ジョブの完了で、通ったパスの一覧を手順に渡す。数十行。
3. **ロックで止まったときは、そう言うべき。**
   - 起きたこと: 12.10 のメッセージは `item-032.txt: … Unknown { description: "Some operations were aborted" }`。
     ファイル名は出る（v0.27.1 の修正は効いている）が、**なぜ**は言わない。
   - 変えるべきこと: 1 件ずつの再試行で失敗したとき、そのファイルを開いてみて os error 32 なら
     `item-032.txt is open in another program` と言う。
   - 理由: 「中断された」では、閉じれば済むのか、権限なのか、壊れているのかが分からない。
   - 大きさ: 失敗の枝で 1 回開いてみる関数 1 つ（`#[cfg(windows)]`）。
4. **正規化できないドライブで `d` が落ちたら、理由と `D` を言うべき。**
   - 起きたこと: 上の「RAM ディスクでは `d` が一度も効かない」。`CanonicalizePath { original: "R:\\Temp\\f12r" }`。
   - 変えるべきこと: この失敗のときは `Trash: keep-me.txt: this drive can't use the Recycle Bin (R:). Use D to delete permanently.` と出す。
     trash クレートの前で自前で正規化を試して、落ちたら生のパスで渡す退路も考えられる（section 13 の提案と同じ根）。
   - 理由: RAM ディスク・仮想ドライブ・一部のネットワークドライブで、`d` が黙って効かない（に見える）。
   - 大きさ: エラーの写像なら 1 腕。退路は trash クレートの呼び方を変えるので要設計。
5. **タスクパネルのごみ箱の行は、`0 B / 0 B` を出さず、動詞を 2 回言わないべき。**
   - 起きたこと: `Trash  Trash 5 item(s)`、`Restore  Restore 1 item(s)` と動詞が 2 回並び、
     2 行目は `5/5 files · 0 B / 0 B`（`h2-tasks.png`、`a3-tasks-after-u.png`）。
     ごみ箱の移動は大きさを数えていないので、`0 B` は「空のファイルだった」と読める。
   - 変えるべきこと: 大きさを数えないジョブでは `· 0 B / 0 B` を省く。見出しは `Trash 5 item(s)` の 1 回。
   - 理由: 12.10 の「件数が実際と合う」を読むとき、並んでいる数のうちどれが意味を持つのか迷った。
   - 大きさ: 描画の条件 1 つと、ラベルの組み立て 1 か所。
6. **役割定義に「section 12 は R: で回せない」と書くべき。**
   - 起きたこと: 役割定義は一時ファイルを R: に置くよう勧めているが、R: にはごみ箱が無く、しかも
     `d` が正規化で落ちる。最初の数分をそれを確かめるのに使った。
   - 変えるべきこと: 「Where to put files」の節に、ごみ箱を通る行（12、および `d` を使う行）は
     C: の一時ディレクトリで回すこと、と 1 行足す。証拠は相変わらず `C:\dev\filer-evidence\` に置く。
   - 理由: 次にごみ箱を通る節を取るセッションが、同じ確認をしなくて済む。
   - 大きさ: 文書 1 行（`.claude/` の下なので、マージする側が入れる）。

### ハーネスについて

- **`f` のフィルタはあいまい検索。**`item-03` で絞ると先頭は `item-003` / `013` / `023` で、`030` はその後
  （`g0-selected.png`）。1 回目はそれに気づかず別の 5 件を選んで消した（ロックしたファイルは選ばれていなかった。
  `u` で全部戻った）。`j` を 3 回挟んで `item-030` に着いてから `<Space>` を 5 回押した回（`runE3`）だけを使った。
- `Hover` の後に `Esc` を押すとフィルタが外れてカーソルが先頭に戻る。`e*.png` の回はそれで `item-001`〜`005` を
  消していた（`u` で戻っている）。証拠には使っていない。
- ごみ箱の項目を Shell の `InvokeVerb('undelete')` で戻すと、同じパスにファイルがあるとき
  「ファイルの置換またはスキップ」のダイアログで止まる。止まったプロセスを落として、残りは 1 件ずつ戻した。
  自分が作ってごみ箱に残った 1 件（`twin3\dup.txt`）は `$R` / `$I` を直接消した。
- `DateDeleted` は UTC で返る（01:42 は日本時間の 10:42）。

## TESTING.md 26.6 / 25.5 — 直ったので取り直した（abd67e1 / 0.51.3、ARM64 レーン）

ARM64 レーンの 2 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
順番表の先頭にあった「26.6 / 25.5, again with the fix」の 2 行だけが担当。

**どちらも合格したのでチェックを付けた。**v0.51.2 の `IsWow64Process2` が、#81 で落ちた
2 行をそのまま閉じている。**そして #81 が残していた疑問（`NativeMachine` は
エミュレーションの側から呼んでも偽られないのか）にも、この run が答えを出した —— 偽られない。**

### この機械

| | |
| --- | --- |
| 機械 | `(Get-CimInstance Win32_ComputerSystem).SystemType` = `ARM64-based PC`、`PROCESSOR_ARCHITECTURE=ARM64` |
| OS | `Windows 11 Home` / `10.0.28000`、UBR 2956（filer の出力と一致、下記） |
| rustc | 1.98.1 (aarch64-pc-windows-msvc) —— CI と同じ stable |
| 昇格 | 無し（`IsInRole('Administrators')` = False）。この 2 行はどちらも昇格を要らない |
| ネイティブ build | `cargo build --release` → PE machine **0xAA64**、21,633,024 B |
| x64 build | `cargo build --release --target x86_64-pc-windows-msvc` → PE machine **0x8664**、25,162,752 B。**この機械で組んだ 0.51.3** |
| `cargo test` | **494 passed; 0 failed**（ネイティブ ARM64、1.73s）。x64 ランナーに無い失敗は無い |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無く、スクリプトがここを `TEMP` / `TMP` に入れた） |

生の出力は `C:\dev\filer-evidence\arm-26b\`（`arm64-env.txt`、`x64-env.txt`、`x64-f12-url.txt`、
`x64-f12-os.txt`、`versions.txt`、および `grab.ps1` / `chord.ps1` / `f12.ps1`）。

### この機械で x64 をクロスビルドできる（#81 の記述の訂正）

#81 は「この機械には x64 の MSVC ツールチェーンが無い（`rustup target list --installed` は
aarch64 だけ、vswhere も無し）」と書いて、リリースの zip で代用した。**いまは揃っている。**

```
C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Tools\MSVC\14.51.36231
  bin\Hostarm64 -> arm64, x64, x86      <-- ARM64 ホストから x64 を吐ける
  lib           -> arm64, arm64ec, onecore, x64, x86
C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\um -> arm64, x64, x86
```

`rustup target add x86_64-pc-windows-msvc` のあと `cargo build --release --target
x86_64-pc-windows-msvc` が 2m31s で通り、`vcvarsall.bat` を通す必要もなかった（rustc が自分で
リンカを見つける）。**だから ARM64 レーンは「x64 でどう見えるか」を、リリースを待たずに
いまの main で試せる。**#81 は 0.49.1 の配布物で測るしかなかったが、この run の x64 は 0.51.3 で、
アーキテクチャの 2 行を含むコードそのものが手元のものである。

### 25.5 —— `filer env` の 2 行が食い違う（合格、チェック済み）

行の期待は「`OS arch` と `Process arch` が**食い違う**」。x64 build（PE machine 0x8664）を
走らせて読んだ（`x64-env.txt`）:

```
Filer
    Version      : 0.51.3
    OS           : Windows 11 Home 26H1 (build 28000.2956)
    OS arch      : aarch64      <-- 機械
    Process arch : x86_64       <-- 動いている binary
```

同じ測り方をネイティブ build でやると 2 行はそろう（`arm64-env.txt`）: `OS arch : aarch64` /
`Process arch : aarch64`。**食い違いは binary を替えたときだけ出る**ので、2 行が別のものを
見ていることが読める。`filer --version` も `filer 0.51.3 (aarch64)` と `filer 0.51.3 (x86_64)`（`versions.txt`）。

**本当にエミュレーションで動いていたことの根拠**（`Process arch` はコンパイル時定数なので、
それだけでは「x64 の binary が走った」ことしか言わない）:

```
main module    : ...\target\x86_64-pc-windows-msvc\release\filer.exe   (PE 0x8664)
読み込まれていたモジュール : C:\WINDOWS\System32\xtajit64se.dll        <-- x64 エミュレータ
IsWow64Process2(pid) -> ProcessMachine=0x0000 NativeMachine=0xAA64
```

`xtajit64se.dll` は ARM64 Windows の x64 エミュレータで、ネイティブの ARM64 プロセスには
載らない。#81 と同じく `ProcessMachine` は `0x0000` で、**エミュレーションの x64 は古典的な
WOW64 として報告されない**（`native_arch()` のコメントがそう書いているとおり）。

### 26.6 —— `<F12>` の URL でも食い違う（合格、チェック済み）

行の期待は「OS arch `aarch64`、Process arch `x86_64`」。x64 build の窓を前面に出したことを
`GetForegroundWindow` の pid で確認してから `SendInput` で `<F12>` を送り、開いた Chrome の
アドレスバーを `<C-l>` `<C-c>` で読んだ（クリップボードには先にセンチネルを置いた）。

```
https://github.com/uchmk/filer/issues/new?template=bug_report.yml&version=filer%200.51.3%20%28x86_64%29&os=OS%3A%20Windows%2011%20Home%2026H1%20%28build%2028000.2956%29%0AOS%20arch%3A%20aarch64%0AProcess%20arch%3A%20x86_64
```

`version=` が `filer 0.51.3 (x86_64)` なので、**この URL を組んだのは x64 の binary** であって、
26.5 の読み直しではない。`os=` を復号すると（`x64-f12-os.txt`）:

```
OS: Windows 11 Home 26H1 (build 28000.2956)
OS arch: aarch64
Process arch: x86_64
```

**#81 が「quietly misleads everyone」と呼んだ状態は解消している。**エミュレーションの build から
出した報告は、もう「この機械は x64 だ」と主張しない。

### #81 の残した疑問への答え: `NativeMachine` はエミュレーション側から呼んでも正しい

#81 は `IsWow64Process2` を提案しつつ、こう書いていた —— 「ただしこれは native の呼び出し元から
問うた結果でしかない。`NativeMachine` がエミュレーションの側から呼んでも偽られないことは
この run では確かめていない」。

**この run で確かめた。**上の `OS arch : aarch64` は、**エミュレーションの x64 プロセス自身が**
`IsWow64Process2(GetCurrentProcess(), ..)` を呼んで得た値である（`src/bugreport.rs:139`）。
外から native の PowerShell で問うた値（`NativeMachine=0xAA64`）と一致する。
**`GetNativeSystemInfo` が偽られる同じ経路で、`NativeMachine` は偽られない。**

### 付随して確かめたこと: エディションが `Home` と出る（#81 の指摘の修正）

#81 は `windows_name()` が `Windows 11 Core 26H1` と出すことを指摘した。0.51.3 では

```
OS : Windows 11 Home 26H1 (build 28000.2956)
```

と出る（`arm64-env.txt` / `x64-env.txt` の両方、および `<F12>` の URL）。`Win32_OperatingSystem.Caption`
= `Microsoft Windows 11 Home` と一致する。26.4 はもう x64 機で `[x]` なので触っていない。

### Proposals

4 件。うち 3 件は #81 の再掲だが、**3 件とも、この run でもう一度同じ回避を書かされた**ので、
起きたことは新しい。

1. **`filer env` に、動いている実行ファイルのパスを出す。**（新規）
   - 何が起きたか: この run は 2 つの build の出力を並べるのが仕事なのに、**出力の中に
     どちらの exe が出したかを言うものが無い。**`arm64-env.txt` と `x64-env.txt` は
     `Process arch` の 1 語しか違わず、取り違えても気づけない。実際は `Start-Process` に
     渡したパス・`MainModule.FileName`・PE ヘッダを別に記録して突き合わせた。
   - どう変えるか: `Filer` の節に `Executable : C:\...\filer.exe` を 1 行足す
     （`std::env::current_exe()`）。
   - なぜ: `filer env` は「バグ報告に貼るテキスト」で、**報告を読む人が最初に知りたいのは
     どのファイルが動いていたか**である。複数の版を並べて試す人（ここ、リリースの検証、
     ポータブル版）には毎回効く。
   - 大きさ: 1 行（`envreport.rs` の `Filer` の節）。
2. **`Process arch` に、エミュレーションであることを言わせる。**（#81 の提案 2 の再掲。データはもう揃っている）
   - 何が起きたか: 2 行が食い違うことが答えだと**知っていたから**読めた。知らない人は
     `OS arch : aarch64` と `Process arch : x86_64` を見て、これが正常なのか異常なのかを
     判断できない。26.6 の行の文言（「食い違うことが、まさに報告したい事実」）が
     TESTING.md にしか無く、出力の側にはヒントが 1 つも無い。
   - どう変えるか: `Process arch : x86_64 (emulated on aarch64)` のように、2 つが違うときだけ
     括弧で添える。`native_arch()` と `consts::ARCH` を比べるだけで、もう両方正しい。
   - なぜ: **いちばん値打ちのある 1 行**（ネイティブ版があるのに x64 版が動いている）を、
     報告を受けた側が見落とさなくなる。ARM64 機からの報告はこれから増える。
   - 大きさ: 数行。文言は持ち主の判断。
3. **`filer env` を、リダイレクトでも読めるようにする。**（#81 の提案 1 の再掲）
   - 何が起きたか: `say()` が `AttachConsole` して `CONOUT$` に書くので、この run でも
     **#81 が書いた `ReadConsoleOutputCharacterW` の 40 行（`grab.ps1`）をそのまま使うしかなかった。**
     しかもコンソールのスクリーンバッファは横 120 桁で折り返すので、`Warnings` の行が
     `max-previe` / `w` に割れて記録された（`arm64-env.txt` を見れば分かる）。
     **測り方のせいでテキストが壊れる**のは、証拠として弱い。
   - どう変えるか: `say()` で、`GetStdHandle(STD_OUTPUT_HANDLE)` が `GetFileType` で
     `FILE_TYPE_DISK` / `FILE_TYPE_PIPE` を返すときは、`AttachConsole` より先にそちらへ書く。
   - なぜ: 無人の実機テストは全部スクリプトで、版・アーキテクチャ・警告を読む唯一の口がここ。
     人にも効く（`filer env | clip` が黙って空を作る）。
   - 大きさ: 関数 1 つ。**2 回続けて同じ回避を書いたので、優先度を上げてほしい。**
4. **`<F12>` の URL をクリップボードにも置く。**（#81 の提案 3 の再掲）
   - 何が起きたか: URL を読む手が「Chrome を前面に出して `<C-l>` `<C-c>`」しか無く、
     **1 回目は静かに失敗した。**PowerShell は構造体の配列を添字で読むと**コピーを返す**ので、
     `$a[0].ki.wVk = ...` が捨てられ、`SendInput` は 4 を返しながら空のキーを送っていた
     （`chord.ps1` の冒頭にその注意を書いた）。センチネルを置いていたので気づけたが、
     置いていなければ**前のクリップボードの中身を URL として報告していた。**
   - どう変えるか: `bug-report` が URL をクリップボードにも入れ、トーストでそう言う。
   - なぜ: 26.8（ブラウザが開けないとき）の実害も小さくなる。テストの側では、ブラウザの
     キー割り当てに依存しない測り方ができる。
   - 大きさ: 1 行（`exec::set_clipboard` を `Act::BugReport` に足す）＋トーストの文言。
     既にあるものを踏むので、持ち主の判断が要る。

### 順番表（`.claude/windows-role.md`「The ARM64 lane」）

無人実行は `.claude/` への書き込みを権限で拒否されるので、変更は PR 本文の `## Queue` に書いた。
**`26.6 / 25.5, again with the fix` の行を消してよい**（この run で両方 `[x]`）。
`the test suite` の行は残す —— 毎回走らせる約束なので、この run も 494 / 0 を記録した。

## TESTING.md section 45 — 2 つのフォルダを実機で比べる（abd67e1 / 0.51.3）

Windows のセッション（`.claude/windows-role.md`、無人実行）から。担当は順番表の先頭の section 45（人の残り 9 行）。
実機の release ビルド（abd67e1、0.51.3）を隔離した設定（`R:\Temp\w45cfg`）で動かした。画面は 150%、最大化。
キーは filer の窓に PostMessage で送り、`<A-d>` は Alt を入力状態に立てて WM_SYSKEYDOWN で送った。
ホバー中の名前は `c` `f` とクリップボードで、窓が固まっていないかは `SendMessageTimeout(WM_NULL)` と
`IsHungAppWindow` で読んだ。スクリプト・生の出力・キャプチャは `C:\dev\filer-evidence\45\` にある
（`fx45.ps1` / `fxbig.ps1` がフィクスチャ、`h45.ps1` / `lib.ps1` がハーネス、`run*.ps1` / `run*.log`、
`*.png`、見るべき部分を切り出した `*-strip.png`）。昇格はしていない。

**45.3 / 45.4 / 45.6 / 45.8 / 45.9 / 45.10 / 45.12 / 45.13 にチェックを付けた。**45.11 は**落ちた**
（下の不具合 1）ので付けていない。45.10 は行の文言どおりには通ったが、出た答えが間違っている（不具合 2）。
不具合 3 件と、提案 6 件。

フィクスチャ（`R:\Temp\f45\`）は組ごとに親フォルダを分け、その中に比べる 2 つだけを置いた。`<Space>` `<Space>` で
ちょうど 2 つが選ばれ、`<A-d>` の組になる。

- `walk\walkL` / `walkR`: `f000`〜`f499.txt`（中身 `line N\n`）。右だけ `f250`（`>`）、右の `f100` は `line 100Z`
  （同じ 9 バイトで最後の 1 バイトだけ `0A`→`5A`）、右の `f400` は 21 バイト。
- `last`: 4096 バイトの `blob.bin` がオフセット `0xFFF` だけ違う（`fc /b` で確かめた）＋同一の `control.txt`。
- `kind`: 左は `thing\inner.txt`（フォルダ）、右は `thing`（ファイル）＋ `control.txt`。
- `mix`: `a-file.txt` と `b-folder\`。
- `link`: 両側に中身 `same\n` の `t1.txt` / `t2.txt`、左の `ln` → `t1.txt`、右の `ln` → `t2.txt`（相対のシンボリックリンク）。
  `link2` は同じ形で `t1` が `one\n`、`t2` が `two\n`。`link3` はディレクトリへのリンク `dl` → `d1` / `d2`。
- `big\nmL` / `nmR`: この機械の実際の `node_modules` を robocopy で 7 つ並べたもの（`/XJ /SL`）と、その複製。
  片側 **111,166 パス**。`mid\mL` / `mR` は 3 つぶんで片側 50,769 パス（2 つ合わせて 10 万を少し超える）。
- `split\one\paneL` / `split\two\paneR`: `p.txt` の長さが違い（5 B / 14 B）、`q.txt` は同一。

| 行 | やったこと | 読んだもの |
| --- | --- | --- |
| 45.3 | `walk` で `<A-d>`、`n` ×4、`N` ×3 | 表示は 49 行。`n` のたびに最下行が `~ f100.txt 9 B → 9 B` → `> f250.txt 9 B` → `~ f400.txt 9 B → 21 B`、4 回目は動かずトースト `At the last difference`。`N` で最上行が `f250` → `f100`、3 回目は動かず `At the first difference`（`a1`〜`a7`、`A-strip1.png`）。表示範囲はカーソルを画面に入れる規則（`diff_tree` の `offset`）がそのまま決めるので、どの行が見えているかがカーソルの位置を言う。間の 497 行の `=` には一度も止まらない |
| 45.4 | 続けて `G`、`gg` | `G` で `f451`〜`f499` が見え最下行 `f499` に色、`gg` で最上行 `f000`（`a8-G.png`、`a9-gg.png`、`A-strip2.png`） |
| 45.6 | `walk` の `f100`（最後の 1 バイトだけ違う）と、`last` の `blob.bin` | `~ f100.txt  9 B → 9 B`、`~ blob.bin  4.0 K → 4.0 K`、同一の `= control.txt` はそのまま（`a1-n.png`、`b2-last-top.png`） |
| 45.8 | `kind` | `~ thing/`、`< thing\inner.txt 7 B`、`= control.txt`（`b1-kind-top.png`） |
| 45.9 | `mix` で 2 つ選んで `<A-d>` | トースト `Compare: compare two files, or two folders — not one of each`、ビューは開かない、直後の WM_NULL に応答（`b3-mix-toast.png`、`runB.log`） |
| 45.10 | `big` と `mid` で `<A-d>`、250 ms ごとに WM_NULL（上限 250 ms）を 2 分間 | 2 組で 913 回、**失敗 0 回、最も遅い往復 7 ms**。0.2 秒後の画面は `Comparing…`。答えは `mid` で 1〜2 秒（`d4-mid-1s.png` は `Comparing…`、`2s` で結果）。フッタは `big` が `5000 only left · 0 only right · 0 differ · 0 match · cut short`、`mid` が `1538 only left · 0 only right · 0 differ · 3462 match · cut short`（`C-strip.png`、`D-strip.png`、`runC.log`、`runD.log`）。**両方とも中身は同一なので、この答えは間違い**（不具合 2） |
| 45.12 | `split\one` で `paneL` に立ち、分割しないで `<A-d>`（対照）→ `<C-w>` → 新しいペインで `h`、`two` に入り `paneR` に立って `<A-d>` | 対照はトースト `Compare: split the view, or select exactly two files`（`c3-nosplit.png`）。分割後は見出し `paneL ↔ paneR — n/N differences, q to close`、`~ p.txt 5 B → 14 B`、`= q.txt`、フッタ `0 only left · 0 only right · 1 differ · 1 match`（`c4-split-compare.png`）。どちらも選択はしていない |
| 45.13 | `walk` のビューを開いたまま `c` `f` → `q` → `c` `f` → `<A-d>` → `c` `f` → `<Esc>` → `c` `f`。そのあと `text\a.txt` と `b.txt` で `<A-d>` | 開いている間はコピーされず（`<no copy>`）、`q` の後は `walkR`、開き直すとまた `<no copy>`、`<Esc>` の後は `walkR`（`runB.log`）。キーが一覧に戻ったこと＝閉じたこと。ファイル同士は見出し `a.txt ↔ b.txt`、左右に行番号 1〜4、2 行目だけ `beta` / `BETA` が色付き（`b10-files-top.png`） |

補足（チェックの対象外）: 45.7 は自動テスト済みだが、65 MB（67,108,865 バイト）の同一ファイル 2 つで
`? big.bin`、`0 only left · 0 only right · 0 differ · 0 match · 1 too big to read` と出た（`G-strip.png`、`runG.log`）。

### 不具合 1: シンボリックリンクは指す先ではなく、指す先の中身で比べられる（45.11 が落ちる）

- `link`（左 `ln` → `t1.txt`、右 `ln` → `t2.txt`、`t1` と `t2` の中身は同じ）で `<A-d>`: **`= ln`**、フッタ
  `0 only left · 0 only right · 0 differ · 3 match`（`b4-link-top.png`、`B-strip.png`、`B-foot.png`）。
- `link2`（指す先の中身が違う）では `~ ln  0 B → 0 B`（`b5-link2-top.png`）。違うと出るのは**中身が違うから**で、
  指す先が違うからではない。大きさは `symlink_metadata` の 0 B。
- 原因（`src/diff.rs`）: `walk` は `symlink_metadata` でリンク自身を記録する（長さ 0）。両側とも 0 なので
  `same_bytes` に回り、そこで `File::open` が**リンクをたどって**指す先を開く。`walk` の doc コメントは
  「リンクの行き先だけが違う 2 つのツリーは異なると読むべき」と書いていて、実装がそれに合っていない。
- 直すなら: 両側がシンボリックリンクなら `read_link` の結果を比べ、中身は読まない。片方だけがリンクなら `~`。
- **直していない。**45.11 は付けていない。

### 不具合 2: 走査の上限 10 万が左右で共有されていて、同一の 2 つのツリーに偽の差分が並ぶ

- `big`（同一の 2 つ、片側 111,166 パス）: `5000 only left · 0 only right · 0 differ · 0 match · cut short`。
  **先頭から 5000 行すべてが `<`**（`p1/`、`p1\.bin/`、…）。右は 1 パスも読まれていない。
- `mid`（同一の 2 つ、片側 50,769 パス）: `1538 only left · … · 3462 match · cut short`。例えば
  `< p1\.bin\loose-envify 387 B` と出るが、右にも同じファイルがある（両側 387 バイト、SHA-256 の先頭
  `D60A3F79B1480123` が一致。`ondisk.log`）。
- 原因: `compare_trees` の `budget` は 1 つで、左の `walk` が使い切ると右の `walk` は残りしかもらえない。右で
  読まれなかったパスは、左にだけあるものとして `LeftOnly` になる。`cut short` は付くが、行そのものは
  「左にだけある」と断言していて、**フッタの `cut short` がその行を疑えとは言っていない**。
- 45.10 の文言（「答えが出るか、打ち切ったと述べる。固まらない」）は満たすのでチェックは付けた。
  ただし 1 つ目の組が一番ありそうな使い方（`node_modules` 同士）で、答えが全部間違っている。
- 直すなら: 左右に半分ずつの予算を持たせる、または両側を同じ相対パス順に歩いて同じところで止める。
  止まった後は、片側しか読んでいない範囲の行を出さない（出すなら `?`）。

### 不具合 3: ディレクトリへのシンボリックリンクが「大きすぎて読めない」に数えられる

- `link3`（`dl` → `d1` / `d2`）: 行は `? dl`、フッタは `0 only left · 0 only right · 0 differ · 4 match ·
  **1 too big to read**`（`b6-link3-top.png`、`B-foot.png`）。大きさは 0 バイト。
- 原因: ディレクトリのリンクは `dir` が偽・長さ 0 で `same_bytes` に回り、`File::open` がディレクトリを開けず
  `None` → `Unread`。フッタは `Unread` を全部 `too big to read` と呼ぶ。
- 不具合 1 を `read_link` で直せば、これも一緒に直る。

### 45.11 を付けなかった理由

上の不具合 1 のとおり、行の期待値（リンクの行が「異なる」と読める）に反する結果が、指す先の中身が同じ組で
確実に出る。中身が違う組（`link2`）では `~` になるが、それはリンクではなく中身を比べた結果なので、代わりにならない。

### filer は何もしていないときも 1 コアを使い続ける（section 45 の外）

- 45.10 で「CPU 時間が止まったら走査が終わった」と読むつもりだったが、2 分間一度も止まらなかった。
  切り分けると、**起動直後の何もしていない一覧で 1.0 CPU 秒/秒**。ビューを開いても閉じても、窓を
  **最小化しても**変わらない（`runD.log`、`runE.log`）。
- スレッドごとに見ると、起動から約 130 ms 後に作られた 1 本（起動時の最初のスレッドではない）が
  5 秒間で 5.0 秒を使い、他の 38 本は 0（`runF.log`）。スレッド名は取れなかった（`GetThreadDescription` が
  全部失敗）。どのスレッドかは突き止めていない。
- 手元の見立て: 最小化しても下がらないので、描画の要求（`request_repaint`）の回しっぱなしだけでは説明が
  付かないかもしれない。どちらにしてもノート PC ならバッテリーに効く。
- **直していない。**原因の特定も、ここではしていない。

### Proposals

6 件。

1. **ツリーの比較は、最初の差分にカーソルを置いて開くべき。**
   - 起きたこと: `walk` は 500 行中 497 行が `=` で、開いた画面（49 行）には差分が 1 つも無い。フッタを読んで
     初めて差分があると分かり、`n` を押すまで何も見えない（`a0-open.png`）。
   - 変えるべきこと: 差分が 1 つでもあれば、カーソルを最初の差分に置いて開く。加えて `=` の行を隠す切り替え
     （例えば `z`）があると、`node_modules` のような大きな組で差分だけを歩ける。
   - 理由: 比べる目的はほぼ常に「どこが違うか」で、一致した行を見たい人は `gg` で戻れる。
   - 大きさ: 開くときに `FindArrow` を 1 回。隠す方はキー 1 つとフィルタ済みの行の添字。
2. **「先頭／末尾の差分です」のトーストは、前のものを置き換えるべき。**
   - 起きたこと: `n` で末尾に当たってから `N` で先頭まで戻ると、`At the last difference` と
     `At the first difference` が同時に 2 つ積まれていた（`a7-NNN-start.png`）。
   - 変えるべきこと: 同じ種類のトーストは新しいものが古いものを消す。
   - 理由: 矛盾した 2 つの言葉が並ぶと、今どちらの端にいるのかがトーストから読めない。
   - 大きさ: トーストに種類の鍵を 1 つ。
3. **ツリーの行の区切り文字を揃えるべき。**
   - 起きたこと: `thing\inner.txt` と `thing/` が同じ画面に並ぶ（`b1-kind-top.png`）。パスの中は `\`、
     フォルダの印は `/`。
   - 変えるべきこと: Windows ではフォルダの印も `\`（またはアイコン）にするか、全部 `/` で表示する。
   - 理由: `/` の付いた行をパスとしてコピーすると Windows のパスとして半端になる。見た目の話でもある。
   - 大きさ: 描画の 1 行（`#[cfg(windows)]` か `MAIN_SEPARATOR`）。
4. **シンボリックリンクは `read_link` で比べ、行にリンク先を出すべき。**
   - 起きたこと: 不具合 1 と 3。加えて `~ ln 0 B → 0 B` は、何が違うのかを何も言っていない。
   - 変えるべきこと: リンクの行は `~ ln  → t1.txt | → t2.txt` のように行き先を出す。
   - 理由: リンクの差分で知りたいのは行き先そのもの。
   - 大きさ: `walk` でリンクを記録するとき `read_link` を 1 回、行の型に 1 項目、描画に 1 分岐。
5. **打ち切ったときは、どちら側をどこまで読んだかを言うべき。**
   - 起きたこと: 不具合 2。`cut short` は 1 語で、右側が 0 パスしか読まれていないことは画面のどこにも無い。
   - 変えるべきこと: 予算を左右に分けたうえで、フッタに `cut short: left 50,000 of ?, right 50,000 of ?` のように
     出す。読み切れなかった範囲の `<` / `>` は出さない。
   - 理由: `node_modules` 同士の比較は、この機能が一番頼られそうな場面で、今はそこで一番間違える。
   - 大きさ: `compare_trees` の予算の分け方と、`Outcome::Tree` に数を 2 つ。
6. **比較の窓に「何が選ばれて比べられているか」をフルパスで出すべき。**
   - 起きたこと: 見出しは `walkL ↔ walkR`、`paneL ↔ paneR` と名前だけ。45.12 で分割したペインが本当に
     `split\two\paneR` を比べているのかは、中身（`5 B → 14 B`）から逆算するしかなかった。
   - 変えるべきこと: 見出しの下に 2 つのフルパスを 1 行ずつ（長ければ中を省略）。
   - 理由: 同じ名前のフォルダ（`node_modules` と `node_modules`）を比べるのが普通なので、名前だけでは
     どちらがどちらか分からない。
   - 大きさ: 描画に 2 行。

### ハーネスについて

- PowerShell の関数名に `Compare` を使うと、組み込みの別名 `Compare-Object` が先に解決されて
  `ReferenceObject` が無いと怒られる。`AltD` に改名した。
- `crop.ps1` に配列を渡すときは `pwsh -File` ではなく `&` で呼ぶ（`-File` は配列を 1 つの文字列にする）。
- 一時ディレクトリは R: で足りた（ごみ箱を使わない節なので、section 12 のような C: への退避は要らない）。
  `big` / `mid` の木は合わせて約 2.6 GB。
- 45.10 の「固まらない」は WM_NULL の往復で測ったので、**描画が進んでいるか**までは言えない。ただ
  `d4-mid-1s.png` の `Comparing…` が `2s` で結果に変わっていて、その間も往復は 7 ms 以内だった。

## TESTING.md 41.8 — 6 つのリリースバイナリを ARM64 機で読んだ（f2df71d / 0.52.1、ARM64 レーン）

ARM64 レーンの 3 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
順番表の先頭「41. spot's four providers / 41.8」が担当。

**41.8 は合格したのでチェックを付けた（9 / 14 → 10 / 14）。**順番表は「ARM64 の半分だけ」と
書いていたが、**6 つ全部をこの機械で読めた。**`executable()`（`src/spot.rs:472`）はマジック
バイトだけを見てホストに依存しないので、ELF も Mach-O もこの機械で正しく読める ——
**走らせる必要が無いのが、この行の性質。**そして #79（x64 機の 41 節）がチェックを
見送った理由（`macos-x64` の成果物が存在しない）は、v0.49.1 で解消している。

### この機械

| | |
| --- | --- |
| 機械 | `(Get-CimInstance Win32_ComputerSystem).SystemType` = `ARM64-based PC`、`PROCESSOR_ARCHITECTURE=ARM64` |
| OS | `Windows 11 Home 26H1 (build 28000.2956)`（`filer env` の出力） |
| rustc | 1.98.1 (48a229cea 2026-09-01) / aarch64-pc-windows-msvc —— CI と同じ stable |
| 昇格 | 無し（`IsInRole('Administrators')` = False）。41 節に昇格の要る行は無い |
| ネイティブ build | `cargo build --release` → PE machine **0xAA64**、21,644,288 B。`filer env` は `OS arch : aarch64` / `Process arch : aarch64` |
| ConPTY | `fetch-conpty.ps1` → `ConPTY 1.24.260710001 (arm64)` |
| `cargo test` | **499 passed; 0 failed**（ネイティブ ARM64、2.63s）。x64 ランナーに無い失敗は無い |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |

生の出力は `C:\dev\filer-evidence\arm-41\`（`panel-1.txt` 〜 `panel-6.txt`、`spot-41-8.png`、
`arm41-local-env.txt`、および `post41.ps1` / `find41.ps1` / `drive41.ps1` / `keys41.ps1` /
`focus41.ps1` / `harness-keymap.toml`）。

### 41.8 —— 6 つのバイナリ（合格、チェック済み）

`gh release download v0.49.1` で 6 つの成果物を落とし、展開して 1 つのフォルダに並べ、
**先に自分でヘッダを読んでから**（PowerShell で `e_machine` / `cputype` / COFF `Machine` を
直接）、filer の `<Tab>` の答えと突き合わせた。名前は並び順のために付け替えたが、
`executable()` は拡張子を見ないので答えは変わらない（`5-windows-arm64.exe` の
`Mime` が PE を当てているのが、その裏返しの証拠）。

| 成果物 | 自分で読んだヘッダ | filer の `Format` | filer の `Architecture` |
| --- | --- | --- | --- |
| `filer-v0.49.1-linux-arm64.tar.gz` | ELF class=2 `e_machine=0x00B7` | ELF 64-bit | **aarch64** |
| `filer-v0.49.1-linux-x64.tar.gz` | ELF class=2 `e_machine=0x003E` | ELF 64-bit | **x86_64** |
| `filer-v0.49.1-macos-arm64.tar.gz` | Mach-O `0xFEEDFACF` `cputype=0x0100000C` | Mach-O 64-bit | **aarch64** |
| `filer-v0.49.1-macos-x64.tar.gz` | Mach-O `0xFEEDFACF` `cputype=0x01000007` | Mach-O 64-bit | **x86_64** |
| `filer-v0.49.1-windows-arm64.zip` | PE `Machine=0xAA64` | PE32+ | **aarch64** |
| `filer-v0.49.1-windows-x64.zip` | PE `Machine=0x8664` | PE32+ | **x86_64** |

6 行とも、成果物の名前のトリプル（`arm64` / `x64`）と一致する。各 capture には `Name` と
`Path` の行が入っているので、**どの答えがどのファイルのものかを取り違えようが無い**
（`panel-N.txt` にそのまま残してある）。Mach-O の 2 つは thin なので `Slices` の行は出ず、
これも期待どおり。`spot-41-8.png` は 6 番目（`6-windows-x64.exe`）のパネルで、
**同じ画面の hex プレビューに `50 45 00 00 64 86 00 00` が出ている** —— PE 署名と
`Machine=0x8664` が、パネルの `x86_64` の隣に生のバイトで並んでいる。

`<Tab>` のあとパネルが出るまでにウィンドウが止まっていないことは、`WM_NULL` を
`SendMessageTimeout` で投げて測った（6 回とも 0〜10 ms で返事）。

### #79 が残した「`macos-x64` が出たことがない」は解消している

#79（52507ef / 0.47.25）は、最新リリース v0.47.10 のアセットが 5 つしか無く、
`macos-13` のランナーが取られずに `macos-x64` のジョブが queued のままだと書いた。
**v0.49.1（2026-09-29）には 6 つそろっている**（`gh release view --json assets`）。

```
filer-v0.49.1-linux-arm64.tar.gz    11,760,260
filer-v0.49.1-linux-x64.tar.gz      12,601,090
filer-v0.49.1-macos-arm64.tar.gz     8,908,788
filer-v0.49.1-macos-x64.tar.gz       9,589,411     <-- #79 の時点では存在しなかった
filer-v0.49.1-windows-arm64.zip     10,872,211
filer-v0.49.1-windows-x64.zip       11,703,838
```

CLAUDE.md の「成果物は 6 つ」と 41.8 の「six release binaries」は、いまは事実に合っている。
**#79 のこの節は閉じてよい。**

### チェックしなかった行（41 節の残り 4 件）

- **41.5 / 41.6（メモ帳の CRLF / UTF-16 LE 保存）**: 取れなかった。**この run では
  そもそもキーが画面に届かない**（下の「スクリーンセーバー」の節）。メモ帳の「名前を付けて
  保存」はダイアログと文字コードのドロップダウンを操るもので、`PostMessage` では駆動できない。
  バイト列を自分で書くのは #79 が既にやっていて、**それは「メモ帳で保存した」の代わりに
  ならない**（行がそう書いている）ので繰り返さなかった。
- **41.12（キーの列が値の列にはみ出していないか）**: 見た目の行。代わりに読めるものを探したが、
  `spot-41-8.png` で言えるのは「Executable と File の 2 つの節で重なっていない」までで、
  行が求めているのは **every new section**（Archive / Text / Document / Executable）。
  4 つ全部を 1 枚に出せないので、画素の位置を測っても行の主張を担げない。持ち主の目に残す。
- **41.14（遅いネットワークドライブ）**: この機械にネットワークドライブが無い。
  共有を作るには昇格が要り、この run は昇格していない。#79 と同じ理由で残る。

### 見つけたもの（どれも直していない）

#### 無人の run では、画面がスクリーンセーバーに取られていて `SendInput` が 1 つも届かない

- **実測**: `Focus-Proc`（#81 / #84 が使った `SetForegroundWindow` の手）が 40 回とも失敗した。
  調べると `GetForegroundWindow()` が **0 を返し続けていた**。`OpenInputDesktop` で入力
  デスクトップの名前を読むと `Screen-saver`、`SystemParametersInfo(SPI_GETSCREENSAVERRUNNING)`
  も `True`。走っていたのは `C:\WINDOWS\ASUS\ScreenSaver\OLED Care Screensaver.scr`（この
  ノート PC は OLED なので、持ち主が入れたものではなく機械に付いてくるもの）。
  `LogonUI` は**無い** —— つまり `windows-role.md` の「Unattended runs」が挙げている
  「画面がロックされたら」の検査（`Get-Process LogonUI`）**には引っかからない。**
- **なぜ困るか**: 無人の run は定義上ずっと放置されているので、**これは例外ではなく通常状態**。
  ARM64 レーンの 1 本目・2 本目がたまたま通ったのは、走った時刻が持ち主の操作の直後
  だっただけに見える。`SendInput` を前提に書かれた測り方（`windows-role.md` の
  「Measure before you call it a look」の表のうち、クリック・ホバー・ドラッグ・
  `<F12>` の行）は、**無人ではどれも静かに空振りする。**しかも `SendInput` は
  「4 送った」と成功を返すので、**センチネルを置いていないと気づけない。**
- **この run はどう逃げたか**: キーを `PostMessage(WM_KEYDOWN / WM_CHAR / WM_KEYUP)` で
  filer のウィンドウへ直接入れた。フォアグラウンドも入力デスクトップも要らず、
  **他のウィンドウにキーが漏れない**（2026-09-30 のクリップボード事故の再発も防げる）。
  修飾キーだけは乗らないので、`C`（`copy all`）と `<A-j>`（`arrow 1`）を、隔離した
  `FILER_CONFIG_HOME` の `[[spot.prepend_keymap]]` で `a` と `n` に置いた
  （`harness-keymap.toml`）。**測っている対象は `Architecture` の値なので、どのキーで
  コピーさせたかは主張に影響しない。**
- **`PrintWindow` はスクリーンセーバー下でも通る**（`PW_RENDERFULLCONTENT = 2`）。
  `spot-41-8.png` はその状態で撮ったもので、wgpu のサーフェスもちゃんと写っている
  （標本した色が 547 色。真っ黒なら 1 色になる）。**スクリーンショットは無人でも証拠になる。**

#### `Process.MainWindowHandle` が winit のイベント用ウィンドウを指すことがある

- **実測**: `Start-Process -PassThru` して `$p.Refresh()` で待つと、`MainWindowHandle` が
  `1902666` を返した。そこへキーを `PostMessage` しても**何も起きない**（6 回とも
  センチネルのまま）。ウィンドウを列挙すると、その HWND の正体はこれ:

```
330566 |vis=True |class=Window Class                  |title=Filer: C:\...\rel41\bins   <-- 本物
1902666|vis=True |class=Winit Thread Event Target     |title=                           <-- これを掴んでいた
2033546|vis=False|class=wgpu Device Class 7ff696a211b0|title=wgpu Device Class ...
```

- winit は本物のウィンドウより先に「Thread Event Target」ウィンドウを作り、**それが
  `IsWindowVisible` で真になる。**`MainWindowHandle` は「オーナーの無い最初の可視な
  トップレベル」を返すので、タイミング次第でこちらを掴む。
- **なぜ困るか**: `windows-role.md` の「Measure before you call it a look」が、
  **「the list went somewhere」の proxy として `(Get-Process filer).MainWindowTitle` を
  名指ししている。**この HWND を掴んだときの `MainWindowTitle` は**空文字列**で、
  「リストがどこにも行かなかった」と読めてしまう。**空振りが失敗に見えない**形の測り方。
- **どうすべきか**: 役割定義の proxy を、クラス名とタイトルで選ぶ形に直す
  （この run の `find41.ps1`: `class == "Window Class"` かつ `title` が `Filer:` で始まる）。
  filer 側の話ではないので、直すのは `.claude/windows-role.md`。

### Proposals

1. **無人の run のために、キーを入れる口を filer 自身に用意してほしい。**
   - 何が起きたか: この run の測定は「`<Tab>` を押してパネルを読む」だけなのに、そこへ
     到達するのに `SendInput` が死んでいることの発見、入力デスクトップの調査、
     `PostMessage` への書き換え、修飾キーを避けるための harness keymap、
     winit のイベントウィンドウの切り分け —— **測る前の足場作りが仕事の大半になった。**
     しかもその足場は毎回 run ごとに作り直されていて、`R:` や `filer-scratch` に
     `grab.ps1` / `input.ps1` / `chord.ps1` / `post41.ps1` と似たものが溜まっている。
   - どう変えるか: `filer --keys "<Tab>a"` のような、**keymap を通してキー列を流し込む
     引数**（か、環境変数で受ける待ち受け）。押した結果は既にクリップボードから読めるので、
     入り口だけあればよい。`Key::parse` は既にあり、`handle_input` も `pub(crate)` になっている。
   - なぜ: 実機の run が毎回作っている Win32 の足場が丸ごと消える。**そして「キーが
     届かなかった」と「届いたが何も起きなかった」を取り違える事故が構造的に無くなる** ——
     これは `windows-role.md` が「一番の危険」と呼んでいるものの一種。
   - 大きさ: 設計の判断が要る（テスト用の口をリリースするバイナリに置いてよいか、
     置くならどう危なくないようにするか）。持ち主に決めてもらう類。

2. **`windows-role.md` の「画面がロックされた」の検査に、スクリーンセーバーを足してほしい。**
   - 何が起きたか: 上の節のとおり。`Get-Process LogonUI` は無人で放置された機械の
     **普通の状態を捕まえられない。**この run は `SendInput` が 40 回失敗してから気づいた。
   - どう変えるか: 役割定義の検査を `OpenInputDesktop` の名前が `Default` であること
     （あるいは `SystemParametersInfo(SPI_GETSCREENSAVERRUNNING)` が偽であること）に広げ、
     そうでないときは **`SendInput` を使わず `PostMessage` で測る**と書く。
   - なぜ: 無人の run は全部この状態で始まる。**気づかないまま「押したのに何も起きない」を
     バグとして報告する run が、いつか出る。**
   - 大きさ: `.claude/windows-role.md` に数行。

3. **`filer env` をパイプで読めるようにしてほしい。**（#81 の提案 3・#84 の提案 3 の再掲、3 回目）
   - 何が起きたか: この run も `grab.ps1`（`ReadConsoleOutputCharacterW` でコンソールの
     画面バッファを読み返す）から始めた。そして**同じ壊れ方をもう一度見た** ——
     `arm41-local-env.txt` の警告行が、コンソールの幅で折り返されて
     `max-previe` / `w` に割れている。画面バッファを読む以上これは避けられない。
   - どう変えるか: `say()`（`src/main.rs:51`）で、`GetStdHandle(STD_OUTPUT_HANDLE)` が
     `GetFileType` で `FILE_TYPE_DISK` / `FILE_TYPE_PIPE` を返すときは、`AttachConsole` より
     先にそちらへ書く。
   - なぜ: 無人の実機テストは全部スクリプトで、版・アーキテクチャ・警告を読む唯一の口がここ。
     人にも効く（`filer env | clip` が黙って空を作る）。
   - 大きさ: 関数 1 つ。**3 回続けて同じ回避を書いた。**

4. **spot の `copy all`（v0.52.0）は残してほしい —— 実機テストの測定手段として一番強い。**
   - 何が起きたか: 提案というより、この run の土台の報告。`C`（`copy all`）は
     **1 キーでパネル全体がラベル付きで取れて、`Name` と `Path` が頭に付く。**
     おかげで「6 つの答えのうち、どれがどのファイルのものか」を取り違えずに済んだ。
     #79 は行ごとに `y` を打って添字を数えており、41.13 が警告しているとおり
     セクションが増えれば添字はずれる。**`copy all` はその事故の種を消している。**
   - どう変えるか: 変えるべきところは無い。強いて足すなら
     `Act::Copy(CopyWhat::All)` が `spot_sections()` の結果を 2 回歩いているので
     1 回にできる（`src/app.rs:2053-2055`）。
   - なぜ: 実機の run が「パネルに何が出ていたか」を文字で持ち帰る唯一の手段。
   - 大きさ: 無し（維持の要望）。整理を足すなら数行。

### 順番表（`.claude/windows-role.md`「The ARM64 lane」）

無人実行は `.claude/` への書き込みを権限で拒否されるので、変更は PR 本文の `## Queue` に書いた。
**`41. spot's four providers` の行を消してよい**（41.8 をチェック済み。同節に残る
41.5 / 41.6 / 41.12 / 41.14 は、それぞれ「メモ帳」「見た目」「ネットワークドライブ」で
**この機械では取れない** —— 上の節に理由を書いた。ARM64 の話ではないので、この行を
ARM64 の順番表に残しても次の run が同じ 4 件を見送るだけになる）。
`the test suite` の行は残す —— 毎回走らせる約束なので、この run も 499 / 0 を記録した。

## リリースの zip そのもの — ARM64 機で展開して、中身を読み、それを動かした（c8db529 / 0.53.1、ARM64 レーン）

ARM64 レーンの 4 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
順番表の先頭「**the release zip itself**」が担当。「Nobody has run the ARM64 zip yet」と
書かれていた行で、**この run で走らせた。**

**チェックは 1 つも付けていない。**この節が求める 3 行（1.31 / 1.32 / 1.34）は x64 機で既に
`[x]` で、レーンの規則どおり ARM64 の結果はここに書く。**そして zip の中身そのものには
TESTING.md に対応する行が無い**（下の「見つけたもの」1 番）。`make-testcheck -- --check` と
`make-keycheck -- --check` は両方 `in sync`（139 / 392、243 / 247）で、ずれは無い。

### この機械

| | |
| --- | --- |
| 機械 | `PROCESSOR_ARCHITECTURE=ARM64`、`Win32_ComputerSystem.SystemType` = `ARM64-based PC` |
| OS | `Windows 11 Core 26H1 (build 28000.2956)`（リリース版 `filer env` の出力） |
| rustc | 1.98.1 (48a229cea 2026-09-01) / aarch64-pc-windows-msvc —— CI と同じ stable |
| 昇格 | 無し（`IsInRole('Administrators')` = False）。この節に昇格の要る行は無い |
| `cargo test` | **502 passed; 0 failed**（ネイティブ ARM64、2.68 s、c8db529）。x64 ランナーに無い失敗は無い |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |
| 入力 | スクリーンセーバーが入力デスクトップを持っていた（`OpenInputDesktop` の名前 = `Screen-saver`、`SPI_GETSCREENSAVERRUNNING` = True、`LogonUI` は無し）。#88 と同じなので、最初から `PostMessage` で入れた |

生の証拠は `C:\dev\filer-evidence\arm-zip\`（`shots\01`〜`15`、`pty.log` 93 KB、
`rel-env.txt`、`rel-version.txt`、`harness-keymap.toml` / `harness-filer.toml`、
および `start.ps1` / `step-*.ps1` / `lib.ps1` / `shot.ps1`）。

### zip の中身（合格）

`gh release download v0.49.1 --pattern 'filer-v0.49.1-windows-arm64.zip'`。
zip 自体の SHA-256 は `AE93440250A7E86B0C3FAB8FED570E776F44F5325596051649767CCB2EE9D975`。

- **1 つのフォルダに 4 ファイル。**`filer-v0.49.1-windows-arm64\` の直下に
  `filer.exe` 21,639,168 B / `conpty.dll` 106,336 B / `OpenConsole.exe` 1,120,056 B /
  `ConPTY-LICENSE.txt` 1,475 B。ほかに何も入っていない。
- **`ConPTY-LICENSE.txt` の版は、ピン止めと一致する。**本文は
  「version 1.24.260710001」と書いており、`scripts/fetch-conpty.ps1` の `$version` は
  **リリースのタグ `v0.49.1` でも、いまの `main`（c8db529）でも** `1.24.260710001`。
- **3 つの PE はすべて ARM64。**`e_lfanew` から COFF ヘッダを直接読んで、
  `filer.exe` / `conpty.dll` / `OpenConsole.exe` とも署名 `0x00004550`・Machine `0xAA64`。
  `conpty.dll` と `OpenConsole.exe` のファイル版はどちらも `1.24.2607.10001`。
- **同梱物はピン止めしたパッケージとバイト単位で同じ。**`fetch-conpty.ps1` を空の
  ディレクトリに走らせて SHA-256 を突き合わせた。
  `conpty.dll` = `DB3D173640B172BAFD42D5B541B638A9AEEC1C7D0E40DD636BF02822A32C912C`、
  `OpenConsole.exe` = `ED7622FD0D3BEDC9AB9F122F5E58EDF0DEF9E7999224F52DD395BA9F54EDBE09`、
  4 つのハッシュが 2 組とも一致。**リリースが古い ConPTY を抱えていない**ことは、
  版の文字列ではなくこれで言える。

### それを動かした（ローカルビルドではない）

`$env:FILER_CONFIG_HOME` を隔離し、展開した `filer.exe` をそのまま起動した。

- `filer --version` → `filer 0.49.1 (aarch64)`。
- `filer env` → `Version 0.49.1` / `OS arch : aarch64` / **`Process arch : aarch64`**。
  レーンの規則が要求する「ネイティブの版を動かしているか」の確認（`rel-env.txt`）。
- **同梱の ConPTY が実際に使われている。**動いているプロセスのモジュールに
  `...\filer-v0.49.1-windows-arm64\conpty.dll`（`1.24.2607.10001`）が入っており、
  子プロセスは `...\filer-v0.49.1-windows-arm64\OpenConsole.exe --headless --width 80
  --height 24 --signal 0x7f0 --server 0x794`。**Windows 内蔵のものではなく、zip の中の 2 つ。**
- ペインのシェルは `pwsh`（1.32 が pwsh のプロンプトを名指しするので、隔離した
  `filer.toml` に `[term] shell = "pwsh"` と書いた）。`pwsh.exe` も `lazygit.exe` も
  この機械では ARM64 ネイティブ（lazygit の PE Machine = `0xAA64`）。
- **`[term]` を `yazi.toml` に書くと無視される。**最初そう書いて、`filer env` の
  Warnings が `[term] belongs in filer.toml and was ignored` と教えてくれた。
  警告が無ければ「pwsh で試した」と思い込んだまま 5.1 を測っていた —— 25 節・33 節が
  見ている警告表示が、実際にこの run を 1 回救っている。

### 1.34 —— 起動時にメニューが開かない（ARM64 で合格、チェックは付けない）

ペインで `lazygit`。**1 回目は lazygit 自身の「Thanks for using lazygit!」の案内**が出た
（版ごとに 1 回出るもので、古い ConPTY が作ったコピーメニューではない。`03-lazygit-started.png`）。
`Esc` で閉じ、`q` で抜け、**もう一度起動した 2 回目**（`06-lazygit-second-start.png`）が
1.34 の言う姿 —— ステータス / 差分 / コマンドログの通常画面で、**メニューは開いていない。**
フッタも `コミット: c ┆ スタッシュ: s ┆ リセット: D ┆ キーバインディング: ?` の通常表示。

裏付けとして、`pty.log` に **1.33 の署名**がそのまま出ている:

```
   62156 in reply  \e[?6c
```

DA1 への応答が filer 自身の `\e[?6c`。Windows 内蔵の ConPTY が答えていれば
`\e[?61;6;7;22;23;24;28;32;42c` になるはずで、**1.34 の不具合の原因だったのがこれ。**
つまり zip から起動しただけで同梱 ConPTY の経路に乗っている。

### 1.31 —— `?` で開いたキー一覧が `Esc` で閉じる（ARM64 で合格、チェックは付けない）

- `?` → キー一覧が開く（`07-keybindings.png`: `--- ローカル ---`、
  `<ctrl+o> パスをクリップボードにコピー`、`<space> ステージ`、フッタが
  `実行: <enter> ┆ 閉じる/キャンセル: <esc>`）。
- `Esc` → **閉じる**（`08-after-esc.png`: ステータス / 差分の通常画面に戻り、フッタも通常表示）。
  `pty.log` には `in key \e[27;1;27;1;0;1_` —— v0.48.6 が入れた win32-input-mode の
  ESC 押下で、平の `\e` ではない。
- **画面を見ない形の裏取り**も取った。キー一覧が開いたままなら `q` は一覧を閉じるだけで
  lazygit は残り、次に打った行はシェルに届かない。実際には `q` で lazygit が終わり、
  続けて打った `ni …` が**ファイルをディスクに作った**
  （`zip\repo\esc-closed-the-key--list.txt`、`10-file-made.png` に一覧と `ni` の出力の両方）。
  **`Esc` が閉じていなければ、このファイルは無い。**

### 1.32 —— pwsh のプロンプトで `abc` と打って `Esc`（ARM64 で合格、チェックは付けない）

`11-abc-typed.png` にプロンプトの `abcc`（`c` が 2 つなのは下の harness の話）、
`Esc` のあと `12-after-esc.png` では**プロンプトの右に何も残っていない。**
こちらもディスクで裏を取った: 行が空になっていなければ次の行は `abcni …` になって
コマンドとして解決しない。実際には `line-was-cleared.txxt` が作られた。

### 取れなかった行

- **1.35 / 1.36（`<C-S-t>` の End the shell? の確認）は、リリースには無い機能。**
  v0.52.0 で入ったもので、最新リリースは **v0.49.1**（2026-09-29）。この節は
  「リリースの zip を動かす」節なので、手元ビルドに持ち替えてまで取らなかった。
  1 節の残りとして次の run（順番表の「1. the terminal pane, again」）に回る。
- 昇格の要る行は、この節には無い。

### 見つけたもの（どれも直していない）

#### 1. リリースの zip を確かめる行が TESTING.md に無い

順番表の先頭にありながら、**チェックできる行がどこにもない。**41.8 は「6 つのバイナリを
filer の spot で読む」行で、zip の**中身**（4 ファイル・ライセンスの版・ConPTY の
アーキテクチャ）には触れていない。今回読んだものは、どれも「テキストかファイルの状態」で、
人の目を要しない。**行が無いので、この run の合格は QA-REPORT にしか残らない。**
TESTING.md に節を足すべき（下の Proposals 1）。

#### 2. `PostMessage` で打った文字が、たまに 2 つになる —— filer のバグではなく harness の話

- **実測**: 約 50 文字打って 3 回、`pty.log` に `--` / `cc` / `xx` と 1 チャンクで 2 文字出た
  （`ni esc-closed-the-key--list.txt`、`abcc`、`line-was-cleared.txxt` がその結果）。
- **原因の切り分け**: `WM_CHAR` だけを投げると**何も届かない**（`14-wm-char-only.png` は
  `13` と 1 バイトも違わない。`pty.log` にも行が出ない）。逆に `WM_KEYDOWN` / `WM_KEYUP`
  だけを投げると **23 文字中 3 文字だけ**届いた（`y` `-` `g`）。つまり winit は通常
  `WM_CHAR` から文字を作るが、**対になる `WM_CHAR` がまだキューに無いときは
  キーボードレイアウトから自分で作る。**別プロセスから `PostMessage` すると 3 つの
  メッセージが 1 つずつ届くので、この「先回り」が時々起きて、**そのあと届いた
  `WM_CHAR` が 2 文字目になる。**
- **本物のキーボードでは起こらない**（`WM_KEYDOWN` と `WM_CHAR` はシステムが同時に
  キューへ入れるので、先回りする隙が無い）。**filer の不具合として報告しない。**
- **次の run への申し送り**: `PostMessage` で打った行は、`Enter` の前に
  `pty.log` か画面で**実際に何が入ったかを読むこと。**打った文字列を期待値にしない。

#### 3. スクリーンセーバーは今回も入力デスクトップを持っていた

#88 が書いたとおりで、`LogonUI` は無いまま `OpenInputDesktop` の名前が `Screen-saver`。
この run は最初から `PostMessage` と `PrintWindow(PW_RENDERFULLCONTENT)` だけで測ったので、
`SendInput` の空振りには 1 度も当たらなかった。**#88 の提案 2（役割定義の検査に
スクリーンセーバーを足す）は、2 本続けて必要だったことになる。**

### Proposals

1. **TESTING.md に「リリースの zip」の節を作ってほしい。**
   - 何が起きたか: 順番表の先頭の仕事なのに、合格を書き込む行が 1 つも無かった。
     今回読んだ 4 つ（ファイルが 4 つであること・ライセンスの版がピン止めと一致すること・
     3 つの PE が目的のアーキテクチャであること・同梱 ConPTY がピン止めパッケージと
     バイト一致すること）は、**全部が機械で読めるテキストかハッシュ**で、
     見た目の判断が 1 つも要らない。
   - どう変えるか: 新しい節（たとえば「47. the release zip」）に 5 行程度。
     Windows の 2 つ（x64 / arm64）と、tar.gz 側の「実行ビットが残っていること」を
     1 行ずつ。**どのプラットフォームの zip かを行に書く**と、レーンごとに取れる。
   - なぜ: **人が最初に触るのはリリースの zip で、そこが壊れると中身は関係ない。**
     いまは誰も確かめておらず、確かめても記録が残らない。
   - 大きさ: TESTING.md に 1 節。`make-testcheck` が生成し直すだけ。

2. **`filer --keys "<C-t>lazygit\r"` のような、キー列を流し込む入り口がやはり欲しい。**
   （#88 の提案 1 の再掲。**今回は別の証拠が付く。**）
   - 何が起きたか: この run で実際に測ったのはキー 6 つ（`t` / `lazygit` / `?` / `Esc` /
     `q` / 1 行）なのに、そこへ至るまでに harness keymap、`PostMessage` の組み立て、
     winit のイベントウィンドウの回避、そして**上の「2 文字になる」現象の切り分け**を
     やっている。**測定そのものより足場のほうが長い。**
   - どう変えるか: keymap を通してキー列を流す引数か、環境変数で受ける待ち受け。
     修飾キーが要らなくなれば、harness keymap ごと消える。
   - なぜ: 無人の実機 run は毎回これを組み直していて、**しかも今回は組み方が原因で
     入力が 2 文字になった。**足場が測定を汚すところまで来ている。
   - 大きさ: 設計の判断（テスト用の口をリリースするバイナリに置いてよいか）。持ち主が決める類。

3. **リリースのページに、zip の中身が 4 つであることと SHA-256 を出してほしい。**
   - 何が起きたか: 同梱 ConPTY が正しいものかを確かめるのに、`fetch-conpty.ps1` を
     もう一度走らせてハッシュを突き合わせた。**落とした人には、この手が無い**
     （スクリプトはリポジトリにあるが、zip だけ取った人は見ない）。
   - どう変えるか: `release.yml` が作るノートに、各成果物の SHA-256 と、
     Windows の zip については「`filer.exe` のほかに ConPTY の 2 ファイルと
     ライセンスが入る」の 1 行を足す。
   - なぜ: **バイナリを配る以上、受け取った側が同一性を確かめられる形にしておくのが筋。**
     いまはノートに版の話しか無く、4 ファイルが揃っているかは展開しないと分からない。
   - 大きさ: `release.yml` に数行（`Get-FileHash` の結果を本文に足すだけ）。

4. **ペインの `lazygit` が初回に出す案内で、`<C-t>` が効かなくなる場面がある。**
   - 何が起きたか: 1 回目の起動で「Thanks for using lazygit!」のポップアップが出た。
     これは lazygit 自身のもので filer は悪くない。ただ、**この状態のペインは
     全画面 TUI がポップアップを持っている状態**で、キーは全部 lazygit に渡る。
     `Esc` で閉じられたから進めたが、閉じ方を知らない TUI に当たった人は
     **ペインごと抜けられない**と思うはずで、画面には何の手掛かりも無い。
     （`<C-t>` は `[term]` 層の `close` なので効くはず —— ただしこれは
     keymap を読んで言っているだけで、**この run では押していない。**
     `PostMessage` に修飾キーが乗らないため、押すには別の harness keymap が要る。）
   - どう変えるか: ペインの下端（ステータスバー側）に、TUI が走っている間だけ
     `<C-t> back to the list` のような 1 行を出す。あるいは 1.35 の確認ダイアログと
     同じ文言で、`<C-t>` が何をするかを最初の 1 回だけトーストで出す。
   - なぜ: **ペインに入った人が出口を知らないのが、いちばん怖い状態。**
     全画面 TUI は画面を丸ごと取るので、filer の UI は何も見えていない。
   - 大きさ: 表示 1 行（どこに出すかは見た目の判断なので持ち主が決める）。

### 順番表（`.claude/windows-role.md`「The ARM64 lane」）

無人実行は `.claude/` への書き込みを権限で拒否されるので、変更は PR 本文の `## Queue` に書いた。
**`the release zip itself` の行を消してよい**（4 つの検査は全部通り、zip の binary で
1.31 / 1.32 / 1.34 も動かした）。次は `1. the terminal pane, again` が先頭になる。
**その行に 1 つ足してほしい**: 1.35 / 1.36 はリリース v0.49.1 に無い機能なので、
**その節は手元ビルドで走らせること**。`the test suite` の行はそのまま残す
（この run も 502 / 0 を記録した）。

## TESTING.md section 1 — ターミナルペインを ARM64 で全部やり直した（e26671d / 0.54.0、ARM64 レーン）

ARM64 レーンの 5 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
順番表の先頭「**1. the terminal pane, again**」が担当。前の run（zip）が
「1.35 / 1.36 はリリース v0.49.1 に無いので手元ビルドで」と申し送っていたので、
**手元の release ビルド（0.54.0、aarch64）**で走らせた。

**チェックを 5 つ付けた**（1.9f / 1.9h / 1.14 / 1.35 / 1.36。どれも `[ ]` だった行で、
根拠は下に 1 行ずつ書いた）。**1.35 と 1.36 は、どのプラットフォームでも初めて押された。**
`make-testcheck` を走らせ直して `--check` が `in sync`（144 / 394、節の見出しは 37 / 48）。
`make-keycheck -- --check` も `in sync`（243 / 247）。

### この機械

| | |
| --- | --- |
| 機械 | `PROCESSOR_ARCHITECTURE=ARM64`、`Win32_ComputerSystem.SystemType` = `ARM64-based PC` |
| OS | `Windows 11 Home 26H1 (build 28000.2956)` |
| rustc | 1.98.1 (48a229cea 2026-09-01) / aarch64-pc-windows-msvc —— CI と同じ stable |
| filer | 手元ビルド 0.54.0、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| GPU | Qualcomm(R) Adreno(TM) X2-90 GPU（Vulkan、IntegratedGpu） |
| 昇格 | 無し（`IsInRole('Administrators')` = False）。この節に昇格の要る行は無い |
| `cargo test` | **505 passed; 0 failed**（ネイティブ ARM64、2.60 s、e26671d）。x64 ランナーに無い失敗は無い |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ。ペインの子は `OpenConsole.exe --headless` で、そこの実体 |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |
| 入力 | **スクリーンセーバーが入力デスクトップを持っていた**（`OpenInputDesktop` の名前 = `Screen-saver`、`SPI_GETSCREENSAVERRUNNING` = True、`LogonUI` は無し）。#88 から 3 本連続 |

生の証拠は `C:\dev\filer-evidence\arm-1\`（`shots\` に 75 枚、`logs\` に `FILER_PTY_LOG` の
記録 15 本、`lib1.ps1` / `desk.ps1` / `post.ps1` / `find.ps1` / `shot.ps1`、`arm1-env.txt`）。

### 測り方 —— `--keys` で足場がほぼ消えた

**この run は v0.54.0 の `filer --keys` でほぼ全部を駆動した。**前の run（#91）の提案 2 が
そのまま効いていて、**harness keymap を 1 度も書かなかった**。修飾キー付きの和音
（`<C-t>` / `<C-S-t>` / `<C-S-Enter>` / `<C-S-f>` / `<C-S-n>` / `<C-S-b>` / `<C-c>` / `<A-t>` /
`<A-Up>`）が、そのまま本物のキーとして押せる。スクリーンセーバーが入力デスクトップを
持っていても関係ない。

足りないのは**待つ手立てだけ**。`--keys` は `App::settled()` の間だけ待って次を押すので、
実測 **1 キー 33 ms**（`<S-End>` を 100 個並べて 3329 ms）。**シェルが起動するのを待つ、
lazygit が描き終わるのを待つ、といった間は「無害なキーを並べて埋めた」**
（一覧の外では `<S-End>` = `term_scroll bot`、全画面 TUI が出ている間は `<A-Up>` = `term_cd`。
前者は代替画面ではプログラムへ転送されるので、TUI が動いている間は後者を使う）。
この埋め草が 1 つ副作用を出した —— 下の「見つけたもの」2 番。

読む側は 3 つ:

- **`FILER_PTY_LOG`** —— シェルに何が行き、シェルが何を返したか。`in key` にあるかどうかで
  「キーが一覧に戻ったか」まで測れる（`<C-t>` のあとに押した `<Down>` や `c` `f` が
  ログに 1 文字も出ない、が証拠になる）。
- **`PrintWindow(PW_RENDERFULLCONTENT)`** —— スクリーンセーバーの下でも撮れる。ダイアログ本文・
  トースト・「N 行前」の注記は、**画像から読んだテキスト**として扱った。
- **プロセスとクリップボード** —— `Win32_Process` で filer の子（`powershell.exe` / `lazygit.exe`）、
  `c` `f` の結果で一覧のカーソル位置。

`PostMessage` で文字を打つのは**やめたほうがいい**（最初に試して 2 つとも外れた）:
`WM_KEYDOWN`+`WM_CHAR` は 12 文字に 1 回 `oo` と重なり（#91 の申し送りどおり）、
`WM_CHAR` だけだと**1 文字も入らず**、`WM_KEYDOWN` だけだと `echo keydownonly` が `eco ` に
なった。`--keys` は 1 度も外していない。

### ARM64 の結果（x64 で `[x]` の行）

すべて**この機械で押し直した**。ペインは既定 12 行 x 159 桁。

| 行 | ARM64 の結果 | 根拠 |
| --- | --- | --- |
| 1.1 | 合格 | `pty.log` の最初のプロンプトが `PS C:\…\filer-fixtures\repo>` —— 一覧が見せていたディレクトリ |
| 1.2 | **半分**（出力は届く。字の重なりは見た目） | `dir` の出力が `Mode / LastWriteTime / Length / Name` の桁で並ぶ（`shots\19-16200.png`） |
| 1.3 | **半分**（塗った四角が打鍵で動くところまで） | `x` を打つ前後で、四角がプロンプト直後から `x` の次へ移った（`19b-18600.png`） |
| 1.5 | 合格 | `== pane opened` が**ログ全体で 1 回**。`<C-t>` を挟んで `echo ONE` と `echo TWO` が同じシェルに届き、ONE の出力が残ったまま |
| 1.6 | 合格 | 同上。往復の間に押した `<Down><Down>cf` は `in key` に 1 文字も無く、クリップボードが `clean.txt`（3 行目）になった |
| 1.7 | 合格 | `<C-S-t>` のあと filer の子から `powershell.exe` と `OpenConsole.exe` が消え、filer は生存 |
| 1.8 | **半分**（桁は組み直る。切れ・伸びは見た目） | ウィンドウを 1376 → 820 px にして、ペインの `$Host.UI.RawUI.WindowSize.Width` が **159 → 93** |
| 1.9 | 合格 | `many\` で `dir` のあと `<S-PageUp>`。上端が item-492 → item-486 に動き、注記が `6 lines back` |
| 1.9a | 合格（両向き） | `<S-PageUp>` x4 で `24 lines back`、`<S-PageDown>` で `18 lines back`。半画面 = 6 行（ペインが 12 行） |
| 1.9b | 合格 | `<S-Home>` で `502 lines back` と最古の `Windows PowerShell` 行、`<S-End>` でプロンプトへ |
| 1.9c | 合格（注記で測った。強調は画像から読んだ） | `item-05` を検索 → `433 lines back`、上端が `item-059.txt` で `item-05` に背景が付く |
| 1.10 | 合格 | `<S-PageUp>` x2 のあと `x` を 1 文字打っただけで注記が消え、最下部の item-500 とプロンプトに戻った |
| 1.11a / 1.11b / 1.11c | **取れず** | ドラッグが要る。スクリーンセーバーが入力デスクトップを持っていて `SendInput` が届かない（下の提案 1） |
| 1.13 | **半分** | `<C-S-n>` で次の一致へ進むところまで確認（433 → 434）。**末尾で先頭に回り込むところは押していない** |
| 1.15 | 合格 | ペインの中から `<C-S-p>`、`filename` で絞って `<Enter>` → クリップボードが `item-001.txt`。パレットはペインの上に描かれた（`115a-palette.png`） |
| 1.16 | 合格（ただし OSC 7 を出すシェルが要る。下の 4 番） | pwsh に README のフックを入れて `cd ..` → `<A-Up>` で**ウィンドウタイトルが 3910 ms に `…\many` → `…\filer-fixtures`** |
| 1.17 | 合格 | 2 つ選んで `<A-t>` → `'…\a file with spaces.txt' '…\long-long…'`。`in key` に `\r` は無い |
| 1.18 | 合格 | 上と同じ run。`in key` は `cd ..\r` の 1 本だけ —— 一覧が追従したあと filer は `cd` を打っていない |
| 1.19 | 合格 | `sleep 30` のあと `<C-c>` が `in key \x03` として届き、プロンプトが即戻って `echo AFTER` が走った。filer は 1 タブのまま生存 |
| 1.20 | 合格（4 シェル） | `quote'in-name.txt` が powershell / pwsh で `'…quote''in-name.txt'`、cmd で `"…quote'in-name.txt"`、bash で `'…quote'\''in-name.txt'`。どれも `\r` 無し |
| 1.21 | 合格 | `it's here` へ入ると `cd 'C:\…\it''s here'\r` が飛び、プロンプトが `PS C:\…\it's here>` に |
| 1.22 | 合格 | Git Bash（ARM64 版、プロンプトが `CLANGARM64`）で `cd 'C:\…\qtest\plain'` が通り、`/tmp/s1/qtest/plain` へ。`No such file or directory` は 1 度も出ていない |
| 1.23 | 合格 | 同じシェルで `<A-t>` → `'C:\…\plain\ordinary.txt'` が丸ごと 1 語で入った |
| 1.24 | 合格 | lazygit が代替画面・罫線・色・自前の分割ペインで描けている（`135a-dialog.png`、`127-8500.png`） |
| 1.25 | 合格 | `j j k k` と `q` が `in key` にあり、`\e[?1049l` で代替画面を出て、lazygit のプロセスが消え、`echo BACK` がプロンプトで走った |
| 1.26 | 合格 | `<C-S-Enter>` でペインの行数が **12 → 35**、画面はヘッダも一覧も無くステータスバーだけ（`126a-maximised.png`）。もう一度で 12 |
| 1.27 | 合格 | **lazygit を動かした状態**で `<C-S-Enter>` → lazygit が全高で描き直され、もう一度で 3 分の 1 に戻って一覧が出た（`127-8500.png` / `127-11500.png`） |
| 1.28 | 合格 | 一覧側から `<C-S-Enter>` → `echo MAXTYPE` が `in key` とペインの出力に出た |
| 1.29 | 合格 | そのあと `<C-t>` → `<Down><Down>cf` が一覧に効いて（クリップボード `clean.txt`）、戻ってから測った行数が **12** |
| 1.30 | 合格 | 最大化から `<C-S-t>` → ペインが消え、`many\` の item-001〜item-033 が**ステータスバーまで**並ぶ（`130-after-close.png`）。隙間は無い |
| 1.31 | 合格 | lazygit で `?` → キー一覧、`<Esc>` で閉じてフッタが `ステージ: <space> ¦ …` に戻った（`131h-after-esc.png`）。**ただし条件付き —— 下の 2 番** |
| 1.32 | 合格 | pwsh のプロンプトで `abc`、`<Esc>` で行が空に（`crop132-4000.png` / `crop132-8000.png`） |
| 1.33 | 合格 | `pty.log` に `in reply \e[?6c` —— filer 自身の答え。Windows 標準の ConPTY なら `\e[?61;6;7;…c` になる |
| 1.34 | 合格 | lazygit が通常画面で開く。コピーメニューは出ていない（`131f-lazygit.png`、`135a-dialog.png`） |

**取らなかった行**（見た目、またはマウス）: 1.3a（中抜きカーソルと枠の色）、1.4（16 色が
一覧の配色と一致して見えるか）、1.9d（ホイールの滑らかさ）、1.9e（カーソルが取り残されないか）、
1.11（ドラッグ中に描かれるか）、1.12（ダブルクリックの選択）。**1.9d / 1.11 / 1.12 と
1.11a-c はマウスが要るので、スクリーンセーバーがある限りこの機械では取れない。**

### 新しく付けたチェック（5 件）

- **1.9f** —— `dir`（500 行）のあと `item-49` を検索。画面に出ている `item-499` が見つかり、
  **「N 行前」の注記が出ず、表示も動かなかった**。履歴側の `item-049` を拾っていたら、
  同じ条件の `item-05` がそうだったように `433 lines back` と出たはず。`shots\19f-onscreen.png`
- **1.9h** —— `zzznotthere` を検索 → 赤枠のトーストに **`No match for zzznotthere`**。無反応ではない。
  `shots\19h-6200.png`
- **1.14** —— ペインの中で `<F1>` → キー一覧が**ペインの上に**開き（背後にペインの
  `Windows PowerShell` / `BEFORE` / プロンプトが見えている）、`<Esc>` のあと `echo AFTER` が
  シェルに届いた。`in key` は `echo BEFORE\r` と `echo AFTER\r` の**間に何も無い** ——
  `<F1>`・埋め草・`<Esc>` は 1 バイトもシェルへ行っていない。`shots\114a-help-open.png` /
  `114c-after-esc.png`
- **1.35** —— ペインで `lazygit` を動かしたまま `<C-S-t>`。**`End the shell?` /
  `` `repo - Lazygit` is still running in the terminal, and ends with it. `` / `[y] End it`
  `[n] Keep it`**（`135a-dialog.png`）。`n` → ダイアログが消え、ペインも lazygit も残る
  （`powershell.exe` の子に `lazygit.exe`、`135b-after-n.png`）。`y` → ペインが消え、
  トーストが **`Ended the shell`**、filer の子は空、`Get-Process lazygit` も空（`135c-after-y.png`）
- **1.36** —— 何も動いていないプロンプトで `<C-S-t>` → **確認は出ず**、ペインが即消えて
  トーストが `Ended the shell`、一覧が全高。filer は生存、子は空（`136-after-cst.png`）

### 見つけたもの

#### 1. `<C-S-b>`（スクロールバック検索を下へ戻る）は、向きを変えた 1 回目が空振りする

`many\` で `dir` → `<C-S-f>` `item-05` → 注記 `433 lines back`（上端 `item-059.txt`）。そこから:

| 押したキー | 注記 |
| --- | --- |
| `<C-S-n>` | `434 lines back` |
| `<C-S-b>` | **`434 lines back`（動かない）** |
| `<C-S-b>` | `433 lines back` |

**`<C-S-n>` 1 回を取り消すのに `<C-S-b>` が 2 回要る。**1.9g の文言
「`<C-S-b>` で下へ戻る」は 2 回目から成り立つ。`app.rs` の `term_search` は
`--prev` を `term_find(&needle, !prev)` に渡して向きだけ反転しているので、
**反転した直後の 1 回が、いま居る一致そのものを見つけ直している**のだと思う
（`terminal.rs` の `search` が現在位置を含むかどうかの話）。
別々の run で 2 度再現した（`logs\r19c-pty.log` / `logs\r19g-pty.log`、
`shots\crop-6700.png` 〜 `crop-18700.png`）。**1.9g にはチェックを付けていない。**

#### 2. 全画面プログラムへ転送されたスクロールキーの塊が、そのプログラムを数十秒止める

`<S-End>` は代替画面のときだけプログラムへ転送される（`app.rs:4304` の
`if scrolls && self.term_alt_screen()`。意図どおり）。lazygit を動かした状態で
`<S-End>` を **1 秒あたり 30 回**（`--keys` の実測ペース）流すと、`\e[1;2F` が
その数だけ届く。165 回ほど流したあとで `?` → `<Esc>` と押すと:

- **15 秒後にもキー一覧が開いたまま**（`shots\131d-after-esc.png` / `131e-after-esc2.png`）
- **60 秒後には閉じている**（`131n-t60.png`）。`<Esc>` は届いていて、遅れているだけ

同じ手順で、埋め草を `<A-Up>`（filer が食べるのでプログラムへ行かない）に替えると
**その場で閉じる**（`131l-altup-pad2.png`）。`PostMessage` で `?` と `<Esc>` だけを
送った run も**その場で閉じる**（`131h-after-esc.png`）。つまり**転送されたスクロールキーの
量だけが原因**。

- **人にも起こりうる。**キーリピートは毎秒 30 回前後なので、TUI の上で `<S-End>` を
  2〜3 秒押しっぱなしにすると同じ量になる。押した本人には「TUI が固まった」に見える。
- どちらの側の問題かは**切り分けていない**（filer が速く書きすぎるのか、tcell が
  1 キーごとに全画面を描き直すのか）。`FILER_PTY_LOG` にはこちらが書いた
  `\e[1;2F` しか残らないので、ここから先は section 40 の仕事。
- **1.31 は合格にした。**転送を挟まない手順では毎回その場で閉じる。

#### 3. `filer env` / `filer --version` の出力は、ファイルにリダイレクトすると**空になる**

`main.rs` の `say()` は `AttachConsole(ATTACH_PARENT_PROCESS)` してから `CONOUT$` を開いて
書く。**`CONOUT$` はコンソールの画面バッファそのものなので、`>` も `|` も素通りする。**

```
> cmd /c start /wait "" cmd /c "filer.exe --version > ver.txt 2>&1"
size: 0
content: []
```

`filer env` の説明は「バグレポートに貼るためのもの」なのに、
**貼るための自然な操作（`filer env > env.txt`、`filer env | clip`）が黙って何も残さない。**
画面には出るので、気づくのは貼り付けたあと。
（この run で `filer env` を取れたのは、**コンソールを持たない親**から起動したときだけ
—— `wscript.exe` 経由なら `AttachConsole` が失敗して `println!` に落ちる。
`arm1-env.txt` はその方法で取った。）
ARM64 の話ではない。**直していない**（`GetStdHandle(STD_OUTPUT_HANDLE)` が
リダイレクトされているかを先に見る、が筋だと思う）。

#### 4. pwsh 7.6.6 は、この機械の既定のプロンプトでは OSC 7 を出さない

1.16 / 1.18 は「OSC 7 を報告するシェル（PowerShell 7、または …）」と書いてあるが、
**この機械の pwsh 7.6.6（Starship）では 1 度も `\e]7;` が出ず、`<A-Up>` は
`The shell has not said where it is (no OSC 7). …` のエラートーストになった**
（`shots\116-final.png` がそのときのペイン）。README のフック
（`LocationChangedAction`）を `-NoProfile -NoExit -File` で入れて初めて通った
（`logs\r116b-pty.log` に `\e]7;file:///C:/…/filer-fixtures` が出ている）。

- **filer の不具合ではない。**README はこの手順をちゃんと書いている。
- ただし **TESTING.md 1.18 の「PowerShell 7」は、そのままでは条件を満たさない。**
  行の文言を「README のフックを入れた PowerShell 7」にするか、pwsh を外すのが正しい。
  **renumbering を避けるためこちらでは直していない。**

#### 5. スクリーンセーバーは 3 本続けて入力デスクトップを持っていた

`OpenInputDesktop` の名前が `Screen-saver`、`SPI_GETSCREENSAVERRUNNING` が True、
`LogonUI` は無し。#88 → #91 → 今回で 3 本連続。**この run は最初から `--keys` と
`PostMessage` だけで測ったので 1 度も空振りしなかったが、マウスの行（1.9d / 1.11 /
1.11a-c / 1.12、section 14 や 30 も同じ）は、この機械では取りようがない。**下の提案 1。

### Proposals

1. **`auto-wintest.ps1` に、実行中だけスクリーンセーバーを止めさせてほしい。**
   - 何が起きたか: 3 本続けて入力デスクトップが `Screen-saver` だった。おかげで
     `SendInput` が 1 つも使えず、**section 1 のマウス 6 行（1.9d / 1.11 / 1.11a / 1.11b /
     1.11c / 1.12）は押せないまま残った。**順番表にはマウスが主役の節（14「the parent
     column, with the mouse」、30「right-click paste」）もあり、**このままだとその 2 節は
     ARM64 レーンに回した瞬間に空振りする。**
   - どう変えるか: `auto-wintest.ps1` の頭で
     `SystemParametersInfo(SPI_SETSCREENSAVEACTIVE, 0)`、`finally` で元に戻す。
     昇格は要らない（ユーザー単位の設定）。**戻し忘れが怖いので、元の値を状態ファイルに
     書いてから消す**形にすれば、run が落ちても次回に戻せる。
   - なぜ: **いま失われているのは「マウスの行が永久に取れない」こと**で、これは
     見た目かどうかの話より重い。持ち主の機械の設定を触るので、判断は持ち主のもの。
   - 大きさ: スクリプトに 10 行ほど＋ P/Invoke 1 つ。

2. **`--keys` に「待つ」を足してほしい —— `<Wait:2000>` のような擬似キー 1 つでいい。**
   - 何が起きたか: **`--keys` のおかげでこの run は harness keymap を 1 度も書かずに済んだ**
     （前の run は書いた）。残った不便は 1 つだけで、**シェルの起動や lazygit の描画を待つ手立てが
     無いこと。**そこで `<S-End>` や `<A-Up>` を 50〜300 個並べて時間を潰したのだが、
     **その埋め草が測定を汚した** —— 上の「見つけたもの」2 番は、まさに埋め草が原因で
     1.31 が落ちて見えた事故で、切り分けに 4 run 使った。
   - どう変えるか: `keyscript::parse` に `<Wait:1500>`（ミリ秒）を足し、
     `raw_input_hook` の gate に「その時間が経つまで次を押さない」を 1 本足す。
     キーではないので `events()` は `None` ではなく専用の分岐になる。
   - なぜ: **無害な埋め草は存在しない。**`<S-End>` は代替画面でプログラムへ行き、
     `<A-Up>` はエラートーストを 500 個積む。待ちを表現できないせいで、
     **計測のたびに「何を押しても副作用が無い場面か」を考える羽目になる。**
   - 大きさ: 15 行ほど。`<Wait:…>` の構文だけ持ち主が決めれば実装は素直。

3. **`filer env` をリダイレクトできるようにしてほしい**（上の「見つけたもの」3 番）。
   - 何が起きたか: `filer env > env.txt` が 0 バイトのファイルを作る。この run では
     `wscript.exe` 経由という遠回りでしか取れなかった。
   - どう変えるか: `say()` で、まず `GetStdHandle(STD_OUTPUT_HANDLE)` が
     コンソール以外（ファイル・パイプ）を指しているかを `GetFileType` で見る。
     そうなら `println!`、そうでなければ今の `CONOUT$` の道。
   - なぜ: **`filer env` は貼るために作った出力で、貼る人はまずリダイレクトする。**
     黙って空になるのは、出力が無いより悪い（気づくのが後になる）。
   - 大きさ: `say()` に 5 行。

4. **ペインの行数と桁数を、どこかに数字で出してほしい。**
   - 何が起きたか: 1.8 / 1.26 / 1.27 / 1.29 を測るのに、毎回ペインの中で
     `$Host.UI.RawUI.WindowSize.Height` を打った。**シェルに依存する**ので、
     cmd や bash では別のコマンドが要る（bash なら `tput lines`、cmd には素直な手が無い）。
   - どう変えるか: `filer env` に `Terminal pane : 12 x 159` の 1 行。あるいは
     ペインの右上の注記（「N 行前」が出るところ）に、リサイズ直後だけ `12 x 159` を
     1 秒出す。
   - なぜ: **ペインの大きさは、この節の 4 行が主張していることそのもの**なのに、
     filer 自身はどこにも言わない。バグレポートでも「3 分の 1 にならない」と書く人は
     数字を持っていない。
   - 大きさ: `filer env` に 1 行なら数行。注記のほうは見た目の判断が要る。

5. **`<C-S-t>` の確認ダイアログが、プログラムの名前をターミナルのタイトルから取っているのは良い。**
   （変更の提案ではなく、そのまま残してほしいという話。）lazygit は
   `cmd /c title repo - Lazygit` を投げるので、ダイアログが
   `` `repo - Lazygit` is still running `` と**リポジトリ名まで**出せていた。
   タイトルを出さないプログラムでは `A program` に落ちる（`app.rs:4124` 付近）。
   **落ち方が正しいので、ここに凝った推測（子プロセスの実行ファイル名を読むなど）を
   足さないほうがいい** —— タイトルは「プログラムが自分で名乗った名前」で、
   実行ファイル名より人に通じる。

### 順番表（`.claude/windows-role.md`「The ARM64 lane」）

無人実行は `.claude/` への書き込みを権限で拒否されるので、変更は PR 本文の `## Queue` に書いた。

- **`the release zip itself` の行は、まだ表に残っている。**#91 が「消してよい」と
  書いたのにマージ側で反映されていない。**今回こそ消してほしい**（残っていると、
  次の run がまた zip を見に行く）。
- **`1. the terminal pane, again` の行も消してよい。**x64 で `[x]` の行は
  全部この機械で押し直し、結果を上の表に書いた。`[ ]` のまま残るのは
  **見た目（1.3a / 1.4 / 1.9e、および 1.2 と 1.8 の後半）とマウス（1.9d / 1.11 / 1.12、
  再確認としての 1.11a-c）と 1.9g（上の 1 番の不具合）**で、
  **どれも ARM64 の話ではない。**この行を残しても、次の run が同じ行を見送るだけになる。
- 次は `21 / 32 / 37. archives and openers, again` が先頭になる。
- `the test suite` の行はそのまま残す（この run も 505 / 0 を記録した）。

## TESTING.md 21 / 32 / 37 — 書庫とオープナーを ARM64 でやり直した（3c1ef3f / 0.54.3、ARM64 レーン）

ARM64 レーンの 6 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
順番表の先頭「**21 / 32 / 37. archives and openers, again**」が担当。レーンの規則どおり、
x64 で `[x]` の行を**この機械で押し直し、結果をここに書く**。

**チェックは 1 つも付けていない。**x64 で `[x]` の 21 行（21 節 8 件・32 節 7 件・37 節 6 件）は
**全部この機械でも合格**したので、規則どおり結果は下の表に置いた。`[ ]` の行では **21.4 を取りに行って、
取れないことを確かめた** —— 詳細は下の「21.4」の節。`make-testcheck -- --check` は `in sync`
（144 / 398）、`make-keycheck -- --check` も `in sync`（243 / 247）。

### この機械

| | |
| --- | --- |
| 機械 | `PROCESSOR_ARCHITECTURE=ARM64`、`Win32_ComputerSystem.SystemType` = `ARM64-based PC` |
| OS | `Windows 11 Home 26H1 (build 28000.2956)` |
| rustc | 1.98.1 (48a229cea 2026-09-01) / aarch64-pc-windows-msvc —— CI と同じ stable |
| filer | 手元ビルド 0.54.3、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| 昇格 | 無し（`IsInRole('Administrators')` = False）。この 3 節に昇格の要る行は無い |
| `cargo test` | **506 passed; 0 failed**（ネイティブ ARM64、2.62 s、3c1ef3f）。x64 ランナーに無い失敗は無い |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |
| 入っている | 7-Zip 26.02、Office 16.0.20430（Excel / Word / PowerPoint）、Edge、Chrome、VS Code、Neovim |
| 入っていない | **秀丸・サクラ・IrfanView**（`App Paths` にも `where` にも無い）。32.2 / 32.5 / 32.8a / 32.8b / 32.9 と 37.7 / 37.8 の一部は、そのせいで取れない |

生の証拠は `C:\dev\filer-evidence\arm-21-32-37\`（`*.png` 31 枚、`arm-env.txt`、
使った設定 2 本 `cfg-readme.toml` / `cfg-per-app.toml`、`postkey.ps1` / `shot.ps1` / `watchwin.ps1`）。

### 測り方

- **キーは `--keys` で入れた。**書庫は `filer <dir> --keys "E<BS><BS><BS>7z<Enter>"` のように
  1 本のコマンドで固められるので、足場がまったく要らない。
- **`--keys` のあとに押したいキーは `PostMessage` で入れた。**ジョブの完了を待ってから押すものが
  あるため（タスクパネル、ピッカーの選択）。#93 は「`PostMessage` はやめたほうがいい」と
  書いているが、**外れていた原因は `lParam` だった** —— 下の「Proposals」1 番。
- **読む側**は 4 つ: `Get-Clipboard`（`c` `c` でカーソル位置）、`PrintWindow(PW_RENDERFULLCONTENT)`
  のスクリーンショット（トースト・タスクパネル・ピッカーを**テキストとして**読む）、
  `Get-CimInstance Win32_Process` の `CommandLine`（どのプログラムが何を渡されて起動したか）、
  ディスク（`7z l` / `tar -tvf` / `Get-FileHash` / マジックバイト）。
- **コンソールが出ていないこと**は、`EnumWindows` を 5 秒回して `ConsoleWindowClass` /
  `CASCADIA_HOSTING_WINDOW_CLASS` / `PseudoConsoleWindow` の可視ウィンドウを集め、
  **開く前と同じ集合であること**で測った（`watchwin.ps1`）。37.1 の「コマンドプロンプトが出ない」はこれ。

### ARM64 の結果（x64 で `[x]` の 21 行）

| 行 | ARM64 | 根拠 |
| --- | --- | --- |
| 21.2 | 合格 | `--keys "jcce"`。`c` `c` が `…\arm21\sample.zip` を返し（カーソルの位置）、隣に `sample\` ができて file1-5.txt（各 12 B）と `nested\deep.txt`（6 B）の 6 件 |
| 21.3 | 合格 | もう一度 `e` → `sample_1\` ができ、`sample\` は 6 ファイルのまま。上書きされていない |
| 21.5 | 合格 | `--keys "E<BS><BS><BS>tar.gz<Enter>"` → `to-pack.tar.gz` 253 B。先頭 2 B が `1F 8B`（gzip）、`7z l` が `Type = gzip`、中の tar を `tar -tvf` すると 6 ファイル 2 フォルダ。**zip ではない** |
| 21.7 | 合格 | 同じく `.7z` → 403 B。先頭が `37 7A BC AF 27 1C`、`7z l` が `Type = 7z` / `Method = LZMA2:23`。**本物の 7z** |
| 21.8 | 合格 | その `.7z` で `e` → `to-pack_1\to-pack\` に 6 ファイル。元の 6 ファイルと SHA-256 が全部一致（`48CE61F9,5B525BE5,071CAD55,162D4166,1585BFC7,682598B1`） |
| 21.9 | 合格 | `7z t` が `Everything is Ok / Folders: 2 / Files: 6`。7-Zip File Manager を起動するとウィンドウタイトルが `…\to-pack.7z\` |
| 21.10 | 合格 | 200 ファイル・5 サブフォルダ・50 MB を `.7z` に。タスクパネルが `90/200 files · 22 M / 50 M · 5.1 M/s · 5s left` → `200/200 files · 50 M / 50 M [done]`。**件数はファイルの 200 で、フォルダを足した 205 ではない**（`2110b-3.png` / `2110b-8.png`） |
| 21.11 | 合格 | 同じ `to-pack\` から 7z 403 B、zip 970 B。7z のほうが小さい |
| 32.1 | 合格 | README の `[opener]` / `[open]` をそのまま貼って `<S-Enter>`。`秀丸エディタ` / `サクラエディタ` / `VS Code` / `Neovim` / `Open with the default app` が**説明文**で並び、コマンド行は右端（`321-picker.png`） |
| 32.3 | 合格 | `doc.pdf` で `<S-Enter>` → `Edge` / `Chrome` / `Open with the default app` / 秀丸 / サクラ / VS Code / Neovim の順（`323-pdf-picker.png`） |
| 32.4 | 合格 | `book.xlsx` で `<S-Enter>` → 先頭が `Excel`。`<Enter>` で `EXCEL.EXE "…\open\book.xlsx"` が起動（`Win32_Process`）。**残った `cmd.exe` は無い** |
| 32.6 | 合格 | `doc.pdf` と `second.pdf` を `<Space>` で選び（ヘッダが `2 selected · 12 items`）`<Enter>` → msedge **1 プロセス**に 2 つの引数。`"…doc.pdf" "…second.pdf"` |
| 32.7 | 合格 | `*.{xlsx,xlsm,xls,csv}` のルールで `book.xlsx` / `data.csv` / `old.xls` の 3 つとも `office` のリストが先頭に来る（`324-xlsx-picker.png` / `327-csv.png` / `327-xls.png`、ヘッダのパスが各ファイル） |
| 32.8 | 合格 | `nosuchprogram-xyz %*` のオープナーで `<Enter>` → **3 秒で**赤いトースト `Open failed: exit code 1 — nosuchprogram-xyz "…\a.txt"`。直後に `c` `c` が応答するので固まっていない（`328-missing.png`） |
| 32.8c | 合格 | `"C:\Program Files\Hidemaru\Hidemruu.exe"`（わざと打ち間違えたフルパス）→ `Open failed: exit code 1 — "C:\Program Files\Hidemaru\Hidemruu.exe" "…\note.md"`。**日本語 Windows なので終了コード**で、行が言うとおり（`328c-typo.png`） |
| 37.1 | 合格 | `browser` 先頭 `start "" msedge %*` の `.pdf` で `<Enter>` → Edge のウィンドウタイトルが `doc.pdf および他 17 ページ - 個人 - Microsoft Edge`。**可視コンソールの集合は開く前と同一**（`watchwin.ps1`、5 秒） |
| 37.2 | 合格（設定は別の話） | `start "" winword %*` / `start "" powerpnt %*` を当てると `WINWORD.EXE "…\memo.docx"` / `POWERPNT.EXE "…\deck.pptx"` が起動。コンソールは出ない。**ただし README の例のままだと Excel が起動する** —— 下の「見つけたもの」1 番 |
| 37.3 | 合格 | `pic.png`（`*` のフォールバック → `open`）で `<Enter>` → `Photos` が起動し、ウィンドウタイトルが `pic.png`。コンソールは出ない |
| 37.4 | 合格 | `has space.pdf` で `<Enter>` → `msedge.exe "…\open\has space.pdf"` の**引数 1 つ**。Edge のタイトルが `has space.pdf - 個人 - Microsoft Edge` |
| 37.5 | 合格 | `start "" msedge "%*"`（手で引用符を付けた）で 37.1 と同じ結果。空白入りの名前でも `"…\has space.pdf"` が 1 引数のまま |
| 37.6 | 合格 | 32.6 と同じ起動。1 プロセスに `"…doc.pdf" "…second.pdf"` の 2 引数で、引用符でひと塊になっていない |

### 21.4 —— 取りに行って、**取れないことが確かめられた**（チェックは付けていない）

x64 の run（この報告の「21.4 の『the result opens』が何を指すのか決まらない」の節）が
「カーソルも動かない」と書いて保留にした行。**ARM64 でも同じ**で、今回は経緯自体が
`windows-role.md` の「一番の危険」そのものだったので書き残す。

1. `filer <dir> --keys "E<Enter>"` で `to-pack.zip`（970 B、先頭 `50 4B 03 04`、
   `7z l` で 6 files / 2 folders）ができる。ここまでは毎回同じ。
2. そのあと `c` `c` を送って読んだら **`…\to-pack.zip`** が返り、プレビュー枠に書庫の
   中身が並んでいた（`214-packed.png`）。「Packed, and the result opens」に見えた。
3. **これは自分が動かしていた。**同じセッションで `PostMessage` の効き方を試していて、
   `lParam = 0` の `WM_KEYDOWN`（VK 0x4A = `j`）を 1 発送っていた。当時は「効いていない」と
   判断したが、**`j` だけは通っていた。**
4. 何も余計なキーを送らずにやり直すと、カーソルは **`to-pack` のまま**（`214-recheck.png`、
   `c` `c` が `…\arm21d\to-pack` を返す）。コードもそうなっていて、
   `OpEvent::Finished`（`app.rs:3264`）は状態・エラー・undo・`rescan` だけで、
   **結果を開く / 選ぶ処理はどこにも無い。**

つまり x64 の報告は正しい。**行の文言か機能か、どちらを直すかは持ち主の判断**なので、
文言はいじらず、チェックも付けない。

### 見つけたもの

#### 1. README の `[opener]` の例では、`.docx` / `.pptx` を `<Enter>` すると **Excel** が起動する

- **どこ**: README.md の `[opener]` / `[open]` の例（140-174 行）。`office` が
  **1 本のリスト**で、その先頭が `start "" excel %*`。`[open].rules` は
  `*.{docx,docm,doc}` にも `*.{pptx,pptm,ppt}` にも `use = ["office", "open"]` を当てている。
  `<Enter>` は「最初に当たったオープナー」を走らせるので、**Word 文書でも Excel が呼ばれる。**
- **実測**: `memo.docx` で `<Enter>` → トーストが
  `$ start "" excel "C:\…\open\memo.docx"`、Excel がタイトル `Excel`（ファイル無し）で起動
  （`372-docx.png`）。`deck.pptx` でも同じ（`372-pptx.png`）。
- **filer の不具合ではない。**`office` を `excel` / `word` / `powerpoint` の 3 本に割って
  ルールを振り直した設定（`cfg-per-app.toml`）では、`WINWORD.EXE` と `POWERPNT.EXE` が
  ちゃんと起動する。つまり `start "" winword %*` の仕組み（37.2 が試している当のもの）は通っている。
- **なぜ書くか**: TESTING.md の 32 節の前書きが「**README の例が試験対象。ここで失敗したら
  手順書が間違っていて、それは機能が無いより悪い**」と言っている。まさにその状態。
  README のコメントも「`open` のリストが既に関連付けで Office に届くので、
  プログラムを名指しするのは関連付けを上書きしたいときだけ」と書いているが、
  **名指ししたリストを 3 つの拡張子で共有している**ところが噛み合っていない。
- **直し方（直していない）**: 例を `excel` / `word` / `powerpoint` に割るのが一番素直。
  `office` 1 本のままにするなら、`<Enter>` の例からは外して `<S-Enter>` 専用として書く。

#### 2. zip の日付が 1980-01-01 になる件は、0.54.3 でもそのまま（既報）

この報告の「filer が書く zip は、全エントリの日付が 1980-01-01 になる」と同じ。
今回の `to-pack.zip` も `1980-01-01 00:00:00`、同じ入力から作った `to-pack.7z` は
`2026-09-30 14:37:17`、`to-pack.tar.gz` の中の tar も `9 30 14:37`。**7z と tar は保つのに zip だけ落とす。**
ARM64 の話ではない。新しい発見でもないので、再掲だけしておく。

#### 3. `filer env` をリダイレクトすると空になる件も、0.54.3 でそのまま（既報）

#93 の 3 番と同じ。`filer.exe env > file` も `| clip` も 0 バイト。この run でも
**コンソールを持たない親から `WScript.Shell.Exec` で呼んで**取った。
`cscript` では駄目で（コンソールを持つので `AttachConsole` が成功してしまう）、
`wscript` から呼んだときだけ `println!` に落ちてパイプに出る。

#### 4. この機械の `.pdf` の関連付けは `start ""` から起動しない —— **filer の外でも同じ**

37.8 の「既定アプリ」の項目を選ぶと、トーストも出ないまま何も起きない（`378-default.png`）。
**filer のせいではない**: 同じことを filer の外でやっても起動しない。

```text
HKCU:…\FileExts\.pdf\UserChoice\ProgId = MSEdgePDF
cmd /c start "" "…\doc.pdf"      -> msedge は 1 つも増えない
Invoke-Item "…\doc.pdf"          -> 同じ
msedge.exe about:blank           -> 普通に起動する（22 プロセス、タイトルあり）
```

`.png` では同じ `start "" %*` が Photos を起動している（37.3）ので、**壊れているのは
この機械の `.pdf` の関連付け**。持ち主に直してもらう以外に無いので、37.8 は
「7 項目のうち 5 つまで」で止めた。

### 取れなかった行と、その理由

| 行 | 理由 |
| --- | --- |
| 32.2 / 32.8a / 32.8b / 32.9 | **秀丸とサクラがこの機械に入っていない。**`App Paths` にも `where` にも無い。32.8a が言う「引用符付きフルパス」の仕組み自体は、VS Code のフルパスで通ることを確かめた（37.7 の欄） |
| 32.5 | 「上のそれぞれを通して」の「それぞれ」に 32.2（秀丸）が入っているので、全部は通せない。通せた分: `has space.pdf` を `<Enter>`（37.4）と `has space.txt` をフルパスのオープナー（37.7）で、どちらも引数 1 つ |
| 37.7 | 名指しの 3 本（IrfanView / サクラ / 秀丸）がどれも入っていない。**代わりに VS Code をフルパスで**指定して確かめた: `"C:\Program Files\Microsoft VS Code\Code.exe" %*` で `has space.txt` を開くと、トーストのコマンド行に `start` が 1 度も出ず、`Code.exe "…\has space.txt"` が起動してタイトルが `has space.txt - Visual Studio Code`（`377-fullpath.png`）。**行が名指しした 3 本では試していない**ので、チェックは付けない |
| 37.8 | 並び順は合っている（`Edge` / `Chrome` / `Open with the default app` / 秀丸 / サクラ / `VS Code` / `Neovim`、`378-O-picker.png`）。起動は **Edge・Chrome・VS Code・Neovim の 4 つが表示どおり**（Chrome はタイトル `doc.pdf - Google Chrome`、VS Code は `doc.pdf - Visual Studio Code`、Neovim は `block = true` のとおり自前のコンソールが出て `nvim "…\doc.pdf"`）。**残る 3 つは秀丸・サクラ（未導入）と既定アプリ（上の 4 番）。**「それぞれ表示どおりのものが起動する」を全部は言えない |
| 21.4 | 上の節のとおり |

### Proposals

#### 1. `PostMessage` は「効かない」のではなく、`lParam` にスキャンコードが要る

- **何に当たったか**: #93 が「`PostMessage` で文字を打つのはやめたほうがいい。`WM_CHAR` だけだと
  1 文字も入らない」と書いていたので、この run も最初 `WM_CHAR` で `c` `c` を送って
  **1 度も届かなかった**。`WM_KEYDOWN` に替えても届かない。原因は `lParam = 0` で、
  **winit は `lParam` の 16-23 ビットからスキャンコードを読む**ため、物理キーに落ちず捨てられる。
- **どう変わるか**: `MapVirtualKey(vk, 0)` でスキャンコードを取り、
  `lParam = 1 | (sc << 16)`（`WM_KEYUP` はさらに `| 0xC0000000`）にすると、**そのまま通る。**
  この run はタスクパネル・ピッカーの `<Down>` / `<Enter>`・`c` `c` を全部これで入れた。
  文字は `VkKeyScan(ch)` で仮想キーに直してから同じ経路に載せる（`postkey.ps1`）。
- **なぜ**: `--keys` は起動時にしか押せないので、**ジョブの完了やピッカーの表示を見てから
  押す**ことができない。この run の 21.10（タスクパネルを開いたまま進捗を読む）と
  37.8（ピッカーの N 番目を選ぶ）は、`--keys` だけでは撮れなかった。
  `windows-role.md` の「Prefer `PostMessage` to `SendInput`」の節に **1 行**足せば、
  次の run が同じ 1 時間を使わずに済む。
- **大きさ**: ドキュメント 1 行（と、置くなら `scripts\postkey.ps1` として 40 行）。

#### 2. `--keys` に「待つ」トークンが欲しい

- **何に当たったか**: 21.10 で `E<BS><BS><BS>7z<Enter>w` と書いたら、
  `w`（タスクパネル）が `<Enter>` の直後に来る。`--keys` は `App::settled()` しか待たず、
  **ジョブは `settled()` に入っていない**ので、間に合うかどうかは運になる。
  一度は `w` が入力欄に入って名前が `big.7zw` になり、何も固められなかった。
- **何を変えるか**: `--keys "…<Enter><Wait:2000>w"` のように、**ミリ秒を待つトークン**を
  1 つ足す。キーではないので `Key::parse` の外で拾う。
- **なぜ**: 「ジョブが終わってから押す」は書庫・コピー・削除のどの節でも要る。
  いまは外から `PostMessage` を足すしかなく、それは提案 1 を知っている人にしかできない。
- **大きさ**: `keyscript.rs` に列挙型を 1 つ（`Key` か `Wait`）と、`main.rs` の送り出しに分岐。

#### 3. オープナーが失敗したときのトーストが `exit code 1` しか言わない

- **何に当たったか**: `nosuchprogram-xyz` を指定したときも、フルパスを打ち間違えたときも、
  出るのは `Open failed: exit code 1 — <コマンド行>`。32.8c は「日本語 Windows では
  終了コードが出るのが仕様」と書いてあるが、**`exit code 1` は原因を 1 文字も言っていない。**
  この run で実際に困った: 「Excel が起動して何も開かない」（上の 1 番）と
  「プログラムが見つからない」が、同じ見た目の失敗として並ぶ。
- **何を変えるか**: `cmd /C` に投げる前に、**行の先頭のプログラムを解決してみる**
  （引用符付きならそのパスの存在、裸の名前なら `PATH` と `App Paths`）。
  見つからなければ `Open failed: nosuchprogram-xyz was not found on PATH` のように、
  終了コードではなく**何が無かったか**を言う。見つかったのに落ちたときだけ終了コードを出す。
- **なぜ**: オープナーの設定は手で書くもので、間違いのほとんどは綴りとパス。
  いまのトーストは「失敗した」までしか言わないので、設定を直す手がかりが無い。
- **大きさ**: `exec.rs` に解決の関数 1 つと、`app.rs` の失敗メッセージの分岐。

#### 4. `<Enter>` が走らせた 1 本を、トーストではなくどこかに残してほしい

- **何に当たったか**: `<Enter>` で何が起動したかは `$ start "" excel "…"` のトーストで
  分かるが、**数秒で消える。**上の 1 番（docx で Excel が起動する）に気づいたのは、
  たまたまトーストが出ている間にスクリーンショットを撮っていたから。
  撮っていなければ「Word が開かなかった」までしか分からず、
  **原因が設定にあることは分からなかった。**
- **何を変えるか**: 起動したコマンド行を `w`（タスクパネル）に 1 行残す。
  ジョブではないので進捗は要らない —— 「いつ、何を、どのファイルに対して走らせたか」だけ。
- **なぜ**: オープナーはこのアプリで一番設定を間違えやすいところで、
  いまは間違いの証拠が数秒で消える。
- **大きさ**: 設計の判断が要る（タスク一覧に「ジョブではないもの」を混ぜてよいか）。
  混ぜたくないなら、別の履歴パネルか `filer env` の隣。

### 順番表（`.claude/windows-role.md`「The ARM64 lane」）

無人実行は `.claude/` への書き込みを権限で拒否されるので、変更は PR 本文の `## Queue` に書いた。

- **`21 / 32 / 37. archives and openers, again` の行は消してよい。**x64 で `[x]` の 21 行は
  全部この機械で押し直し、結果を上の表に書いた。`[ ]` のまま残るのは
  **秀丸・サクラ・IrfanView が無いと押せない行**（32.2 / 32.5 / 32.8a / 32.8b / 32.9、37.7 / 37.8 の一部）と
  **21.4**（持ち主の判断待ち）で、**どれも ARM64 の話ではない。**
- `the test suite` の行はそのまま残す（この run も 506 / 0 を記録した）。
- **代わりに 2 つ足してほしい**（どちらもこの run で足場を確かめたもの）:
  - **22. opening an editor at a line** —— `Win32_Process` の `CommandLine` で
    filer が組んだ `-n42` / `+42` / `--goto` が読める。この機械には **VS Code と Neovim が
    入っていて、秀丸・サクラ・IrfanView は入っていない**（上の表）。6 行のうち取れるのは
    その 2 つぶんで、残りは「入っていない」と報告する行になる。
  - **7. the config paths in the help panel** —— 並ぶディレクトリはただのテキストで、
    `YAZI_CONFIG_HOME` / `FILER_CONFIG_HOME` で動かせる。**この run は 5 つの設定を
    その 2 つの環境変数で差し替えて回した**ので、足場はもう分かっている。7.2 / 7.3 は
    ポインタと強調なので見た目。

## TESTING.md 1.9g / 45.11 と lazygit の遅れ — v0.54.5 を ARM64 で確かめた（cfb75ed / 0.54.5、ARM64 レーン）

ARM64 レーンの 7 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
順番表の先頭「**1.9g, 45.11 and the lazygit lag, after v0.54.5**」が担当。

**1.9g / 45.14 / 45.15 にチェックを付けた。45.11 は付けていない** —— 昇格していないこの機械では
シンボリックリンクが作れず（下の「45.11」）、**ジャンクションでの同じ比較は合格した**。lazygit の
遅れは **filer の側ではない**と言い切れるところまで読めた（下の「lazygit の遅れ」）。
`make-testcheck -- --check` は `in sync`（147 / 401）、`make-keycheck -- --check` も `in sync`（243 / 247）。
提案 5 件。

### この機械

| | |
| --- | --- |
| 機械 | `PROCESSOR_ARCHITECTURE=ARM64`、Adreno X2-90（Vulkan） |
| OS | `Windows 11 Home 26H1 (build 28000.2956)` |
| rustc | 1.98.1 (48a229cea 2026-09-01) / aarch64-pc-windows-msvc —— CI と同じ stable |
| filer | 手元ビルド 0.54.5、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| 昇格 | **無し**（`IsInRole('Administrators')` = False、開発者モードも off）。45.11 が落ちる唯一の理由 |
| `cargo test` | **509 passed; 0 failed**（ネイティブ ARM64、2.67 s、cfb75ed）。x64 ランナーに無い失敗は無い |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |
| 入力デスクトップ | **スクリーンセーバが握っていた**（`OpenInputDesktop` = `Screen-saver`、`SPI_GETSCREENSAVERRUNNING` = True、`LogonUI` は 0 本）。#88 と同じ状況 |

生の証拠は `C:\dev\filer-evidence\arm-1.9g-45.11\`（`shots\` に 74 枚 —— 窓ぜんぶと、読んだところを切り出した `strip-*` / `*-rows` / `*-foot`、`runs\<tag>\pty.log`、
足場の `lib.ps1` / `find.ps1` / `shot.ps1` / `desk.ps1`、フィクスチャの `fx45.ps1` / `fx4514.ps1`、
`run19.ps1`）。

### 測り方

- **キーは全部 `--keys` で入れた。**スクリーンセーバが入力デスクトップを握っているので `SendInput` は
  届かず、`PostMessage` は修飾キーの状態を運ばないので `<C-S-f>` / `<C-S-n>` / `<C-S-b>` / `<A-d>` が
  作れない。`--keys` は窓の外の事情と無関係に押せる。
- **`--keys` は 1 キーを 2 フレームおきに押すので、押した「途中」は撮れない。**だから
  **状態ごとに別の run** にした: 同じ前半に別の後半を足した filer を順に起動し、script が流れ切って
  から `PrintWindow(PW_RENDERFULLCONTENT)` で 1 枚撮る。スクリーンセーバの下でも撮れる。
- **読む側**は 3 つ: スクリーンショットの**テキスト**（ペインの注記 `N lines back`、比較ビューの
  行とフッタ）、`FILER_PTY_LOG`（どのキーがシェルに届いたか）、ディスク（`Get-ChildItem -Attributes
  ReparsePoint` のリンク先）。
- 設定は run ごとに空の `FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` / `FILER_STATE_HOME` を渡したので、
  この機械の `keymap.toml`（`T` の再割り当て）は混ざっていない。

### 1.9g —— `<C-S-b>` は 1 回で戻る（#93 の不具合 1 は直っている）

`many\`（500 ファイル）で filer を起こし、`<C-t>` → `dir<Enter>` → `<S-End>`×30（既に最下部なので
何もしない埋め草。`dir` の 500 行が出切るまでの間を作る）→ `<C-S-f>` → `item-05` → `<Enter>`。
そこから後半だけを変えた 6 本。読むのは**ペイン右上の注記**と**ペイン最上行のファイル名**。

| run | 後半 | 注記 | ペイン最上行 | 証拠 |
| --- | --- | --- | --- | --- |
| 19a | （検索まで） | `433 lines back — <S-End> to return` | `item-059.txt`（`item-05` が強調） | `19a-note.png` |
| 19b | `<C-S-n>` | `434 lines back` | `item-058.txt` | `strip-b.png` |
| 19c | `<C-S-n><C-S-b>` | **`433 lines back`** | **`item-059.txt`** | `strip-c.png` |
| 19e | `<C-S-n><C-S-n><C-S-b>` | `434 lines back` | `item-058.txt` | `strip-e.png` |
| 19f | `<C-S-n><C-S-n><C-S-b><C-S-b>` | `433 lines back` | `item-059.txt` | `strip-f.png` |

**19a と 19c が同じ**なので、`<C-S-n>` 1 回は `<C-S-b>` 1 回で戻る。19e / 19f は上へ 2 回いってから
1 回ずつ下りるので、`<C-S-b>` が毎回ちょうど一致 1 つぶん動くことを示す。#93 の表（`<C-S-b>` を
2 回押して初めて 433 に戻る）とは違う動きで、**v0.54.5 の修正は ARM64 でも効いている。**
`FILER_PTY_LOG` の `in key` は `d` `i` `r` `\r` の 4 本だけ —— 和音も `item-05` の 7 文字も
シェルへは 1 バイトも行っていない。

補足（不具合ではない）: 19d = `<C-S-n><C-S-b><C-S-b>` は `442 lines back` / `item-050.txt` になる
（`strip-d.png`、別の run `strip-d3.png` でも同じ）。`item-05` に当たるのは `item-050`〜`item-059` の
10 行だけで、`item-059` が一番下なので、そこから**下向きに 2 回目**は行き先が無い。
alacritty の `search_next` が**自分で端から回り込む**ので一番上の `item-050` に着く。
**そのとき `Wrapped` のトーストは出ない**（`19d3-t7`〜`t11` の右上、トーストの寿命 6 秒の間に 5 枚撮って
1 枚も写っていない）。`term_find` の `Wrapped` は alacritty が `None` を返したときだけなので、
回り込みが中で済むと黙る。下の「Proposals」5 番。

### 45.11 —— シンボリックリンクは作れない。ジャンクションでは合格

**この run は昇格していないので、シンボリックリンクが 1 本も作れない。**
`New-Item -ItemType SymbolicLink` は `この操作には管理者特権が必要です`、`mklink` は
`You do not have sufficient privilege to perform this operation`、開発者モードも off
（`AllowDevelopmentWithoutDevLicense` が無い）。**だから 45.11 にはチェックを付けていない。**

ジャンクションは権限が要らず、filer から見ると**同じ道を通る**
（`symlink_metadata().is_symlink()` は Windows のジャンクションにも真、`read_link` も読める。
`diff.rs` の doc コメントと `junctions_compare_by_where_they_point` がその形）。
そこで 3 組を作って `<Space><Space><A-d>` で比べた（`fx45.ps1`、中身はどの `t1` / `t2` も
`same bytes` の 10 バイトで同一）。

| 組 | 左の `ln` | 右の `ln` | 行 | フッタ | 証拠 |
| --- | --- | --- | --- | --- | --- |
| `differ` | `L\t1` | `R\t2` | **`~ ln  0 B → 0 B`** | `0 only left · 0 only right · 1 differ · 4 match` | `45-differ.png` |
| `same` | `L\t1` | `L\t1` | `= ln` | `0 only left · 0 only right · 0 differ · 5 match` | `45-same-rows.png` / `-foot` |
| `copies` | `L\t1` | `R\t1` | `~ ln  0 B → 0 B` | `0 only left · 0 only right · 1 differ · 4 match` | `45-copies-rows.png` / `-foot` |

- `differ` が 45.11 の形そのもの。**指す先の中身は両側同じなのに「異なる」と読める** ——
  v0.54.5 より前は `= ln` で `3 match`、ディレクトリへのリンクは `? dl`（大きすぎて読めない）だった。
  ジャンクションでは**直っている。**
- `same` は同じ行き先どうしで `= ln`。中身を読まずに行き先で比べていることの裏。
- `copies` は下の「見つけたもの」。

### 見つけたもの: 同じツリーの 2 つの複製が、ジャンクション 1 本のぶんだけ「異なる」と出る

`copies` は**同一のツリーを 2 つ**作り、それぞれの `ln` が**自分の側の** `t1` を指している
（`L\ln → L\t1`、`R\ln → R\t1`）。人の目には同じツリー 2 つだが、結果は `~ ln` の
`1 differ · 4 match` で、45.5 の「同一のツリー 2 つ → 全部 `=`」と食い違う。

- 原因は**ジャンクションの行き先が必ず絶対パス**であること。`read_link` は
  `…\copies\L\t1` と `…\copies\R\t1` を返すので、文字列としては違う。
  シンボリックリンクなら相対で作れるので、この食い違いは Windows のジャンクション固有。
- どちらが正しいかは**持ち主の判断**だと思う。「リンクの行き先が違う」は事実だし、
  「同じ木の複製なのに差分が出る」も事実。**直すなら**、行き先が自分の側のルートの下にあるときは
  ルートからの相対で比べる（`t1` と `t1` になり `=`）。**直さないなら**、行に行き先を出せば
  少なくとも理由は読める（下の Proposals 2 番）。
- **2 つの作業コピー（`C:\dev\filer` と `C:\dev\filer-armtest` のような）を比べる**のが
  一番ありそうな使い方なので、黙って 1 行だけ差分が立つのは気づきにくい。

### lazygit の遅れ（#93 の 2 番）—— filer は待たせていない

順番表の指示どおり、#93 の証拠の `pty.log` で `<Esc>`（`in key \e[27;1;27;1;0;1_`）の時刻と
lazygit が描き直す `out` の時刻を比べた。時刻は `== pane opened` からのミリ秒。

**対照（`r131c-pty.log`、`<S-End>` を流していない run）**

| 時刻 | 何 |
| --- | --- |
| 8647 | `in key ?` |
| 8661 | `out` 2505 バイト —— キーバインディングの枠が開く（**14 ms**） |
| 11978 | `in key \e[27;1;27;1;0;1_`（`<Esc>`） |
| 12103 | `out` 2306 バイト —— 枠が消え、フッタが `ステージ…` に戻る（**125 ms**） |

**`<S-End>` を 30/s で流した run（`r131b-pty.log`）**

| 時刻 | 何 |
| --- | --- |
| 9929 | `in key \e[1;2F`（直前の `<S-End>`） |
| **9961** | **`in key \e[27;1;27;1;0;1_`（`<Esc>`）** |
| 10006 | `in key \e[1;2F`（次の `<S-End>`） |
| 10017, 10037, 10049, … | `out` 1648〜1657 バイトの描き直しが**止まらずに続く。フッタは `実行 / 閉じる / キャンセル` のまま** |
| 62658（ログ末尾） | **まだ `実行 / 閉じる / キャンセル`** —— `<Esc>` から **52.7 秒後** |

- **`<Esc>` は時刻どおりに出ている。**前のキーから 32 ms、次のキーまで 45 ms で、流していた
  30/s の間隔そのもの。filer が溜めたなら、ここが空くか、後ろにずれる。
- もっと強い証拠として、**その前後ずっと、`<S-End>` 1 本ごとに 12〜16 ms で lazygit の
  描き直しが返っている。**PTY は詰まっておらず、lazygit は読んで描いている。
  つまり `<Esc>` は**流れている管**に、**時刻どおりに**入った。
- **結論: filer の側ではない。**遅れているのは lazygit（か、その手前の ConPTY の入力経路）で、
  v0.54.5 の「コードを読んだ限り filer は溜めていない」という見立てと一致する。
  `feed_term_key` が `term.send` でその場で書いていることは、`in key` の時刻が示している。
- **どちらが悪いかまでは決めていない。**ここで言えるのは「filer は渡した」までで、
  lazygit が `\e[1;2F` を 165 本さばく間に `\e` を握っているのか、win32 入力モードの
  `\e[27;…_` と生の `\e[1;2F` が混ざっているのが効いているのかは、この 2 本のログからは分からない。
  切り分けるなら、`<S-End>` の代わりに**lazygit が理解する別のキー**を同じ速さで流して
  同じことが起きるかを見るのが次の 1 手。

### ついでに取った 45.14 / 45.15

順番表の行には無いが、同じ節で足場がそのまま使え、期待値が全部テキストなので取った。

- **45.14** —— 300 ファイルずつの 2 つのツリーで `f250.txt` だけが違う（`fx4514.ps1`）。
  `<Space><Space><A-d>` で開いた直後のカーソルは **`~ f250.txt  10 B → 7 B`** の上にあり、
  一覧は `f222` あたりまでスクロールしている。フッタは `0 only left · 0 only right · 1 differ ·
  299 match`（`4514-far-mid.png`）。差分の無い 300 組では**先頭の `= f000.txt`** の上で開く
  （`4514-nodiff-rows.png`）。両方確かめたのでチェックを付けた。
- **45.15** —— 20 ファイル中 `a005` / `a010` / `a015` が違う組で、`z` の前後を 5 本の run で撮った。
  `z` を押すと一覧は `~` の 3 行だけになり、フッタは **`0 only left · 0 only right · 3 differ ·
  17 match · matches hidden (z)`**（`4515-z1-rows.png` / `-foot`。17 件を数えたまま `matches
  hidden (z)` が付く）。`j` は `a005` → **`a010`**（隠れている `a006`〜`a009` を踏まない、
  `4515-z2-top.png`）、`n` も同じく `a010`（`4515-z4-top.png`）。もう一度 `z` を押すと
  20 行が戻り、**カーソルは `~ a010.txt` のまま**（`4515-z3-mid.png`）。チェックを付けた。

### Proposals

#### 1. 存在しないパスを渡された filer は、黙って親を開く

- **何が起きたか**: 足場の変数名を `$F` と `$f` で衝突させて（PowerShell の変数は大小を区別しない）、
  filer に壊れたパスを渡してしまった。filer は**何も言わずに別のディレクトリを開き**、
  その上で `<Space><Space><A-d>` が走って、**まったく無関係な 2 つのフォルダの比較結果**が
  撮れていた（`4514-nodiff` の 1 本目。見出しが `assets ↔ docs` で初めて気づいた）。
  確かめ直すと、`filer C:\…\no-such-directory` は**その親**を開く（`badpath-top.png`。
  トーストは出ない。3 秒後のスクリーンショットで、寿命 6 秒のトーストは写っていない）。
- **どうするべきか**: 落ちる先が渡されたパスと違うときは、トーストで
  `C:\…\no-such-directory is not there — showing C:\…\filer-scratch` のように 1 行言う。
  親へ落とす動き自体はそのままでよい（綴り間違いの近所が見えるのは親切）。
- **なぜ**: 黙って別の場所を開くのは、**うまくいったように見える**のが一番まずい。
  スクリプトから起動する人（この run のような足場や、ランチャ）は、名前を間違えた瞬間ではなく
  結果を読んだ後で気づく。
- **大きさ**: `start_unproven` の落ち先が決まるところに 1 行。

#### 2. リンクの行は `0 B → 0 B` としか言わない

- **何が起きたか**: 45.11 の `differ` で行き先だけが違うジャンクションの行は
  **`~ ln  0 B → 0 B`**。大きさが両方 0 なので、「違う」と言いながら**なぜ違うのかは読めない**。
  `copies`（同一ツリーの複製）でも同じ見た目になるので、**差分が正しいのか自分の作り間違いなのかを、
  行からは判断できなかった**（フィクスチャを作り直して確かめた）。
- **どうするべきか**: TODO.md に既にある形 —— `~ ln  → t1 | → t2` —— を出す。
  長すぎるなら末尾だけでもよい。
- **なぜ**: この行は「中身を読まずに行き先で比べた」という **v0.54.5 の修正そのものの出力**で、
  それが読めないと直ったかどうかが分からない。
- **大きさ**: 行を組み立てるところ 1 か所（TODO.md に未完として残っている項目）。

#### 3. 比較ビューの見出しの `n/N differences` が、数に見える

- **何が起きたか**: 見出しは `L ↔ R — n/N differences, z hides matches, q to close`。
  最初にこれを撮ったとき、**`1/5` のような数がまだ埋まっていないバグだと思って**
  `overlay.rs` を読みに行った。`n` と `N` は**キーの名前**で、その 2 つで差分を渡り歩けという意味だった。
- **どうするべきか**: キーだと分かる書き方にする。`n / N for the next difference, z hides matches,
  q to close` など。ほかの 2 つ（`z hides matches` / `q to close`）は「キー＋動詞」で書かれていて、
  ここだけ形が違う。
- **なぜ**: 見出しは初めて開いた人が唯一読む説明で、そこが数字に見えるとキーだと気づけない。
- **大きさ**: 文字列 1 本（`overlay.rs:880` と `882`）。

#### 4. `--keys` に「待つ」が無いので、途中の状態を撮るには run を分けるしかない

- **何が起きたか**: 1.9g は「押すたびに注記がどう変わるか」を見る行なのに、`--keys` は
  1 キーを 2 フレームおきに押し切ってしまうので、**途中で撮れない**。同じ前半を持つ
  **6 本の filer を順に起動**して、それぞれの最後を撮って並べた（1 本あたり 12 秒の待ちで、
  この節だけで 7 回の起動）。45.15 でも同じ理由で 5 本立てた。
- **どうするべきか**: script に待ちのトークンを足す —— `<Wait>`（1 秒）か `<Wait500>`（ミリ秒）。
  `keyscript::parse` が読み飛ばし、`raw_input_hook` がその間だけ `script_at` を伸ばせばよく、
  「動いている filer に外から入り口を作らない」という Q24 の約束は変わらない。
- **なぜ**: 実機の run のほとんどは「押す → 読む → また押す」で、今はそれが
  **run の本数**になっている。1 本にまとまれば、撮る枚数も待ちも減り、同じ状態から続けて押せる。
- **大きさ**: パーサに 1 トークン、`raw_input_hook` に数行。

#### 5. 端で回り込んだ検索が黙る

- **何が起きたか**: 19d（上の 1.9g の補足）。`item-05` の一致 10 個の一番下から
  さらに下へ `<C-S-b>` を押すと、**一番上の一致に飛ぶのに `Wrapped` が出ない。**
  注記は `433` から `442 lines back` に変わるだけなので、**1 つ進んだのか端から回ったのかが読めない。**
  filer の `Wrapped` は alacritty が `None` を返したときだけで、alacritty の `search_next` は
  自分で回り込むので、返ってくるのは常に一致。
- **どうするべきか**: 見つけた位置が**押した向きと逆**へ動いたときも `Wrapped` と言う
  （`search_in` は動く前の `found` を持っているので、比べるだけで分かる）。
- **なぜ**: 「次の一致」を押したつもりが 500 行跳んでいる、というのは戻れなくなる類の驚きで、
  1.9h（無い語には赤いトースト）と同じ理由でここも黙るべきではない。
- **大きさ**: `search_in` の戻り値に 1 ビット、`term_find` で 1 分岐。

## lazygit の遅れ（#93）— 原因まで辿った（c86d434 / 0.54.8、ARM64 レーン）

ARM64 レーンの 8 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
順番表の先頭「**the lazygit lag (#93)**」の (a) (b) (c) を担当。

**原因が分かった。filer は遅れていないが、**#93 の遅れは**やはり filer の側**で起きている。
`<Esc>` の直後（31 ms 後で落ち、127 ms 後なら通る）に、転送されたスクロールキーが **1 つでも**届くと、
**lazygit はその `<Esc>` を永久に落とす**（228 秒待っても閉じない）。同じ塊を filer を通さずに
lazygit のコンソールへ直接入れると、**111 ms で閉じる**。チェックは付けていない（この行は
TESTING.md の節ではなく測定で、`[ ]` の行を新しく通したわけではない）。
`make-testcheck -- --check` は `in sync`（147 / 401）、`make-keycheck -- --check` も `in sync`（243 / 247）。
`cargo test` は **509 passed; 0 failed**（ネイティブ ARM64、6.59 s、c86d434）。提案 4 件。

### この機械

| | |
| --- | --- |
| 機械 | `PROCESSOR_ARCHITECTURE=ARM64`、Adreno X2-90（Vulkan） |
| OS | `Windows 11 Home 26H1 (build 28000.2956)` |
| filer | 手元ビルド 0.54.8、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| lazygit | **0.62.2 / arch=arm64**（`build date=2026-06-04`、winget）。**#93 と同じ実行ファイル**（`lazygit.exe` の更新日時が 2026-06-04 で、#93 は 2026-09-30 の実行） |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ |
| 昇格 | **無し**（`IsInRole('Administrators')` = False） |
| 入力デスクトップ | **スクリーンセーバが握っていた**（`OpenInputDesktop` = `Screen-saver`、`SPI_GETSCREENSAVERRUNNING` = True、`LogonUI` は 0 本）。#88 と同じ |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |

生の証拠は `C:\dev\filer-evidence\arm-lazygit-93\`（`runs\<tag>\pty.log` 25 本、`runs\<tag>\foot.csv`
にフッタ帯のハッシュ列、`logs\` に keyprobe の記録 9 本、`shots\` に切り出し 22 枚、
足場の `lib.ps1` / `pair.ps1` / `outs.ps1` / `foot.ps1` / `runa*.ps1` / `runb*.ps1` / `runc.ps1`）。
窓の写真は `prune.ps1` で間引いた（判断の根拠が残る形は `prune.ps1` の先頭に書いた）。

### 測り方

- **キーは全部 `--keys`。**スクリーンセーバが入力デスクトップを握っているので `SendInput` は届かない（#88）。
- **時計は 1 つ。**`FILER_PTY_LOG` の `== pane opened at <unix ms>` と `keyprobe.ps1` の各行の時刻が
  同じ Unix ミリ秒なので、`in key` と、プログラムがそれを読んだ時刻を引き算できる（v0.54.6 の仕掛け）。
- **「キー一覧が開いているか」はテキストで読む。**lazygit のフッタは閉じていれば
  `コミット: c ¦ … ¦ キーバインディング: ?`、開いていれば `実行: <enter> ¦ 閉じる/キャンセル: <esc>`。
  窓の写真の下端 32 px をハッシュして並べ、**変わったところの前後だけ**を目で読んだ（`foot.ps1`）。
- **間を作る埋め草は `<A-Up>`**（`term_cd`）。`[term]` に束縛されていて `TermScroll` ではないので、
  `app.rs:4310` の `scrolls` が偽になり、**代替画面でも filer が食べてプログラムへ行かない**
  （実測: どの run でも `in key` に `\e[1;3A` は 0 本）。`<S-End>` は代替画面では転送されるので埋め草に使えない。

### (a) filer と ConPTY は 1〜2 ms で渡している

`pwsh -File scripts\keyprobe.ps1 -AltScreen [-Win32]` をペインで動かし、`<S-End>` を流して、
filer が書いた `in key \e[1;2F` と keyprobe が読んだ記録を突き合わせた（`pair.ps1`）。

| run | 転送された `<S-End>` | 1 本目の遅れ | 80 本目 | 165 本目 | 全体 |
| --- | --- | --- | --- | --- | --- |
| `a-vt`（VT 入力のみ） | 164 | 9 ms | 1 ms | 1 ms（164 本目） | min 0 / max 9 / 平均 **1.5 ms** |
| `a-win32`（`\e[?9001h` も） | 167 | 2 ms | 4 ms | 1 ms | min 1 / max 17 / 平均 **1.7 ms** |
| `pilot`（378 本） | 378 | 11 ms | 1 ms | 1 ms | min 0 / max 23 / 平均 **1.3 ms** |

filer は 33.4 ms 間隔で書き、プログラムは同じ 33.4 ms 間隔で受け取っている。**溜まらない。**

**`<Esc>` も同じ。**`a3` は 165 本のあとに `<Esc>`、`a4` は `<Esc>` のあとに**さらに 90 本**。

| run | filer が書いた | keyprobe が読んだ | 差 | 何として届いたか |
| --- | --- | --- | --- | --- |
| `a3-vt` | +10636 ms | +10639 ms | 3 ms | **`chars: \e`** —— ConPTY が win32 レコードを**裸の ESC 1 個に潰した**（離したレコードは無い） |
| `a3-win32` | +10959 ms | +10960 ms | 1 ms | `chars: \e[27;1;27;1;0;1_`（そのまま通る） |
| `a4-vt` | +9993 ms | +9995 ms | **2 ms** | `chars: \e` —— **後ろに 90 本控えていても遅れない** |
| `a4-win32` | +10323 ms | +10325 ms | **2 ms** | `chars: \e[27;1;27;1;0;1_` |

**順番表の「`down vk= 35` のレコードと比べる」は、この道では成り立たない。**
keyprobe は `ENABLE_VIRTUAL_TERMINAL_INPUT` を立てるので、ConPTY は `\e[1;2F` を
**文字レコードのまま**渡す（`chars:` の行）。仮想キーを持つレコードは 1 つも来ない
（`a-win32` の全 170 行の内訳は `chars: \e[1;2F` が 167、`chars: q` が 1、`up vk= 13`(Enter) が 1、見出しが 1）。
`-Win32` で mode 9001 を頼んでも変わらない。**下の Proposals 4 番。**

### (b) 遅れは再現する。ただし #93 の本文どおりにやると再現しない

**#93 の本文（「165 本流したあとで `?` → `<Esc>`」）では 113 ms で閉じる。**
節を変えて 7 本走らせ、`<Esc>` を書いた時刻と、キー一覧を消す描き直し（`out`）の時刻を比べた。

| run | ペイン | 塊 | `<Esc>` | 閉じる描き直し | 差 |
| --- | --- | --- | --- | --- | --- |
| `b2` | 最大化 | 165 | +5925 ms | +6045 ms | **120 ms** |
| `b3` | 最大化 | 166 | +5918 ms | +6031 ms | **113 ms** |
| `b4` | 既定（1/3） | 165 | +8784 ms | +8897 ms | **113 ms** |
| `b6` | 既定 | 165、`?` の 2 秒後に `<Esc>` | +10793 ms | +10906 ms | **113 ms** |
| `b5-500` | 既定 | **500** | +19971 ms | +20084 ms | **113 ms** |
| `b7-oldconpty` | 既定 | 165、**同梱 ConPTY 無しの `filer.exe` 単体** | +10796 ms | +10908 ms | **112 ms** |

500 本流しても 113 ms。**溜まった仕事が `<Esc>` の前に並ぶ、という話ではない。**
（`b6` は 2 秒の間を空けたので、キー一覧が開いている写真も撮れている:
shot 29 `+9023 ms` が開、shot 35 `+10915 ms` が閉。`b4` は shot 28 `+8871 ms` が開、shot 29 `+9145 ms` が閉。）

**#93 のログを読むと、本文と手順が違う。**`arm-1\logs\r131b-pty.log` を数えると
`<S-End>` は **536 本**、`+2062` から `+19970 ms` まで。`?` は `+6929`、`<Esc>` は `+9961` で、
**どちらも塊の途中**であり、`<Esc>` のあとにまだ **約 300 本**が控えている。
本文の「165 本ほど」は、`?` を押すまでに入った本数（+2062〜+6929 ≒ 146 本）のことだった。

**その形で組み直すと、再現する**（`b8` / `b9-long`: 146 本 → `?` → 90 本 → `<Esc>` → 300 本）。

| | `b8` | `b9-long` |
| --- | --- | --- |
| `?`（一覧が開く） | +8113 ms（shot 17 `+8511` で開） | +8134 ms（shot 9 `+9010` で開） |
| `<Esc>` | +11155 ms | +11172 ms |
| 最後のキー | +21186 ms | +21171 ms |
| 観測の終わり | **+90 s、まだ開いている** | **+239 s、まだ開いている**（`shots\b9-foot-239.png`） |

**#93 の「60 秒後には閉じている」より悪い。閉じない。**`b9-long` は 240 秒撮り続けて、
フッタ帯のハッシュは shot 9 から shot 239 まで同じ（`ECDEE8E87D36`。61〜64 / 121〜124 / 181〜184 秒に
一瞬だけ別のハッシュが出るが、これは lazygit の 60 秒ごとの更新で同じ文字列が横にずれるだけで、
その前後は同じ値に戻る）。**`<Esc>` は遅れているのではなく、失われている。**

#### どれだけあれば失われるか —— **1 本**

| run | `<Esc>` のあと | 結果 |
| --- | --- | --- |
| `b4` / `b5-500` / `b6` | 何も来ない | **113 ms で閉じる** |
| `b11x` | **`<S-End>` を 1 本だけ**（31 ms 後） | **閉じない**（15 秒撮って変化なし。`out` も +11204 で止まる） |
| `b13-spaced` | 100 本、1 本目が **31 ms** 後、以降 95 ms 間隔 | 閉じない |
| `b14-space15` | 60 本、1 本目が **31 ms** 後、以降 **532 ms** 間隔 | 閉じない |
| `b15-pregap3` | **127 ms 空けてから** 60 本（33 ms 間隔） | **閉じる**（shot 22 `+11015` → shot 23 `+11501`） |
| `b15-pregap6` | **237 ms 空けてから** 60 本 | **閉じる**（同じ） |

**効くのは量でも速さでもなく、`<Esc>` の直後にもう 1 本来るかどうか**で、
境目は **31 ms と 127 ms の間**にある（`b14-space15` が効いている ——
2 本目以降が 532 ms 間隔でも、**1 本目が 31 ms 後にある**というだけで落ちる）。`terminal.rs:160` の doc コメントがその理由を先に書いている ——
tcell は ESC を受け取ると「続きがあるか」を **50 ms** 待ち、その間に来た ESC で前の ESC を Alt の
前置きに変える。**`<S-End>` は `\e[1;2F`、つまり ESC で始まる。**だから 33 ms 間隔で流れている限り、
`<Esc>` は毎回その窓に捕まる。

### (c) filer を通さなければ、同じ塊でも 111 ms で閉じる

順番表は「Windows Terminal で `SendInput`」と言っているが、**この機械では `SendInput` が届かない**
（スクリーンセーバが入力デスクトップを握っている、#88）。そこで**もう 1 段プログラム側**から入れた ——
lazygit を自分のコンソールで起動し、`AttachConsole` してから `WriteConsoleInputW` で
キーレコードを直接入力バッファに書く（ConPTY が書き込む先と同じバッファ）。読むほうも
`ReadConsoleOutputCharacterW` でフッタの 1 行を**テキストのまま**取った（`runc.ps1`、`logs\c2.txt`）。

```
1790761335813 sent 90 more  footer: [実行: <enter> | 閉じる/キャンセル: <esc> …]
1790761335815 sent <Esc>  footer: [実行: <enter> | 閉じる/キャンセル: <esc> …]
1790761335927 list closed 111 ms after <Esc>, at key 3 of the tail
1790761345900 sent 300 more  footer: [コミット: c | スタッシュ: s | … キーバインディング: ? …]
```

**`<Esc>` のあとに 300 本の Shift+End が 30/s で流れている最中に、111 ms で閉じている。**
filer を通した `b8` / `b9-long` と**同じ形の入力で、結果が正反対**。

### 見つけたもの: filer は「win32 入力モードは知らない」と答えたうえで、win32 レコードを送っている

(c) の差がどこから来るかの手がかりが、ペインの最初のやりとりに残っている。

```
       4 out       \e[c\e[?1004h\e[?9001h      ← lazygit が win32 入力モードを立てる
     322 out       \e[?1006$p\e[?9001$p        ← 立ったか問い合わせる（DECRQM）
     348 in reply  \e[?9001;0$y                ← filer の答え: 0 = そんなモードは知らない
```

`0` は alacritty が未知のモードに返す既定の答えで、filer は間に入っていない。
そのあとで filer は `<Esc>` を **`\e[27;1;27;1;0;1_`（win32 入力モードのレコード）** で送る
（`terminal.rs:176` の `win32_key`）。**「知らない」と答えたモードの書式で送っている。**

本物のコンソールでは lazygit はキーレコードそのものを受け取るので、ESC に曖昧さが無い ——
(c) が 111 ms で閉じたのはそれで説明が付く。ペインでは ESC が**バイト列**になり、
次に来る `\e[1;2F` と区別できなくなる。**どちらを直すかは持ち主の判断**なので Proposals に回した（1 番）。

### この節の結論

- **(a) filer は待たせていない。**165 本でも 500 本でも、`<Esc>` の後ろに 90 本控えていても、
  プログラムに届くまで **1〜3 ms**。#98 の結論（PTY ログの時刻から「filer は渡した」）は正しい。
- **(b) それでも #93 は filer の側で起きる。**渡し方（ESC をバイト列にすること）が、
  直後の 1 本で壊れる。**量ではなく、`<Esc>` の直後 31 ms に 1 本来るかどうか。**
- **(c) filer を外すと起きない。**同じ lazygit、同じ塊、同じ機械で 111 ms。
- **人にも起こる。**`<S-End>` や矢印キーをリピートさせながら `<Esc>` を押す ——
  たとえば「行き過ぎたので Esc で抜ける」——だけで、キーが 1 本後ろに残れば一覧は二度と閉じない。
  #93 の「TUI が固まったように見える」はこれ。

### Proposals

#### 1. 代替画面のプログラムへ `<Esc>` を渡したら、次のキーを 60 ms 待つべき

- **何が起きたか**: 上の (b)。`<Esc>` の 31 ms 後に `<S-End>` が 1 本届くだけで、lazygit は
  `<Esc>` を落とす。228 秒待っても閉じない（`b9-long`）。127 ms 空ければ閉じる（`b15-pregap3`）。
- **どうするべきか**: `feed_term_key` が `Special::Escape` を送ったら、**その後 60 ms のあいだ、
  ESC で始まるバイト列になるキー**（矢印・Home/End・ファンクション）を送らずに溜める。
  60 ms は tcell の 50 ms に余裕を足した値で、人の打鍵では気づけない。
  溜めるのは**代替画面のときだけ**でよい（普通のシェルでは ESC の曖昧さが問題にならない）。
  もっと安い形なら、**`<Esc>` の直後の 1 フレームだけキーを送らない**でも境目は越える。
- **なぜ**: いまは「Esc を押したら二度と閉じない」で、**復帰する手が無い**。
  filer は既に Esc のために win32 レコードという特別扱いを 1 つ入れている（v0.48.6）が、
  **後ろに何も来ないことを前提にしている**。
- **大きさ**: 送信側に小さなキュー 1 つ。`terminal.rs` の `send` の手前に数行と、タイマ 1 つ。

#### 2. DECRQM に `0` と答えたモードの書式で送るのをやめるか、`1` と答えるか

- **何が起きたか**: 上の「見つけたもの」。lazygit は `\e[?9001h` で win32 入力モードを立て、
  `\e[?9001$p` で確認し、filer（alacritty）は `\e[?9001;0$y` =「知らない」と答える。
  そのうえで filer は Esc を `\e[27;1;27;1;0;1_` で送る。
- **どうするべきか**: どちらかに揃える。**(a)** mode 9001 を受け取ったら記録し、DECRQM に `1` を返し、
  **すべてのキー**を win32 レコードで送る（そうすれば ESC の曖昧さが原理的に消え、1 番も要らなくなる）。
  **(b)** 逆に、レコードで送るのをやめて素の ESC にする（v0.48.5 以前に戻ることになるので、たぶん違う）。
  **(a) を勧める。**本物のコンソールがやっているのはそれで、(c) はそれが効くことを見せている。
- **なぜ**: いまは「知らないと言ったモードで話しかけている」状態で、**相手がどう解釈するかは
  相手任せ**。1 番の 60 ms は対症療法で、これは原因のほう。
- **大きさ**: モードの記録 1 つ（`\e[?9001h` / `l` を見る）、DECRQM の分岐 1 つ、
  `encode` を win32 レコードに切り替える経路 1 本。**設計の判断が要る**ので、
  QUESTIONS.md 向きかもしれない。

#### 3. `--keys` に「待つ」が無い（前の run の 4 番の再掲。今回はもっと効いた）

- **何が起きたか**: 「押す → 読む → また押す」が要る測定を、**run の本数**でやるしかない。
  この節は 25 本の filer を起こしている。さらに悪いことに、**`<Esc>` と次のキーの間**を
  空けたいだけのために、`<A-Up>`（filer が食べるキー）を 3〜15 個並べて時間を作った
  —— `b15-pregap3` の `-PreGap 3` がそれで、**127 ms を作るための 3 キー**という書き方になる。
  埋め草のキーが**たまたま無害**であることに頼っていて、無害でなくなったら測定が壊れる。
- **どうするべきか**: `<Wait>`（1 秒）と `<Wait500>`（ミリ秒）を `keyscript::parse` に足す。
  前の run と同じ提案だが、**今回は「待つ長さそのものが測っている量」**だったので、
  埋め草のキー数で代用するのは精度も意味も落ちる（`<A-Up>` 1 つ = 33 ms という換算を、
  毎回 PTY ログで確かめ直した）。
- **なぜ**: 実機の測定のほとんどがこの形で、いまは run の本数と待ち時間に化けている。
- **大きさ**: パーサに 1 トークン、`raw_input_hook` に数行。

#### 4. `keyprobe.ps1` は仮想キーのレコードを見られない —— 順番表の前提が違う

- **何が起きたか**: 順番表は「`in key \e[1;2F` と、その `down vk= 35` のレコードの差」を読めと言うが、
  keyprobe が立てる `ENABLE_VIRTUAL_TERMINAL_INPUT` のせいで、ConPTY は VT のバイト列を
  **文字レコードのまま**渡す。`a-win32` の 170 行に `vk= 35` は 1 つも無い（`chars: \e[1;2F` が 167）。
  比較そのものは `chars:` の行で問題なくできたが、**書いてあるとおりに読もうとして 1 往復した。**
- **どうするべきか**: `keyprobe.ps1` に **`-NoVt`**（`ENABLE_VIRTUAL_TERMINAL_INPUT` を立てない）を足す。
  そうすれば同じキーを「アプリが VT で読んだ場合」と「レコードで読んだ場合」の両方で見られて、
  2 番の判断（win32 レコードに寄せるか）の材料にもなる。
  順番表の文言は、`-AltScreen` 単体では `chars:` の行になることを書き添えれば足りる。
- **なぜ**: この道具は「ペインと Windows Terminal で同じキーを比べる」ために作られたのに、
  いまは片側しか見られない。
- **大きさ**: スクリプトに `param` 1 つと分岐 1 つ。

---

## TESTING.md section 40 — 全画面プログラムへのスクロールの受け渡しを ARM64 で確かめた（ce258cd / 0.54.9、ARM64 レーン）

ARM64 レーンの 9 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
順番表の先頭「**40. scroll gestures handed to full-screen programs**」の `[ ]` 13 行を**全部**通した。

**10 行は期待どおりで `[x]` を付けた。3 行は期待どおりにならなかった**（40.7 の前半、40.8、40.12）。
うち **40.8 は filer 側の問題**で、残り 2 つは**行の書きかたが実際のプログラムと合っていない**。
`make-testcheck -- --check` は `in sync`（**157 / 401**）、`make-keycheck -- --check` も `in sync`（243 / 247）。
`cargo test` は **509 passed; 0 failed**（ネイティブ ARM64、debug 2.60 s / release 6.40 s、ce258cd）。提案 4 件。

### この機械

| | |
| --- | --- |
| 機械 | `PROCESSOR_ARCHITECTURE=ARM64`、Adreno X2-90（Vulkan）、画面 2560x1440 |
| OS | `Windows 11 Home 26H1 (build 28000.2956)` |
| filer | 手元ビルド 0.54.9、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| プログラム | nvim **0.12.5**、less（Git for Windows `usr\bin`）、bash 5.3（同）、PSReadLine **2.0.0**（`powershell.exe` 5.1、EditMode=Windows） |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ |
| 昇格 | **無し**（`IsInRole('Administrators')` = False。この節は昇格を要らなかった） |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |

生の証拠は `C:\dev\filer-evidence\arm-40\`（`runs\<tag>\pty.log` 11 本、`shots\` に窓とペイン上端の切り出し、
足場の `lib.ps1` / `chord.ps1` / `wake.ps1` / `mouse.ps1` / `post.ps1` / `outs.ps1` / `fx40.ps1` と `r0`〜`r9` の run）。

### 測り方 — この節は「見た目」の行が 1 つも無い

- **キーは 2 つの道で入れた。**修飾キーの付くキー（`<A-j>`、`<S-PageUp>`、`<C-t>`、`Alt-b`…）は **SendInput**、
  修飾キーの無いキー（`<F5>`、`Home`、`PageDown`）は **PostMessage**。`--keys` は filer の起動直後にしか
  押せず、**「nvim が立ち上がってから押す」ができない**ので、この節では起動コマンドを打つところまでにした。
  SendInput の前後で毎回 `GetForegroundWindow` が filer であることを確かめている（`chord.ps1` の `Send-Chord`）。
- **ホイールは SendInput でしか回らない。**下の「見つけたもの 5」。
- **読みは全部テキスト。**
  - **filer が何を送ったか**: `FILER_PTY_LOG` の `in key` 行。転送されたなら `\ej` や `\e[1;2F` が出る。
    **転送されなかったこと**も、その区間に行が 1 本も無いことで読める。
  - **プログラムが受け取ったか**: nvim の `nnoremap ... :call writefile(...)`（`cfg.vim`）。
    押したキーの名前が `mark.txt` に 1 行ずつ増える。`<F5>` は `line('.')` と `line('w0')`（画面最上行）を書く。
  - **シェルの行編集がどう動いたか**: PTY ログの `out` 行。`echo ab cd` の echo と、実行した行の出力が
    そのまま入っているので、`Alt-b` の後に `X` を打てば `ab Xcd` か `ab cdX` かが読める。
  - **スクロールバックがどこまで動いたか**: ペインに `1..300` を流し、窓の写真の**左上 3 行の数字**を読む
    （`Save-PaneTop`、260x76 px を 2 倍）。`less` / nvim では `L0001` 形式の行番号。
  - **一覧にキーが戻ったか**: `c` `f` の後の `Get-Clipboard`。

### 行ごとの結果

| 行 | 結果 | 根拠 |
| --- | --- | --- |
| 40.1 | **合** | `move.vim` に `nnoremap <A-j> :m+1<CR>`。`in key \ej`（+2348 ms）、`out` に `:m+1` と `BBB2`/`AAA1` の描き直し、`:wq` 後の **move.txt が `BBB2 / AAA1 / CCC3 / DDD4`** |
| 40.2 | **合** | 4 つとも転送された: `\e[5;2~` `\e[6;2~` `\e[1;2H` `\e[1;2F`。mark.txt に `s-pgup 250` `s-pgdn 250` `s-home 250` `s-end 250` |
| 40.3 | **合** | `<C-t>` の後、`in key` に**新しい行が 1 本も無い**。`Get-Process nvim` は 2 本のまま。`c` `f` → クリップボードが `cfg.vim`（一覧がキーを持っている） |
| 40.4 | **合** | `:q!` の後の `<A-k>` 4 回は `in key` に何も残さず、ペイン上端は **291 → 271**（5 行 x 4）。同じ run の nvim 中では `\ek` が出ていた |
| 40.5 | **合** | nvim: 上へ 3 ノッチで `line('w0')` **245 → 240**。less: **L0001 → L0006**、戻して **L0001** |
| 40.6 | **合** | プロンプトでホイール、`in key` は空。上端 **280 → 275**（上へ 3 ノッチ）→ **279**（下へ 3 ノッチ） |
| 40.7 | **否（前半）** | `\e[5;2~` / `\e[6;2~` は less に**渡っている**のに **L0001 のまま動かない**。素の `\e[5~` / `\e[6~` なら **L0001 → L0012**。後半（`q` の後の `<S-PageUp>`）は合で、**291 → 285** |
| 40.8 | **否** | 1 ノッチ上げて 1 ノッチ下げると、上は矢印 **1 本**、下は **0 本**。nvim の行は **245 → 244 → 244**。3 ノッチでは 4/5 の回と 5/5 の回があった |
| 40.9 | **合** | `\e[?1h` を立てたプログラム（`appcur.ps1`、および **less**）では、ホイールが **`\eOA` / `\eOB`** で出た。CSI ではない |
| 40.10 | **合** | bash 5.3: `echo ab cd` → `Alt-b` → `X` → 出力 **`ab Xcd`**。`Alt-b Alt-b Alt-f` → `X` → 出力 **`abX cd`** |
| 40.11 | **合** | 同じ prompt で `Alt-b` `Alt-d` → 出力 **`ab`**（`cd` が消えた） |
| 40.12 | **否** | PSReadLine 2.0.0（EditMode=Windows）に **`Alt+b` / `Alt+f` の割り当てが無い**。`\eb` は行に `b` を差し込む（`echo ab cd` → `echo ab cdb`）。**`Alt+d` と `Ctrl+Left` は効く**（下の「見つけたもの 3」） |
| 40.13 | **合** | プロンプトで `<A-k>` 4 回・`<A-j>` 2 回、`in key` は空。上端 **290 → 270 → 280** |

### 見つけたもの 1: ホイールの端数が向きを変えるときに 1 行落ちる（40.8）

代替画面のホイールは `ui/term.rs` の `wheel_whole` で端数を持ち越し、矢印キーの本数に変える。
**1 ノッチ上げてから 1 ノッチ下げると、上に 1 本、下に 0 本**しか出ない（`runs\40-8\pty.log`、
mark.txt が `line 245 top 245` → `line 244 top 244`）。

| ノッチ | 上に出た矢印 | 下に出た矢印 | nvim の行（前 → 後） |
| --- | --- | --- | --- |
| 1 | 1 | **0** | 245 → **244** |
| 3 | 4 | **5** | 244 → **245** |
| 5 | 8 | 8 | 245 → 245 |
| 3 | 5 | 5 | 245 → 245 |

1 ノッチは実測 **約 1.6 行**。`wheel_whole` は `acc.trunc()` なので、上げで `acc` に 0.6 が残り、
下げで `0.6 - 1.6` が**ちょうど -1.0 のすぐ手前**（`-0.99999994`）になると `trunc` が 0 を返す
——というのが計算の合う説明だが、**確かめたのは本数と行番号のほうで、原因の断定はしていない。**
どちらにせよ行の言う「1 ノッチは両方向とも固定本数の矢印」にはなっていない。
同じ端数はプロンプト側（40.6）にも出ていて、3 ノッチ上げ = 5 行、3 ノッチ下げ = 4 行だった。

### 見つけたもの 2: less は `\e[5;2~` を知らない（40.7 の前半は less の話）

filer は `<S-PageUp>` を `\e[5;2~` で渡していて、**渡すところまでは正しい**（`in key` にある）。
less はそれを読めず、下端に `:` プロンプトを出したまま **1 行も動かない**。
同じ less に素の `PageUp` / `PageDown`（`\e[5~` / `\e[6~`）を入れると **L0001 → L0012** と普通にページ送りする。
nvim は `\e[5;2~` を `<S-PageUp>` として理解する（40.2 が通っている）ので、**プログラム側の差**。

**行の直しかた（人の判断）**: 40.7 を「素の `PageUp`」に変えるか、「less では動かないことを確かめる」に
変えるか。**節の主旨（代替画面のプログラムにスクロールキーを渡す）を確かめたいなら nvim のほうが向いている。**
いまの文言のままだと、`less` を使う人が毎回ここで詰まる。

### 見つけたもの 3: PSReadLine の Windows モードには `Alt+b` / `Alt+f` が無い（40.12）

`Get-PSReadLineKeyHandler -Bound` に `Alt+` で始まる割り当ては `Alt+d`（KillWord）、`Alt+.`、`Alt+F7`、
`Alt+0`〜`Alt+9`、`Alt+-`、`Alt+?` だけ。**`Alt+b` / `Alt+f` は Emacs モードにしか無い。**
だから `\eb` は「割り当ての無いキー」として文字 `b` が差し込まれ、`echo ab cd` が `echo ab cdb` になる。

**filer の側は正しく働いている。**同じ run で確かめた:

```
   3861 in key  \e[H          ← Home
   4606 in key  \ed           ← Alt+d
   4608 out      ab cd        ← KillWord が走り、`echo` が消えた
   5424 in key  \e[F          ← End
   6063 in key  \e[1;5D       ← Ctrl+Left
   6888 out      ab Zcd       ← 単語 1 つ分戻ったところに Z が入った
```

**行の直しかた（人の判断）**: 40.12 を `Alt-d` と `Ctrl-Left` / `Ctrl-Right` に変えるのが実態に合う。
`Alt-b` / `Alt-f` を残すなら「EditMode が Emacs のとき」と書き添えが要る。

### 見つけたもの 4: nvim はアプリケーションカーソルモードを立てない（40.9 の手引きが古い）

40.9 は「挿入モードの nvim が確かめやすい」と言うが、**nvim 0.12.5 は `\e[?1h` を一度も送らない**
（この節の PTY ログ 11 本すべてで 0 回）。だから nvim では、どのモードでもホイールの矢印は CSI（`\e[A`）で出る。
**立てるプログラムでは行のとおりになる**ことは、2 つで確かめた——`less`（`\eOA` / `\eOB`）と、
このために書いた `appcur.ps1`（`\e[?1049h\e[?1h` を出して待つだけのもの。`runs\40-5-8-9\pty.log` の
`13841 out \e[?1049h\e[?1h`、その後の `18142 in key \eOA`）。行の**期待は正しく、例が古い**。

### 見つけたもの 5: ペインへ PostMessage したマウスメッセージは egui に届かない

役割定義は無人実行に `PostMessage` を勧めていて、**キーはそのとおり届く**（この節でも `<F5>` や `Home` は
PostMessage で入れている）。**マウスは届かない。**同じウィンドウハンドルに対して:

| 送ったもの | 結果 |
| --- | --- |
| `WM_MOUSEMOVE` + `WM_LBUTTONDOWN/UP` を一覧の 4 行目へ | カーソルは動かない（`c` `f` が 1 行目の `cfg.vim` を返す） |
| 同じものをペインへ | ペインがキーを取らない（後から入れた `x` が PTY ログに出ない） |
| `WM_RBUTTONDOWN/UP` をペインへ | 貼り付けが起きない（`in paste` が無い） |
| `WM_MOUSEMOVE` + `WM_MOUSEWHEEL` を nvim の出ているペインへ | **1 本も矢印が出ない** |
| 同じ 3 ノッチを SendInput で | `\e[B` が 5 本（`runs\postmouse\pty.log`） |

`PostMessageW` は毎回 `True` を返し、`GetLastError` は 0。**入力デスクトップが自分のものになってからも同じ**
（上の表は全部その状態で測り直したもの）。だから**マウスの行は SendInput でしか測れず、SendInput は
入力デスクトップを要る**——この機械ではスクリーンセーバ（`OLED Care Screensaver.scr`）がそれを握っている（#88）。

**この run はスクリーンセーバのプロセスを終わらせてから測った**（`wake.ps1`。`SetThreadExecutionState`
(`ES_DISPLAY_REQUIRED`) も立てて、途中で戻ってこないようにした）。マウスを動かせば消えるものなので
機械に残るものは何も変えていないが、**測定の前後で毎回 `OpenInputDesktop` が `Default` を返すことを
確かめてから測った**（各 run の 1 行目の `desktop:`）。

もう 1 つ、足場の落とし穴: 64 ビットの `INPUT` は **40 バイト**でなければならない。`MOUSEINPUT` を
入れた構造体に `pad1` / `pad2` を足すと 48 になり、`SendInput` は **0 を返して何もしない**
（最初のホイールはこれで 1 往復した。`KEYBDINPUT` のほうは 24 バイトなので、逆に pad が要る）。

### Proposals

#### 1. マウス報告を有効にしたプログラムには、ホイールを矢印ではなくマウスイベントで渡すべき

- **何が起きたか**: 代替画面のホイールは `\e[A` / `\e[B` を並べて送っている（`ui/term.rs:224`）。
  ところが nvim は起動時に **`\e[?1002h\e[?1006h`**（SGR マウス報告）を出していて、
  ホイールを**マウスとして**受け取る用意がある（この節のどの PTY ログにもある）。
  filer は `1002` も `1006` も見ておらず（`terminal.rs` に文字列すら無い）、矢印で代用している。
- **どうするべきか**: プログラムが `\e[?1000h` / `\e[?1002h` / `\e[?1003h` を立てたら記録し、
  ホイールを SGR 形式（`\e[<64;col;rowM` = 上、`65` = 下）で送る。立てていないプログラムには
  いまどおり矢印を送る。
- **なぜ**: 3 つ一度に直る。**(a)** 40.8 の「上げて下げると戻る」が原理的に成り立つ
  （プログラムが自分のスクロール量で動かすので、端数が filer 側に残らない）。
  **(b)** カーソルが動いてしまう副作用が消える——いまは矢印なので、nvim では**本文がスクロールする前に
  カーソルが移動する**（40.5 の測定で `line('.')` が 245 → 240 と動いた。ホイールを回しただけで
  編集位置が変わるのは、他のどの端末でも起きない）。**(c)** less の `:` プロンプトのような
  「知らないキー」の副作用も減る。
- **大きさ**: モードの記録 1 つ（DECSET のパース、alacritty 側が既に持っている可能性あり）、
  送信の分岐 1 つ、セル座標の計算 1 つ。**設計の判断が要る**（クリックやドラッグも渡すのか、
  ペインの選択コピーとどちらを優先するのか）ので QUESTIONS.md 向きかもしれない。

#### 2. ホイールの端数は、向きが変わったところで捨てるべき

- **何が起きたか**: 「見つけたもの 1」。1 ノッチ上げて 1 ノッチ下げると 1 行ぶんずれる。
  **人が気づく形はこれ**: 行き過ぎたので 1 ノッチ戻す、を繰り返すと、少しずつ下へ流れていく。
- **どうするべきか**: `wheel_whole` で `rows` の符号が `*acc` の符号と違ったら `*acc` を 0 にしてから足す。
  1 行に満たない端数は、向きを変えた時点で「もう要らないもの」で、持ち越す理由が無い。
  （1 番を入れるならマウス報告のプログラムでは使われなくなるが、シェルのスクロールバック側には残る。）
- **なぜ**: いまは「同じ回数だけ戻したのに元の位置に戻らない」で、**気づいたときに直しようが無い**。
  40.6 のプロンプト側でも同じずれ（3 ノッチで 5 行 / 4 行）が出ている。
- **大きさ**: `ui/mod.rs` の `wheel_whole` に 1 行と、テスト 1 つ。

#### 3. Windows の既定のシェルは、`pwsh` があれば `pwsh` にすべき

- **何が起きたか**: 40.12。ペインが起こすのは `C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe` で、
  そこに載っている PSReadLine は **2.0.0**（2018 年）。この機械には **pwsh 7.6.6** が入っていて、
  PSReadLine も新しく、予測入力も効く。**`filer env` は pwsh を見つけられるのに、ペインは 5.1 を起こす。**
- **どうするべきか**: `[term] shell` が空のとき、`pwsh` が PATH にあればそれを使い、無ければ
  `powershell.exe` に落ちる。いまの「プラットフォームの既定」を「見つかった中でいちばん新しいもの」にする。
  `filer env` の表示は `terminal pane, the platform default` から `…, pwsh found on PATH` のように変える。
- **なぜ**: 5.1 の PSReadLine は行編集が目に見えて弱い（この節の測定中も、予測の表示が
  出たり消えたりしてログが読みにくかった）。**入っている新しいほうを使わない既定は、説明が要る既定。**
- **大きさ**: `terminal.rs` の既定を決めているところに `which` 1 回と分岐 1 つ。
  **既定の変更なので CHANGELOG の「変更」に要る。**

#### 4. 無人実行は、スクリーンセーバを終わらせるところまでを仕掛けに入れるべき

- **何が起きたか**: この節はマウスの行が 4 つあり（40.5 / 40.6 / 40.8 と 40.9 の半分）、
  **どれも SendInput でしか測れない**（見つけたもの 5）。この機械ではスクリーンセーバが
  入力デスクトップを握っていて（#88 以来ずっと）、そのままでは 4 行とも測れずに終わっていた。
- **どうするべきか**: `scripts/auto-wintest.ps1` が run を起こす前に
  (a) `*.scr` のプロセスを終わらせ、(b) `SetThreadExecutionState(ES_CONTINUOUS|ES_DISPLAY_REQUIRED)` を立て、
  (c) run が終わったら `ES_CONTINUOUS` に戻す。そのうえで `windows-role.md` の「Unattended runs」に
  「**入力デスクトップは run が自分で取り返してよい。ただし取り返せたことを毎回確かめる**」と書く。
  いまの文言（「スクリーンセーバが出たら、そこから先は数えるな」）は**諦める側にしか倒れていない**。
- **なぜ**: マウスの行は 13 節に散らばっていて（14 節は丸ごとマウス、30 節も半分）、
  **この 1 台が測れないままだと、その全部が人待ちになる。**
- **大きさ**: スクリプトに 10 行ほどと、役割定義に 1 段落。

## TESTING.md section 29 — ターミナルのカレントディレクトリを持ち帰るを ARM64 で確かめた（ca8aecd / 0.54.9、ARM64 レーン）

ARM64 レーンの 10 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
順番表の先頭にあった **40** は #100 で済んでいるので、その次の
「**29. the terminal's directory, brought back**」の `[ ]` 5 行を全部通した。

**4 行は期待どおりで `[x]` を付けた。29.2 だけ付けていない** —— この節の前書きが自分で言うとおり
「what is being tested here is mostly the instructions」で、**その指示が、ペインが既定で起こすシェルでは動かない**から。
`make-testcheck -- --check` は `in sync`（**161 / 401**）、`make-keycheck -- --check` も `in sync`（243 / 247）。
`cargo test` は **509 passed; 0 failed**（ネイティブ ARM64、debug 2.86 s、ca8aecd）。見つけたもの 5 件、提案 5 件。

### この機械

| | |
| --- | --- |
| 機械 | `PROCESSOR_ARCHITECTURE=ARM64`、Adreno X2-90（Vulkan）、窓 1376x899 |
| OS | `Windows 11 Home 26H1 (build 28000.2956)` |
| filer | 手元ビルド 0.54.9、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| シェル | `powershell.exe` **5.1.28000.2952**（ペインの既定）、`pwsh` **7.6.6**、starship **1.26.0**、mise / zoxide / atuin / PSFzf |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ |
| 昇格 | **無し**（`IsInRole('Administrators')` = False。この節は昇格を要らなかった） |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |

生の証拠は `C:\dev\filer-evidence\arm-29\`（`runs\<tag>\pty.log` 4 本、`shots\` 10 枚、
足場の `lib.ps1` と `fx29.ps1`、run の `r1`〜`r4`、`profile-hook.ps1`）。

### 測り方 — この節も「見た目」の行が 1 つも無い

- **一覧がどこへ行ったか**は**窓のタイトル**で読む。`title_format` の既定が `Filer: {cwd}` なので、
  タイトルがそのまま「一覧はどこにいるか」になる。窓は `EnumWindows` でクラス `Window Class` かつ
  タイトルが `Filer:` で始まるものを選ぶ（`MainWindowHandle` は winit の無題の窓を指すことがある、#88）。
- **シェルが現在地を言ったか**は `FILER_PTY_LOG` の `out` 行にある `\e]7;file://…`。
  言わなかったことも、ログに 1 本も無いことで読める。
- **トーストの文面**は `PrintWindow(PW_RENDERFULLCONTENT)` の写真を**テキストとして**読む。
- **キー**は `--keys "<C-t>"` でペインを開くところまでを filer 自身に押させ、その先（シェルが
  立ち上がってから打つ `cd` と `<A-Up>`）は SendInput。入力デスクトップは各 run の前後で
  `OpenInputDesktop` が `Default` を返すことを確かめている。
- **日本語のディレクトリへは `. .\jp.ps1` で入る。**SendInput はキーボードレイアウト経由なので
  日本語の名前は打てない。`Set-Location` はシェル自身の `cd` で、フックは同じように動く。
- **シェルが「打ち終わった」判定**は、**PTY ログが 900 ms 伸びなくなったこと**にした。
  プロンプトの形（`PS …>`）で待つ作りだと **Starship のプロンプト（`❯` 1 文字）で永久に当たらない**。
  最初の 29.5 の run はこれで 25 秒の空振りを 5 回した。

### 行ごとの結果

| 行 | 結果 | 根拠 |
| --- | --- | --- |
| 29.1 | **合** | 既定のシェル、フック無し（`test-path $PROFILE` = **False**、`hook=[]`）。`cd plain` の後 `<A-Up>` → トーストが **`The shell has not said where it is (no OSC 7). PowerShell: set LocationChangedAction in $PROFILE — the line is in the README`**。タイトルは `…\s29` のまま。キーから戻るまで **623 ms**、その後 `<C-t>` `c` `f` が `plain` をコピーした（待ちでも無反応でもない） |
| 29.2 | **付けず** | pwsh の `$PROFILE` に貼れば通る（`osc7 file:///C:/dev` → タイトル **`Filer: C:\dev`**）。**既定のシェル（5.1）では、プロファイルが毎回 `PropertyAssignmentException` を出して終わる。**下の「見つけたもの 1」 |
| 29.3 | **合** | 空白: `osc7 …/s29/with space` → タイトル `…\s29\with space`。日本語: `osc7 …/s29/日本語フォルダ` → タイトル `…\s29\日本語フォルダ`。どちらも壊れていない |
| 29.4 | **合** | `\\localhost\C$\dev` へ `cd` → **落ちず、理由を述べる**。トースト 2 本: `\localhost\C$\dev: 指定されたパスが見つかりません。 (os error 3)` と `C$: 同`。ただし**追従はできない**し、名前が 1 文字ずれている。下の「見つけたもの 2」 |
| 29.5 | **合** | 実機の pwsh プロファイル（`starship init powershell` を読む）でペインを開き、README の 4 行を**手で 1 行にして打った**。プロンプトは前も後も Starship の 2 行（パス行 + `❯`）のまま。`cd plain` の後も同じ形で描かれ、`osc7 file:///C:/Users/…/s29/plain` が出て一覧が動いた |

`<A-Up>` が効いた 4 回とも、**ペインに `cd` が打ち返されていない**（`follow` の
「シェルが既に言っているなら打たない」が働いている。PTY ログの `in` 行に `cd ` で始まるものが 0 本）。

### 見つけたもの 1: README のフックは、ペインが既定で起こすシェルでは動かない

`$ExecutionContext.SessionState.InvokeCommand.LocationChangedAction` は
**Windows PowerShell 5.1 に存在しない**。`Get-Member -MemberType Property` の答えがそれを言う:

| シェル | `InvokeCommand` のプロパティ |
| --- | --- |
| `powershell` 5.1.28000.2952 | `CommandNotFoundAction` `HasErrors` `PostCommandLookupAction` `PreCommandLookupAction` |
| `pwsh` 7.6.6 | 上の 4 つ + **`LocationChangedAction`** |

だから README の 4 行を 5.1 の `$PROFILE` に貼ると、**シェルを開くたびに**こうなる（run `29-2-default` の写真）:

```
発生場所 C:\Users\yuu06\OneDrive\ドキュメント\WindowsPowerShell\Microsoft.PowerShell_profile.ps1:1 文字:1
+ $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction =  ...
    + CategoryInfo          : InvalidOperation: (:) []、RuntimeException
    + FullyQualifiedErrorId : PropertyAssignmentException
```

そのうえで `hook=[]`、`cd C:\dev` に OSC 7 は出ず、`<A-Up>` は 29.1 と**同じトースト**を出す。
**指示どおりにやった人に返ってくるのは「フックを入れろ」という同じ案内**で、出口が無い。

**README はこの 2 つを両方書いている**——「ペインは `powershell`、つまり 5.1」（1125-1131 行）と
「この 4 行を `$PROFILE` に入れる」（1111 行）——が、**その組み合わせは動かない。**
5.1 の `$PROFILE` の場所を表に載せていることが、動くという意味に読める。

### 見つけたもの 2: UNC パスはスラッシュが 1 つ多い URL になって、届いた先で 1 つ足りなくなる

README のフックは `$PWD.ProviderPath` の `\` を `/` にして `file:///` を前に付ける。
UNC では `ProviderPath` が `\\localhost\C$\dev` なので、出ていく URL は:

```
\e]7;file://///localhost/C$/dev\e\        ← スラッシュ 5 本
```

`from_file_url` は `file://` を外して最初の `/` から後ろを取るので `///localhost/C$/dev`。
Windows のパス解析は UNC の前置詞に**ちょうど 2 本**を要求するため、3 本は前置詞にならず
**`\localhost\C$\dev`**（先頭 1 本）に潰れる。トーストがその名前をそのまま出している。

- 行の期待（**追従するか、理由を述べる。落ちない**）は**満たされている**ので 29.4 は合。
- ただし**追従はできない**。直すなら (a) README のフックを UNC のとき `file://localhost/C$/dev` の形にする、
  (b) `from_file_url` で先頭の `/` が 3 本以上なら 2 本に畳む、のどちらか。**(b) のほうが、
  フックを書いたのが誰であっても効く。**

### 見つけたもの 3: `-replace` が効かなくても、filer は追従した

29.5 の 1 回目、こちらの手違いで `-replace '\\\\', '/'`（＝バックスラッシュ 2 つの正規表現）を打ってしまい、
シェルは `file:///C:\Users\…\s29\plain` と**バックスラッシュのまま**の URL を出した。
**それでも一覧は動いた**（タイトルが `…\s29\plain` になった）。`from_file_url` は最後に
`Path::new` へ渡すだけなので、Windows では `\` も区切りとして通る。

**README の `-replace` は、Windows では必須ではない。**必須のように読めるので、書き添えるか、
そのままにするなら「他の端末と同じ綴りにするため」と理由を書くほうがいい。

### 見つけたもの 4: `LocationChangedAction` は 1 つしか持てず、README の行は既にあるものを捨てる

この機械の pwsh は、プロファイルを読むだけで `LocationChangedAction` が**埋まっている**。
3 つの init を別々に試して、埋めているのが **mise** だと分かった:

| init | `LocationChangedAction` を設定するか |
| --- | --- |
| `zoxide init powershell` | no |
| `mise activate pwsh` | **yes** |
| `atuin init powershell --disable-up-arrow` | no |
| `starship init powershell` | no（null のまま） |

**mise は既にあるハンドラを呼んでから自分の仕事をする**（先にマーカーを入れてから
`mise activate` すると、`Set-Location` でマーカーが 1 回上がる）。
**README の 4 行は `=` で代入するので、既にあるものを捨てる**（前後のハンドラの参照が別物になる）。

つまり `$PROFILE` の**下のほうに貼ると mise の `cd` フックが黙って死ぬ**。
これは「どこに貼るか」で結果が変わる指示で、README は場所を指定していない。

### 見つけたもの 5: この run 自身がやった事故 —— `$PROFILE` はシンボリックリンクだった

29.2 のために README の 4 行を `$PROFILE` へ入れようとして、`Set-Content` で**上書き**した。
この機械の `$PROFILE`（`…\OneDrive\ドキュメント\PowerShell\Microsoft.PowerShell_profile.ps1`）は
**シンボリックリンク**で、実体は `C:\dev\obsidian-notes\notes\config\PowerShell\…` にある。
書き込みはリンクを素通りして実体に届き、**128 行が 4 行になった。**

**実体が git の管理下にあったので完全に戻せた**（`git restore` で 128 行 / 6486 バイト、`git status` は空）。
戻せたのは運で、設計ではない。その後の 29.2〜29.4 では、(a) 先に控えを取り、(b) `Add-Content` で
**追記**し、(c) 終わったら `git restore` して**追記前の控えとバイト単位で一致することを確かめた**。
いま `git -C C:\dev\obsidian-notes status` はこのファイルについて空で、5.1 側に作った
プロファイルと空のディレクトリも消してある。

### Proposals

#### 1. ペインの既定シェルは、`pwsh` があれば `pwsh` にすべき（#100 の提案 3 と同じ結論、別の理由）

- **何が起きたか**: 見つけたもの 1。**29 節の指示は、ペインの既定シェルでは成立しない。**
  #100 は「5.1 の PSReadLine が古い」という理由で同じことを言ったが、こちらは**機能が無い**。
  `LocationChangedAction` は 5.1 に存在せず、代わりになるのは `prompt` の差し替えだけで、
  それは README が「Starship を壊すから」と明示的に避けた方法。
- **どうするべきか**: `[term] shell` が空のとき `pwsh` を PATH から探し、あればそれを起こす。
  併せて README の「Which PowerShell」の表に、**5.1 ではこのフックは使えない**と 1 行足す。
- **なぜ**: いまは「README のとおりにしたのに動かない」が既定の体験で、**エラーは
  プロファイルを読むたびに出るのに、トーストは「フックを入れろ」と言い続ける。**
- **大きさ**: 既定の決定に `which` 1 回と分岐 1 つ。**既定の変更なので CHANGELOG の「変更」。**

#### 2. `from_file_url` は、先頭のスラッシュが 3 本以上なら 2 本に畳むべき

- **何が起きたか**: 見つけたもの 2。UNC で `file://///host/share` が来て `\host\share` になった。
- **どうするべきか**: `file://` を外したあと、先頭の `/` の連続が 3 本以上なら 2 本にしてから
  `Path::new` に渡す。`//host/share` は Windows で正しく UNC の前置詞になる。
- **なぜ**: **書き手の側では直しきれない。**OSC 7 を出すのは shell のフックで、
  「UNC のときだけ `file://host/share` にする」を全員に書かせるより、受け取る側が 1 行で吸収するほうが確実。
  31 節（ホストの共有一覧）を使う人はそのまま `<A-Up>` に来る。
- **大きさ**: `terminal.rs` の `from_file_url` に数行と、テスト 1 つ（Linux でも走る形にできる）。

#### 3. README のフックは、既にあるハンドラを呼ぶ形で配るべき

- **何が起きたか**: 見つけたもの 4。mise（や、同じところを使う他のツール）の `cd` フックが、
  貼る場所によって黙って消える。**消えたことは何も知らせない。**
- **どうするべきか**: 配る 4 行を、こう変える:

  ```powershell
  $prev = $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction
  $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = {
      param($s, $e)
      if ($prev) { & $prev $s $e }
      $p = $PWD.ProviderPath -replace '\\', '/'
      [Console]::Write("$([char]27)]7;file:///$p$([char]27)\")
  }
  ```

- **なぜ**: `LocationChangedAction` は**セッションに 1 つしかない**。mise は自分で連鎖しているので、
  「連鎖するのが作法」の場所。filer だけが上書きするのは行儀が悪いし、**壊れ方が静か**で、
  「mise が効かなくなった」と `<A-Up>` が結び付く人はいない。
- **大きさ**: README の 4 行を 7 行に。コードは 1 行も要らない。

#### 4. `<A-Up>` のトーストは、シェルが何を言ったかで文面を変えるべき

- **何が起きたか**: 29.1（シェルが何も言っていない）と、**フックはあるのに 5.1 で失敗している状態**で、
  出るトーストが**同じ**。29.4 のように「言ったが行けなかった」ときだけは別の文面（os error 3）になる。
- **どうするべきか**: 一度も OSC 7 を見ていないときは今の文面のまま。**ペインの中で
  `LocationChangedAction` の代入が失敗した**ことまでは filer には見えないので、
  代わりに「**このペインのシェルは `powershell` 5.1 です**」を文面に足す
  （filer は自分が何を起こしたか知っている）。`pwsh` を起こしていればその 1 行は出ない。
- **なぜ**: 5.1 の人は、いまの案内に従うかぎり**永久に同じところを回る**。
  シェルの名前が出れば、README の「Which PowerShell」の表にたどり着ける。
- **大きさ**: `term_pull_cwd` の文面に 1 分岐。

#### 5. 役割定義は、人の設定ファイルを書き換える前に実体を確かめさせるべき

- **何が起きたか**: 見つけたもの 5。**このセッションが人の pwsh プロファイル 128 行を消した。**
  `$PROFILE` はシンボリックリンクで、`Set-Content` はリンクを素通りする。git で戻せたのは偶然。
- **どうするべきか**: `.claude/windows-role.md` の「Unattended runs」に 1 項足す——
  **人のファイル（`$PROFILE`、`%APPDATA%` の設定、レジストリ）に触る行は、(a) 触る前に
  `(Get-Item -Force).Target` と `(Get-Item).Length` を控え、(b) 上書きではなく追記で行い、
  (c) run の最後に元に戻ったことをバイトで確かめ、(d) その 3 つを報告に書く。**
  29 節のように**人の `$PROFILE` を書き換えないと測れない節がある**ので、「触るな」では回らない。
- **なぜ**: 実機のセッションだけが起こせる壊し方で、**壊したことに気づかないまま終われる**。
  この run も、プロファイルが git の中に無ければ、気づいたときには戻せなかった。
- **大きさ**: 役割定義に 1 段落。

## TESTING.md section 39 — ペインの `<A-j>` / `<A-k>` を ARM64 で確かめた（d6ca3df / 0.54.10、ARM64 レーン）

ARM64 レーンの 11 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、無人実行）。
順番表の先頭「**39. `<A-j>` / `<A-k>` in the pane**」の `[ ]` 9 行を**全部**通した。

**8 行は期待どおりで `[x]` を付けた。39.9 だけ否**で、これは **filer 側の問題**——
ヘルプパネルは `[mgr]` レイヤーしか出さないので、ペインの中で `<F1>` を押しても
`[term]` の `<A-j>` / `<A-k>` は出てこない。しかも同じパネルに `[mgr]` のほうの
`<A-j>`（「Scroll the preview down」）が出るので、**ペインの中では答えが逆になる。**
`make-testcheck -- --check` は `in sync`（**169 / 401**）、`make-keycheck -- --check` も `in sync`（243 / 247）。
`cargo test` は **509 passed; 0 failed**（ネイティブ ARM64、debug 2.77 s、d6ca3df）。見つけたもの 3 件、提案 3 件。

### この機械

| | |
| --- | --- |
| 機械 | `PROCESSOR_ARCHITECTURE=ARM64`、Adreno X2-90（Vulkan）、画面 1920x1200、窓 1376x899（クライアント 1360x860） |
| OS | `Windows 11 Home 26H1 (build 28000.2956)` |
| filer | 手元ビルド 0.54.10、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| プログラム | nvim **0.12.5**、bash **5.3.15**（Git for Windows `usr\bin`）、`powershell.exe` 5.1（ペインの既定） |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ |
| 昇格 | **無し**（`IsInRole('Administrators')` = False。この節は昇格を要らなかった） |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |

生の証拠は `C:\dev\filer-evidence\arm-39\`（`runs\<tag>\pty.log` 8 本、`shots\` に窓の写真とペイン上端・
プレビュー上端の切り出し、足場の `lib.ps1` / `chord.ps1` / `wake.ps1` / `mouse.ps1` / `post.ps1` / `desk.ps1` /
`find.ps1` / `shot.ps1` と `fx39.ps1`、run の `r0`〜`r8`）。

### 測り方 — この節は「見た目」の行が 1 つも無い

- **スクロールバックがどこまで動いたか**は**数**で読む。ペインに `type long.txt`（`L0001`〜`L0500`、
  1 行が自分の番号を名乗る）を流し、`PrintWindow(PW_RENDERFULLCONTENT)` の写真から**ペインの左上 3 行**を
  2 倍に切り出して読む（`Save-PaneTop`、クライアント座標 y=560 から 76 px）。
  「5 行上」は **L0490 → L0485** という引き算で、目分量ではない。
- **キーがシェルに届いたか / 届かなかったか**は `FILER_PTY_LOG` の `in key` 行。
  **届かなかったことも読める**——その区間に行が 1 本も増えないことで。
  39.1〜39.5 と 39.7 では、Alt の押下が合計 **270 回以上**あって `in key` は **14 本のまま**だった
  （14 本は `type long.txt` の 13 文字と `\r` の分）。
- **プログラムが受け取ったか**は nvim 自身に書かせた（`cfg.vim` の
  `nnoremap <A-j> :call writefile(['altj ' . line('.')], 'mark.txt', 'a')<CR>`）。ディスクのファイルが答える。
- **readline がどう動いたか**は PTY ログの `out` 行。`echo ab cd` を打って Alt で動かし `X` を入れると、
  **実行された行の出力そのもの**（`ab Xcd` か `abX cd` か）がログに入る。
- **ヘルプパネルの中身**は写真を**テキストとして**読む。行の期待が「`<A-j>` / `<A-k>` が説明付きで出る」なので、
  出ている文字列そのものが期待値。
- **キーの入れ方**: 修飾キーの付くもの（`<A-j>`、`<S-PageUp>`、`<C-t>`、`<C-F5>`、`Alt-b`）は **SendInput**、
  付かないもの（`<F1>`、`G`）は **PostMessage**。`--keys` は起動直後にしか押せないので、ペインを開くところ
  （`<C-t>`）までにしか使っていない。SendInput の前後で毎回 `GetForegroundWindow` が filer であることを、
  各 run の前後で `OpenInputDesktop` が `Default` を返すことを確かめている
  （この機械のスクリーンセーバは #88 以来、放っておくと入力デスクトップを握る。`wake.ps1` で終わらせてから測った）。

### 行ごとの結果

| 行 | 結果 | 根拠 |
| --- | --- | --- |
| 39.1 | **合** | ペイン上端が `<A-k>` 1 回につき **L0490 → L0485 → L0480** と 5 行刻みで上へ。`in key` は 14 本のまま |
| 39.2 | **合** | 続けて `<A-j>` 2 回で **L0480 → L0485 → L0490**。プレビューの `seek 5` と同じ向き・同じ量 |
| 39.3 | **合** | `<A-k>` 120 回で最上部（`Windows PowerShell` のバナー）、**さらに 5 回押しても同じ**。戻りの **1 回目で L0002 へ動く**（空押し無し）、2 回目で L0007。下端も同じ——`<A-j>` 130 回で L0490、5 回足しても L0490、戻りの 1 回目で L0485 |
| 39.4 | **合** | `<S-PageUp>` で **490 → 484 → 478**（半画面 = 6 行、ペインは 12 行）、`<S-PageDown>` で 484、`<S-Home>` でバナー、`<S-End>` で 490。ホイールは上 3 ノッチで **490 → 485**、下 3 ノッチで **489**（端数の持ち越しは #100 の見つけたもの 1 と同じ。下の「見つけたもの 2」） |
| 39.5 | **合** | `<C-t>` で一覧へ戻ったあと、**プレビューが L0001 → L0006 → L0011**（`<A-j>` 2 回）、`<A-k>` で L0006。**同じ写真のペイン上端は L0475 のまま動かない。**`in key` も 14 本のまま |
| 39.6 | **合** | `nvim -u cfg.vim long.txt` の中で `in key` に **`\ej` `\ej` `\ek`**、`mark.txt` に **`altj 1` / `altj 1` / `altk 1`**。`\e[?1049h` / `\e[?1049l` も上下 1 回ずつ記録 |
| 39.7 | **合** | 再読み込み**前**の `<A-j>` は L0475 → L0480 と動く。`[[term.prepend_keymap]] on="<A-j>" run="noop"` を書いて `<C-F5>`（一覧から）を押したあと、`<A-j>` を 2 回押して **L0480 のまま**、`in key` も **14 本のまま**（シェルにも届いていない）。対照の `<A-k>` は同じ状態で L0475 へ動く |
| 39.8 | **合** | bash 5.3 で `echo ab cd` → `Alt-b` → `X` → 出力 **`ab Xcd`**。`Alt-b Alt-b Alt-f` → `X` → **`abX cd`**。`Alt-b Alt-d` → **`ab`**。filer が送ったのは `\eb` `\eb` `\eb` `\ef` `\eb` `\ed`。**同じ bash プロンプトで `<A-k>` 2 回は `in key` を 1 本も増やさず**、ペイン上端が **L490 → L480**（取られたのは j と k だけ） |
| 39.9 | **否** | ペインから `<F1>` で開いたパネルには **`[term]` の節が無い**。`G` で最下段まで送っても最後の行は `<C-S-Enter> … term_max`＝`[[mgr.keymap]]` の最後（`keymap.toml:727`）。パネルに出る `<A-j>` は `[mgr]` のほうで、説明は **「Scroll the preview down」/ `seek 5`** |

### 見つけたもの 1: ヘルプパネルは `[mgr]` しか出さない（39.9）

`ui/overlay.rs:502` が `for b in &app.cfg.keymap.mgr` で、**パネルは開かれたレイヤーを見ていない。**
だから `[term]` はもちろん、`[help]` `[spot]` `[pick]` `[input]` `[confirm]` `[tasks]` `[cmp]` `[diff]` の
どれも出ない。ペインから開いた写真（`shots\i0-help.png` / `i1-help-page2.png` / `i3-help-bottom.png`）を
上から下まで読んで確かめた。

**ただ出ないだけではない。**同じパネルに `[mgr]` の `<A-j>` が

```
<A-k>    Scroll the preview up      seek -5
<A-j>    Scroll the preview down    seek  5
```

と出る。**ペインの中でこれを読んだ人は、`<A-j>` がプレビューを動かすと思う。**実際には
ターミナルのスクロールバックが動く（39.1 / 39.2 がそれ）。`[term]` の同じ 2 キーには
`Scroll the terminal up` / `down` という説明が既に書いてあり、**出す先が無いだけ**。

行 39.9 は**プログラムのほうが間違っている**と読んだ（行の期待は自然で、`[term]` の
`desc` も書いてある）。行は直していない。

### 見つけたもの 2: ホイールの端数は 39 節でも同じだけずれる

`<S-PageUp>` / `<S-PageDown>` は 6 行ずつで往復がぴったり戻るのに、**ホイールは 3 ノッチ上げて
3 ノッチ下げると 1 行ずれる**（L0490 → L0485 → L0489）。#100 が 40.6 / 40.8 で見つけた
`wheel_whole` の端数の持ち越しと同じ形で、**別の節・別の run で独立に再現した**ことになる。
39.4 の期待は「以前のまま」なので**行としては合**（v0.37.0 の変更が壊したものではない）だが、
#100 の提案 2（向きが変わったら端数を捨てる）は**まだ有効**で、押す価値が上がったと思う。

### 見つけたもの 3: `<C-F5>` はペインの中では設定を読み直さない（39.7 の手順）

39.7 は「`<C-F5>`」と書いているが、`config_reload` は **`[mgr]` にしかない**（`keymap.toml:124`）。
ペインの中で押すと `[term]` が拾わないので、**エスケープシーケンスとしてシェルへ流れる**。測った:

```
   3491 in key  \e[15;5~     ← ペインの中で <C-F5>。設定は読み直されない
```

（`runs\cf5\pty.log`。39.7 の測定そのものは `<C-t>` で一覧へ戻ってから押している。）
**行の文言は間違っていない**——ペインの外で押せばそのとおりに動く——が、
「ペインの中の話をしている節の中で、ペインの外へ出ないと押せないキー」なので、
読んだ人はまずここで 1 回つまずく。下の提案 2。

### Proposals

#### 1. ヘルプパネルは、いま効いているレイヤーの鍵を出すべき

- **何が起きたか**: 39.9。ペインの中で `<F1>` を押すと `[mgr]` の一覧が出る。
  `[term]` の 11 個の割り当て（`<A-j>` `<A-k>` `<S-PageUp>` `<S-PageDown>` `<S-Home>` `<S-End>`
  `<A-Up>` `<C-t>` `<C-S-t>` `<C-S-Enter>` `<C-S-p>` と `<F1>` 自身）は**アプリのどこからも一覧できない。**
  そのうえ `[mgr]` の `<A-j>` が「Scroll the preview down」と出るので、**その場では嘘になる。**
- **どうするべきか**: `overlay::help` が `app.cfg.keymap.mgr` 固定なのをやめ、
  **いまのレイヤーの節を先頭に出す**（`term` / `help` / `spot` / `pick` / …）。
  最小の形は「`keys`」の見出しを「`keys — term`」にして、`term_focus` のときだけ
  `keymap.term` を先に並べ、そのあとに `keys — mgr` を続けること。
  レイヤーの区別がパネルに 1 行入るだけで、嘘ではなくなる。
- **なぜ**: `[term]` レイヤーは**覚えるものが一番多い層**（Shift の付く 4 つと Alt の 2 つは
  どれも「この層だけ」の割り当て）なのに、**思い出す手段が README しかない。**
  `<F1>` が `[term]` にわざわざ割り当ててあるのは「ここでも鍵を見たい」からで、
  いまはその `<F1>` が違う層の鍵を見せている。
- **大きさ**: `ui/overlay.rs:502` の 1 ループを、レイヤーを選ぶ関数に差し替える。
  見出し行が増えても `help_lines` の数え方は変わらない。**関数 1 つぶん。**

#### 2. `config_reload` は `[term]` にも割り当てるべき

- **何が起きたか**: 見つけたもの 3。ペインを使っている最中に設定を直しても、
  **一度ペインから出ないと読み直せない。**出ると `[term]` の `term_max` が解除される
  （`feed_term_key` の「Leaving the pane un-maximises it」）ので、
  最大化して使っている人は**窓の大きさまで戻される。**
- **どうするべきか**: `[[term.keymap]]` に `<C-F5>` → `config_reload` を足す。
  `<F1>` と `<C-S-p>` が既に「シェルが使わないから app が取っておく」枠で、
  **`<C-F5>` も同じ枠**（`\e[15;5~` を読むシェルはまずいない）。
- **なぜ**: 設定をいじりながらペインを使う場面は、`[term]` の keymap をいじっているときそのもの。
  **いま一番読み直したい人が、一番読み直せない。**39.7 を測るのに一覧へ出る必要があったのも同じ理由。
- **大きさ**: 既定の keymap に 4 行。**TESTING-KEYS.md が 1 行増える**ので、
  `make-keycheck` を回すのは人の側。

#### 3. `--keys` に「待つ」を入れるべき

- **何が起きたか**: この節の 9 行のうち、`--keys` だけで測れたものは **1 つも無い。**
  `--keys` はウィンドウが開いた直後に 2 フレーム間隔で押し切ってしまうので、
  「ペインを開く → **シェルが立ち上がるのを待つ** → コマンドを打つ → **出力が終わるのを待つ** →
  `<A-k>`」という、この節の全部の行が持っている形が書けない。
  結果 `SendInput` の足場（`chord.ps1` + `wake.ps1` + 入力デスクトップの確認）が要り、
  **スクリーンセーバを終わらせないと 1 行も測れない**（#88 以来この機械の毎回の障害）。
- **どうするべきか**: キー列に**待ち**を書けるようにする。案は 2 つあって、
  **(a)** `<Wait:1500>` のような擬似キー（`keyscript::parse` に 1 つ足す）、
  **(b)** `--keys` を複数回渡せるようにして、1 つ目が settled してから次を押す。
  **(a) を推す**——いまの `script: VecDeque<Vec<Event>>` に「空の events と期限」を 1 つ積むだけで、
  `raw_input_hook` の `waited_long` の隣に条件が 1 つ増えるだけで済む。
- **なぜ**: `--keys` は役割定義が「Start with `--keys`」と言う、**実機の測定の第一手段**。
  いま届かないのは「時間の関わる行」だけで、それは**ターミナルペインの行のほぼ全部**
  （1 / 8 / 29 / 38 / 39 / 40 節）。ここが届けば、この機械の測定から
  SendInput とスクリーンセーバの話が丸ごと消える。
- **大きさ**: `keyscript.rs` に擬似キー 1 つと、`main.rs` の `raw_input_hook` に条件 1 つ。
  **README の `--keys` の説明に 1 行。**

### この run が触ったもの

- `TESTING-CHECKS.md`: 39.1〜39.8 に `[x]`、`cargo run --example make-testcheck` で見出しと
  合計を書き直した（**169 / 401**、`-- --check` は `in sync`）。
- ほかに変えたファイルは無い。`Cargo.toml` と `CHANGELOG.md` は役割定義のとおり触っていない。
  人の設定ファイル（`$PROFILE` など）には 1 つも触っていない——この節の設定はすべて
  run ごとの `FILER_CONFIG_HOME` の下に書いた。
- 起動した `filer.exe` は 8 本、いずれも run の最後に `Stop-Filer` で終わらせた。
  終了時の `Get-Process filer` は 0 本。

---

## TESTING.md section 47 — 放置した窓の CPU を ARM64 で測った（1dff909 / 0.54.12、ARM64 レーン）

ARM64 レーンの 12 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、
無人実行）。順番表の先頭「**47. an idle window uses no CPU**」の 4 行を見た。

**47.1 / 47.2 / 47.3 は期待どおりで `[x]` を付けた。47.4 は条件が成り立たなかったので `[ ]` のまま。**
v0.54.2 の直し（`request_preview` 冒頭の `pending_since.take()`）は ARM64 でも効いていて、
#86 が測った 1.0 CPU-s/s は**この機械のどの測り方でも再現しない**——10 秒の窓で差が
**0.0000 CPU-s**（`Get-Process` の粒度 15.6 ms より小さい）が既定で、最大でも 0.0156 CPU-s。
`make-testcheck -- --check` は `in sync`（**172 / 401**）、`make-keycheck -- --check` も
`in sync`（243 / 247）。`cargo test` は **509 passed; 0 failed**（ネイティブ ARM64、debug 2.68 s、1dff909）。
見つけたもの 5 件、提案 4 件。

### この機械

| | |
| --- | --- |
| 機械 | Snapdragon X2 Elite (X2E88100, Oryon)、Adreno X2-90（Vulkan, IntegratedGpu）、窓 1360x860 |
| OS | `Windows 11 Home 26H1 (build 28000.2956)`、`PROCESSOR_ARCHITECTURE=ARM64` |
| filer | 手元ビルド 0.54.12 release、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ |
| 昇格 | **無し**（`IsInRole('Administrators')` = False。この節は昇格を要らなかった） |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |
| デバウンス | 既定の 40 ms（人の `yazi.toml` は `ratio` / `preview` / `opener` だけで、ここに触っていない） |

生の証拠は `C:\dev\filer-evidence\arm-47\`（`readings.md` に**この run の CPU の読みが全部**、
足場の `s47.ps1` / `shot2.ps1`、`env47.txt`、変種 A 用の `cfg47-yazi.toml`、窓の写真 1 枚）。

### 測り方 — 「0 だった」を測定にするために

この節の困りごとは、**期待値がゼロ**だということ。何も動いていない窓と、測れていない窓は、
同じ 0 を返す。だから先に**陽性対照**を取った。同じプロセス・同じ `Measure-Cpu` で、
100 ms おきに `j` / `k` を 100 回投げながら 11.32 秒:

```
control delta = 1.1875 CPU-s / 11.32 s  = 0.105 CPU-s/s
```

を返し、手を離して 3 秒後には同じ窓が `0` に戻った。**この方法は #86 の 1.0 CPU-s/s の
10 分の 1 の活動でも見える**ので、0 は盲点ではなく測定値として書いてよい。

窓が本当に画面に出ていることも確かめた。1 本目は ASUS OLED Care のスクリーンセーバが
入力デスクトップを握ったまま（`OpenInputDesktop` が `Screen-saver`）測っていて、
これは「隠れているから描いていないだけ」の疑いが残る。セーバを落として
`Desktop = Default` / `ScreenSaver = False` にしてから測り直し、同じ 0 を得た。

キーは `PostMessageW`（`WM_KEYDOWN` / `WM_CHAR` / `WM_KEYUP`、`lParam = 1 | (MapVirtualKey(vk,0) << 16)`）。
**1 回の投函が 1 回の押下になることを先に確かめた**——4 つのディレクトリだけの木で
`filer <dir> --keys "j<Enter>"` と「`j` を 1 回投函して `<Enter>`」が、どちらも
窓のタイトルを `...\rows47\r2` にした。

カーソルがどこに居たかは**窓のタイトル**で読んだ。47.2 の対の直後に `<Enter>` を送り、
タイトルがディレクトリの名前になれば、`j` と `k` の**両方が届いたこと**が分かる
（`j` だけなら `a.txt` の上、`k` だけなら `dir1` の上にいるので、どちらもタイトルが変わらない）。

### 行ごと

| # | 結果 | 根拠 |
| --- | --- | --- |
| 47.1 | **[x]** | `t47a`（サブフォルダ 2・ファイル 3）で開いて 10 秒放置、10 秒の窓を 2 つ: **0 / 0**。セーバを落とした状態で開き直して **0 / 0**。`make-fixtures.ps1` の `fx`（15 項目）で **0.0156 / 0**。どれも「1 秒よりずっと小さい」を 2 桁下回る |
| 47.2 | **[x]** | 対の間隔を `Stopwatch` で測って **25.39 ms**（既定設定、`j` で `a.txt` に乗り `k` で `dir2` へ）。前 **0 / 0**、後 **0 / 0 / 0**。`<Enter>` でタイトルが `...\t47a\dir2` になり両方のキーが届いたことを確認。**書いてあるとおりの `j` → `j`** も別に踏んだ（見つけたもの 1）: `sort_dir_first = false` で `a.txt`→`b.txt`→`czdir`、間隔 **21.84 ms**、前 **0**、後 **0 / 0 / 0**、`<Enter>` で `...\t47b\czdir` |
| 47.3 | **[x]** | 47.2 の対（間隔 **24.78 ms**）のあと `ShowWindow(SW_MINIMIZE)`。`IsIconic` は測る前も後も True。10 秒の窓が **0.0156 / 0 / 0**。復元して `<Enter>` → `...\t47a\dir2` |
| 47.4 | `[ ]` のまま | **条件が成り立たなかった**（47.1〜47.3 のどれも増えていない）。読み自体は記録した: `fx` のプロセスで上位 3 スレッドの `TotalProcessorTime` を 10 秒あけて 2 回読み、`25328 0.4687500` / `21968 0.0312500` / `15872 0.0156250` が**両方の読みで 1 ティックも動かなかった**。行の文言が「まだ増えるとき」なので、チェックを付けるのは嘘になると判断した |

ついでに測ったもの（節の行ではない）: カーソルを `fx` の途中の行に置いて 10 秒 **0**、
`zoom-me.png`（3200x2400、fit 23% で表示）に置いて 10 秒 **0**。プレビューが出ていても増えない。

### 見つけたもの

#### 1. 47.2 の手順は、既定の設定では**踏めない**（TESTING.md 側）

`sort_dir_first` の既定は `true`（`src/config/yazi.rs:51`、`src/fs/sort.rs:66`）なので、
**ディレクトリは必ずファイルより上に並ぶ。**「`j` でファイルに乗り、すぐ `j` でサブフォルダに乗る」は
既定のままでは**どのフォルダでも起こらない**——`j` がファイルからサブフォルダへ動くことがない。

この run は両方を踏んだ。既定の設定では `j`（ファイルへ）→ `k`（ディレクトリへ戻る）で、
書いてある `j` → `j` は `FILER_CONFIG_HOME` に `sort_dir_first = false` だけを書いた
設定で踏んだ。**どちらも 0 だった**ので合否は動かないが、行の文言は直したほうがいい。
番号を動かさない直し方の案:

> `j` でファイルに乗り、すぐ（40 ms のデバウンスの内に）**ディレクトリの行へ移って**手を離す
> （既定では並びがディレクトリ先なので `k`。`sort_dir_first = false` なら `j`）。

直すのはこの役割の仕事ではないので提案にとどめる。

#### 2. 順番表の「`filer <dir> --keys "jj"` is 47.2 with no harness」は成り立たない

`--keys` は `raw_input_hook` で 1 つずつ押し、`App::settled()` が true になるまで次を押さない
（`src/main.rs:494`）。そして `settled()` は **`self.preview.pending_since.is_some()` の間 false**
（`src/app.rs:1749`）。47.2 が割り込みたいのは**まさにその `pending_since` が立っている 40 ms**
なので、`--keys` は構造的に**デバウンスが切れるまで待ってから**次のキーを押す。
`--keys "jj"` で測れるのは「40 ms を過ぎてからの 2 打」で、それは 47.2 ではない。

だから対は `PostMessageW` + `Stopwatch` で投げた。役割定義の順番表（x64 側の 47 の行と
ARM64 側の 47 の行、両方）にこの一文があるので、**次の run が同じ勘違いをする。**

#### 3. プロンプトを開いたままの窓は 0 ではない（0.14〜0.27 CPU-s / 10 秒）

`f`（`filter --smart`）でプロンプトを開いただけで、あとは何も触らずに測ると
**0.1406 / 0.2656 / 0.2188**（10 秒の窓 3 つ）。閉じると **0.0312 / 0** に戻る。
プロンプトが本当に開いていたことは、そのあと `d` `i` `r` `1` `<Enter>` で一覧が `dir1` だけになり、
もう一度の `<Enter>` で中へ入った（タイトル `...\t47a\dir1`）ことで確かめた。

#86 の 1.0 CPU-s/s とは 2 桁違うので**節の合否は動かない**。ただし 47 の本文が言う
「放置した窓」は、実際には**オーバーレイを開いていない窓**に限る。この run はこれに 2 回引っかかって
（カーソルの居場所を読むつもりの `c` `f` が `cc` + `f` になり、フィルタのプロンプトを開いたまま
測っていた）、0.03〜0.05 CPU-s/s の「増えている」を 2 回作った。**47 に 1 行足す価値がある**と思う。

ついでに: コピーのトーストが出ている間も 0.1875 CPU-s / 10 秒ほど使う。これは数秒で収まる。

#### 4. 最小化した窓は、投函したキーをその場では処理しない

47.3 の最中に `<Enter>` を投函しても**何も起きなかった**（タイトルは変わらず、
`Win32_Process` に子プロセスも無し）。`ShowWindow(SW_RESTORE)` のあとに同じ `<Enter>` を送ると
`...\t47a\dir2` へ入った。**最小化中の確認は後回しにして、復元してから読むこと。**
47.3 の測定そのものには影響しない（測るのは CPU で、キーは要らない）が、
「最小化したまま何かを確かめる」行が来たら、この順番でないと測れない。

#### 5. この機械のスクリーンセーバの止め方（#88 / #100 の続き）

`Stop-Process -Name 'OLED Care Screensaver'` は**当たらない**——この機械のプロセス名は
`.scr` 込みの `OLED Care Screensaver.scr` で、`-Name` の一致に失敗しても
`-ErrorAction SilentlyContinue` で黙って通る。**Id で止めるしかない**:

```powershell
Get-Process | Where-Object { $_.ProcessName -match 'OLED Care' } | Stop-Process -Force
```

止めた直後に `OpenInputDesktop` が `Default` を返す。`OpenDesktopW('Default') + SwitchDesktop`
は試したが **False**（権限が足りない）ので、これは使えない。

### Proposals

#### 1. プロンプトを開いている間、窓は描き続けるべきではない

- **何が起きたか**: 見つけたもの 3。`f` を押してプロンプトを出しただけの窓が、
  10 秒で 0.14〜0.27 CPU-s 使う。閉じると 0。キャレットの点滅のために
  `request_repaint` が回り続けている。
- **どうするべきか**: プロンプトが開いている間の再描画を、**点滅の周期でだけ**起こす
  （`request_repaint_after(blink)`）。あるいは**窓がフォアグラウンドでないときは点滅を止める**——
  裏に回っている窓のキャレットが光る必要は無い。
- **なぜ**: v0.54.2 が消したのは「増え続ける」ほうで、これは残った小さいほうだが、
  **同じ理由で同じ場所を直せる。**ノート PC のバッテリの話でもあるし、
  「放置した窓は 0」という節の約束が、プロンプト 1 つで崩れるのは分かりにくい。
- **大きさ**: 描画側の 1 か所。**関数 1 つぶん**だと思う。

#### 2. `--keys` に「待たずに押す」を足すべき（前回の `<Wait:N>` と対になるもの）

- **何が起きたか**: 見つけたもの 2。`--keys` は `settled()` を待つので、
  **デバウンスの内側に 2 打目を入れられない。**47.2 は `PostMessageW` と `Stopwatch` の
  足場が要り、そこから「投函 1 回 = 押下 1 回か」の確認から始めることになった。
- **どうするべきか**: 前の run（section 39）が提案した `<Wait:1500>` と対に、
  **待ちを飛ばす印**を入れる。`<Now:j>` でも `!j` でもいいが、`raw_input_hook` の
  `!(self.app.settled() || waited_long)` の条件を**そのキーに限って外す**だけで済む。
  `<Wait:N>` と合わせると「開く → 待つ → **間を空けずに 2 打** → 測る」が
  1 本の文字列で書けるようになる。
- **なぜ**: 役割定義は「Start with `--keys`」と言っていて、**実機の測定の第一手段**。
  デバウンス・アニメーション・多重描画のように「**間に合わせないと再現しない**」種類の行は
  これからも出るし、その全部がいまは Win32 の足場からになる。
- **大きさ**: `keyscript.rs` に印 1 つと、`main.rs` の条件 1 つ。前回の提案と**同じ 2 か所**。

#### 3. カーソルの居場所を、副作用なしに読める口が要る

- **何が起きたか**: この run は「カーソルはどの行か」を読むのに `c` `f` を使い、
  **2 回とも壊した**——`c` が `cc`（絶対パスのコピー）になったうえに、続く `f` が
  `filter --smart` のプロンプトを開き、そのまま CPU を測って「増えている」を作った。
  最後はタイトルで読む形（`<Enter>` を送って行き先を見る）に変えたが、
  これは**ディレクトリの行にしか使えない。**
- **どうするべきか**: `--keys` の相棒として `--dump <file>` を足す。
  スクリプトを押し切ったあと、**カーソルのパス・タブの数・開いているオーバーレイの名前**を
  1 行ずつ書いて終わる。`--cwd-file` と同じ仕組みの上に乗る。
- **なぜ**: いまは「状態を読む」ためにキーを押すしかなく、**読む行為が状態を変える。**
  実機レーンの run はほぼ毎回これをやっていて、この run では**測定を 2 回汚した。**
  ファイル 1 本で読めれば、`Get-Clipboard` も窓のタイトルも要らなくなる。
- **大きさ**: `main.rs` の CLI に 1 つ、`app.rs` に短い書き出しが 1 つ。**関数 1 つぶん。**

#### 4. `auto-wintest.ps1` は run ごとに scratch を分けるべき

- **何が起きたか**: `C:\Users\yuu06\AppData\Local\Temp\filer-scratch` は
  **レーンの全 run で共有**されていて、いまは 10 本ぶんの置き土産（`report.md`、
  `pr-body.md`、`keys41.ps1`、`env.vbs` …）が同じ平らな場所にある。この run の最中、
  **別の run が残した `wscript` のエラーダイアログ**（`runenv.vbs` を実行しようとして
  コンパイルエラー、その `.vbs` はもう存在しない）が画面の真ん中に出たままだった。
  写真（`state-after-cf.png`）に写っている。
- **どうするべきか**: スクリプトが `...\filer-scratch\<lane>-<section>-<日時>\` を作り、
  `TEMP` / `TMP` をそこに向ける。終わったら**数本ぶんだけ残して古いものを消す。**
- **なぜ**: x64 側は RAM ディスクが電源で消えるので、この問題が起きない。
  **ARM64 だけが溜め続ける。**自分の run のファイルと前の run のファイルが区別できないと、
  証拠として引用するときに間違える。画面に出たままのダイアログは、
  `SendInput` を使う節（30 など）では**入力を奪う**ので実害もある。
- **大きさ**: `auto-wintest.ps1` の scratch を決めるところ。**10 行ぐらい。**

### この run が触ったもの

- `TESTING-CHECKS.md`: 47.1〜47.3 に `[x]`、`cargo run --example make-testcheck` で見出しと
  合計を書き直した（**172 / 401**、`-- --check` は `in sync`）。
- ほかに変えたファイルは無い。`Cargo.toml` と `CHANGELOG.md` は役割定義のとおり触っていない。
  人の設定ファイル（`$PROFILE` など）には 1 つも触っていない——変種 A の設定は
  run 専用の `FILER_CONFIG_HOME`（scratch の下）に書いた。
- 止めたもの: `OLED Care Screensaver.scr`（この機械のスクリーンセーバ。idle で戻る）。
- 起動した `filer.exe` は 9 本、いずれも run の最後に終わらせた。終了時の `Get-Process filer` は 0 本。

## TESTING.md section 30 — プロンプトでの右クリック貼り付け（18e01cb / 0.54.13、ARM64）

Windows のセッション（`.claude/windows-role.md`、**ARM64 レーン、無人実行**）から。順番表の
ARM64 の表の先頭にあった section 30 を 1 つだけ進めた。

| | |
| --- | --- |
| 機械 | `PROCESSOR_ARCHITECTURE=ARM64`、Adreno X2-90（Vulkan）、窓はクライアント 1360x860 @ 1 |
| OS | `Windows 11 Home 26H1 (build 28000.2956)` |
| filer | 手元ビルド 0.54.13、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| プログラム | Windows PowerShell 5.1（PSReadLine **2.0.0**）、pwsh **7.6.6.0 arm64**（PSReadLine **2.4.5**）、cmd.exe、nvim |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ |
| 昇格 | **無し**（`IsInRole('Administrators')` = False。この節は昇格を要らなかった） |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |
| `cargo test` | **509 / 0**（ARM64 でネイティブ実行、0.54.13） |

生の証拠は `C:\dev\filer-evidence\arm-30\`（`runs\<tag>\pty.log` 32 本、`shots\` に 55 枚、
`logs\` に各 run の出力、足場の `lib.ps1` / `wake.ps1` / `chord.ps1` / `post.ps1` / `mouse.ps1` /
`desk.ps1` / `find.ps1` / `shot.ps1` と `fx30.ps1`、run は `r0`〜`r9`）。

**14 行のうち 10 行に `[x]` を付けた**（30.2、30.5〜30.10、30.12〜30.14）。残り 4 行は付けていない:
**30.3 と 30.4 は不具合**、**30.1 と 30.11 は行の前提が Windows では成り立たない**。

### 測り方 — この節も「見た目」の行が 1 つも無い

- **右クリックは SendInput でしか入らない。**#100 が測ったとおり、`PostMessageW` で
  `WM_RBUTTONDOWN` / `WM_RBUTTONUP` を送っても egui には 1 つも届かない（戻り値は `True`）。
  だから入力デスクトップが要る。各 run の 1 行目が `desktop: Default / SaverRunning False /
  LogonUI 0 / Ours True` で、`Click-At` は毎回そのあと `GetForegroundWindow` が filer であることを
  確かめている（全 run で `foreground ours=True`）。
- **プロンプトの中身を写真で読まない。**`<Enter>` で確定すると、フィールドにあった文字列が
  そのまま**ディスク上のファイル名**になる。だから貼り付けの行はほぼ全部 `r`（rename）を通し、
  読みは `Get-ChildItem` にした。行が別のプロンプトを名指すところ（`cd`、`f`、パレット、
  一括リネーム）はそのプロンプトを使い、読みはそれぞれ**窓のタイトル**・**`c` `f` のクリップボード**・
  **ディスク上の名前**にした。
- **どの文字の間をクリックしたかは、機能そのもので較正した**（`r1-cal.ps1`）。`-` を
  クリップボードに置いて 3 か所を右クリックし、できた名前の `-` の位置から添字を読む:

  | client x | できた名前 | 添字 |
  | --- | --- | --- |
  | 86 | `A-BCDEFGHIJKLMNOP` | 1 |
  | 128 | `ABCDEF-GHIJKLMNOP` | 6 |
  | 178 | `ABCDEFGHIJKL-MNOP` | 12 |

  この 3 点から、フィールドの左端は client x = **77.6**、1 文字は **8.364 px**。
  ピクセルを目で数えるより確かで、しかも**これ自体が 30.2 の中身**になっている。
- **ターミナルの行は `FILER_PTY_LOG` とディスク。**`in paste` の行が filer の送ったバイトで、
  `\e[200~` が付いたかどうかはそこで読める。「ペインがキーを取ったか」は、貼り付けた
  コマンドが**ファイルを作るもの**なので、そのファイルの有無で読んだ。

### 行ごとの結果

| 行 | 結果 | 根拠 |
| --- | --- | --- |
| 30.1 | **否（書いてあるとおりでは）** | Explorer のアドレスバーで Alt+D / Ctrl+C したクリップボードは正しい（`C:\…\s30\filt`）。だが `cd` のプロンプトは**現在地で埋まって開く**ので、そのまま右クリックすると現在地の途中にパスが差し込まれ、`<Enter>` でどこへも行かない（タイトルは `…\s30\term` のまま）。`<C-a><Backspace>` で消してから同じことをすると **`…\s30\filt` へ移動する** |
| 30.2 | **合** | `abc` に `r`、添字 1 で右クリック（クリップボードは `X`）→ ディスクに **`aXbc`**。末尾ではない |
| 30.3 | **否** | `abcdef` の添字 2..4（`cd`）をドラッグで選択（`shots\30-3-selected.png` に反転が写っている）、その**選択の上**（添字 3）で右クリック → **`abcXdef`**。選択は置き換わらず、クリックした位置に挿入された |
| 30.4 | **否（後半）** | 「1 行になる」は合。だが `line one\r\nline two` を右クリックで貼ると **`twoline one␣␣line two`**（空白 **2 つ**、文字コード 32,32）、同じクリップボードを `<C-v>` で貼ると **`ctrlvline one␣line two`**（**1 つ**）。行は「`<C-v>` と同じ」と言っている |
| 30.5 | **合** | クリップボードは `…\jp\日本語テストA`（末尾 1 文字を欠いたもの）。`cd` を空にして右クリック → 貼られたあとに `B` を打ち、`<Enter>`。タイトルが **`Filer: …\s30\jp\日本語テストAB`** になった。壊れず入り、カーソルが後ろに来ていなければこのディレクトリは見つからない |
| 30.6 | **合** | STA のプロセスで `Clipboard.SetImage`（`ContainsImage=True` / `ContainsText=False` / `formats=System.Drawing.Bitmap,Bitmap`）。`r` の上で右クリック → **1.5 秒後の写真でプロンプトは `Rename: keepme` のまま、トーストは無い**（`shots\30-6-after.png`）。`<Enter>` 後のディスクも `keepme` |
| 30.7 | **合** | 3 つとも貼れた。パレット（`<C-S-p>`）に `temporary` を貼って `<Enter>` → タイトルが TEMP へ。`f` に `zzz` を貼って `<Enter>` → 3 ファイルのうち `c` `f` が **`zzz-only.txt`**。`R`（一括リネーム）に `pre-{name}{ext}` を貼る → **プレビューが `one.txt → pre-one.txt` / `two.txt → pre-two.txt` に描き直され**（`shots\r6-bulk-pasted.png`）、`<Enter>` でディスクが **`pre-one.txt \| pre-two.txt`** |
| 30.8 | **合** | 3 ファイルの 3 行目を右クリック → `Actions: zzz-only.txt` のパネル（`shots\r6-list-menu.png`）。`<Esc>` のあと `c` `f` は **`zzz-only.txt`**、タイトルは変わらず、ディレクトリも `aaa-other.txt \| bbb-third.txt \| zzz-only.txt` のまま。クリップボードに置いた `SHOULD-NOT-APPEAR` はどこにも出ない |
| 30.9 | **合** | `<C-t>` で開いたあと `<C-t>` でキーを一覧へ戻し（`c` `f` が `note.txt` を返す）、ペインを右クリック → `in paste  Set-Content p9.txt nine`。**その時点では `p9.txt` は無い。**続けて `<Enter>` を**投函**すると `in key \r` が出て `p9.txt` ができ、中身は `nine`。貼り付けとキーの受け取りが両方読める |
| 30.10 | **合** | `echo SELECTME1` の出力行をドラッグ → 離した時点でクリップボードが **`SELECTME1`**（センチネルから変わった）。そのまま右クリック → `in paste  SELECTME1` |
| 30.11 | **否（行の前提が Windows で成り立たない）** | **この機械のどの PowerShell も `\e[?2004h` を 1 度も送らない。**Windows PowerShell 5.1（PSReadLine 2.0.0）でも pwsh 7.6.6（PSReadLine **2.4.5**）でも 0 回。だから filer は素で送り、3 行のうち **最初の 2 行が `<Enter>` の前に走る**（`pA.txt` と `pB.txt` ができる）。filer 側は正しい（下の見つけたもの 4） |
| 30.12 | **合** | `[term] shell = "cmd"`。cmd も `\e[?2004h` を送らない → `in paste  echo c1 > c1.txt\recho c2 > c2.txt\recho c3 > c3.txt\r`。**`<Enter>` を 1 度も押さずに `c1.txt` `c2.txt` `c3.txt` が 3 つとも**できた。cmd が書き返したものに `200~` / `201~` は **0 行** |
| 30.13 | **合** | nvim の挿入モードで右クリック。nvim は `\e[?2004h` を**送る**ので `in paste  \e[200~PASTE13-A\rPASTE13-B\e[201~`。`:wq` のあとの `v13.txt` は `PASTE13-A\r\nPASTE13-B\r\n` で、**`200~` も `201~` も入っていない**。nvim が描いたものにも 0 行 |
| 30.14 | **合** | 同じ 3 行を `<C-v>` で。`in paste` のバイトが右クリックと**完全に同じ**（`Set-Content pA.txt a\rSet-Content pB.txt b\rSet-Content pC.txt c`）で、結果も同じ（`<Enter>` の前に 2 つ、後に 3 つ）。行の末尾が指す「23.11」は 30.11 のことと読んだ |

### 見つけたもの 1: 選択範囲の上で右クリックしても、選択は置き換わらない（30.3）

`abcdef` の `cd` をドラッグで選び（写真で反転している）、その上で右クリックすると、
**選択はそのまま残り、クリックした位置に挿入される**（`abcXdef`）。行は「選択が置き換わる」と言う。

原因は `ui/overlay.rs` の読む順番にある。`right_click_paste` は egui の `TextEdit` が
**そのフレームの入力を処理したあと**に `TextEditState` を読む（`overlay.rs:72`）。
egui はどのボタンの押下でもキャレットを動かすので、読んだときには範囲は
すでにクリック位置に潰れている——関数の上のコメント（`overlay.rs:50`）自身が
「egui moves the caret on a press of *any* button」と書いていて、**コードはそれを知っている。**
知らないのは行のほうか、あるいは「選択の上なら残す」が実装されていないだけか、どちらか。

**直しかた（人の判断）**: 行を実態に合わせる（「クリックした位置に入る」）か、
コードを行に合わせる。後者なら、押下がその前の選択の**内側**だったときに限って
前フレームの範囲を使う——`Response::interact_pointer_pos()` は押下位置を持っているので、
`right_click_paste` が 1 フレーム前の `char_range` を覚えておけば判定できる。
**行の言うほうが、ブラウザでもターミナルでも普通の振る舞い**だとは思う。

### 見つけたもの 2: CRLF が、右クリックでは空白 2 つ、`<C-v>` では 1 つ（30.4）

同じクリップボード（`line one` CRLF `line two`）を、同じ rename のプロンプトに入れて確定した
結果がディスクで違う。

| 入れかた | できた名前 | 空白 |
| --- | --- | --- |
| 右クリック | `twoline one␣␣line two`（…,101,32,**32**,108,…） | **2 つ** |
| `<C-v>` | `ctrlvline one␣line two`（…,101,32,108,…） | 1 つ |

`overlay.rs:68` が `.replace(['\r', '\n'], " ")` で、**`\r` と `\n` を 1 文字ずつ**空白にしている。
`<C-v>` のほうは egui の paste イベントで、そこへ来る前に改行が正規化されているので 1 つで済む。

**ターミナル側は正しい。**`Terminal::paste`（`terminal.rs:591`）は
`.replace("\r\n", "\r").replace('\n', "\r")` と、**CRLF を先に畳んでいる**。
同じ畳みかたをプロンプト側にも入れれば直る——**1 行**。

行は「`<C-v>` と同じ」と明記しているので、これは行の間違いではなく**プログラムの不具合**。

### 見つけたもの 3: `cd` のプロンプトは現在地で埋まって開くので、30.1 は書いてあるとおりには通らない

`Act::Cd`（`app.rs:2377`）は `format!("{}\\", cwd.display())` でプロンプトを開く。だから
「アドレスバーからコピーして、`cd` のプロンプトを出して、入力欄を右クリック」を素直にやると、
**現在地の途中に絶対パスが差し込まれる**:

```
Change directory: C:\Users\yuu06\AppData\LocalC:\Users\yuu06\AppData\Local\Temp\filer-scratch\s30\jp\日本語テストA\Temp\filer-scratch\s30\jp\
```

（`shots\30-5-pasted.png`。この写真は同時に、**日本語が壊れずに入っていること**と
**キャレットが貼り付けの直後にあること**も見せている。）
`<C-a><Backspace>` で消してから貼ると、30.1 は期待どおりに通る（タイトルが `…\s30\filt` になる）。

一括リネームのプロンプトも同じで、`{name}{ext}` で埋まって開く。こちらは
**消さなくても貼り付け自体は働く**（`{name}{ext}pre-{name}{ext}` という規則になり、
プレビューもそのとおりに描かれた）。

**行の直しかた（人の判断）**: 30.1 に「入力欄を空にしてから」を足すか、
プロンプトの初期テキストを**選択済み**で開くようにするか。後者は下の提案 2。

### 見つけたもの 4: Windows のどの PowerShell も bracketed paste を要求しない（30.11）

30.11 は「PSReadLine が bracketed paste を要求するので、`<Enter>` まで何も走らない」と書いている。
**この機械では、どちらの PowerShell でもそうならない。**

| シェル | PSReadLine | ログ中の `2004` | 結果 |
| --- | --- | --- | --- |
| Windows PowerShell 5.1（filer の既定） | 2.0.0 | **0 回** | 貼った瞬間に 2 行走る |
| pwsh 7.6.6.0 arm64 | **2.4.5** | **0 回** | 同じ |

pwsh のほうは PSReadLine が動いていることがログで分かる（`\e[93mSet-Content\e[0m…` と
**構文の色付き**でコマンド行を描き直している）。それでも `\e[?2004h` は 1 度も出てこない——
Windows の PSReadLine はコンソール API で入力を読むので、**そもそも VT の
bracketed paste を使わない。**

**filer は正しい。**`Terminal::paste` は「プログラムがモードを立てたか」で決めていて
（`terminal.rs:596`）、立てるプログラム（nvim）では 30.13 のとおり `\e[200~` が付く。
**この節で「付く側」を確かめられたのは nvim だけ**で、それは確かめられた。

**行の直しかた（人の判断）**: 30.11 の「PowerShell のプロンプト」を
**bracketed paste を立てる程度の何か**に変える（nvim、あるいは WSL の bash）。
いまの文言だと、Windows で試した人は必ず「落ちた」と読む。

### 足場の覚え書き（次の run のために）

- **`<C-a>` も `<Backspace>` もプロンプトの入力欄に届く。**`on_key_event` が
  開いた Input から取るのは `Escape` / `Enter` / `Tab` だけ（`main.rs:686`）。
  だから `--keys "g<Space><C-a><Backspace>"` で「開いて、空にする」が 1 本で書ける。
- **`PostMessage` で大文字を送ると 2 回入る。**`PM::Key` は `WM_CHAR` を自分で投函するが、
  winit は `WM_KEYDOWN` からも文字を作るので、`B` が `Bb` になった（`shots\r4-30-5-typed.png`）。
  小文字では起きない。**文字を打つのは `chord.ps1` の `Send-Text`（SendInput）**にすること。
  #100 が `:wq` → `:wwq` で踏んだのと同じ穴。
- **`g` `<Space>` のような前置き付きの和音は、投函だと開かないことがある。**r3 では
  3 回とも開かず、続く `<C-a>` が一覧の `toggle_all` に、`<Backspace>` が `leave` になって、
  測定が丸ごと無意味になった。**`--keys` に移したら 3 回とも開いた。**
  役割定義の「Start with `--keys`」は、プロンプトを開けるところまでは本当に先に試すべき。
- 前の run（#103）の提案 4 が言っていた「前の run の置き土産のダイアログ」は、
  この run の最中には出なかった。`Click-At` は毎回 `foreground ours=True` を返している。

### Proposals

#### 1. 選択の上の右クリックは、選択を置き換えるべき

- **何が起きたか**: 見つけたもの 1。`abcdef` の `cd` を選んでからその上で右クリックすると、
  選択は残ったまま、クリック位置に貼られて `abcXdef` になる。
  ブラウザでもターミナルでも、選択の上への貼り付けは置き換えになる。
- **どう変えるか**: `right_click_paste` が**前フレームの `char_range` を覚えておき**、
  `resp.interact_pointer_pos()`（押下位置）がその範囲の中なら、潰れた後の範囲ではなく
  そちらを使う。`splice` はすでに範囲を受け取る形なので、渡す範囲を選ぶだけ。
- **なぜ**: 「一部を選んで、貼って直す」は、パスを打ち間違えたときに一番よくやる操作。
  いまはそれをすると**古い文字が残る**ので、気づかずに `<Enter>` を押すと
  存在しないパスへ行く（この run は `abcXdef` というファイルを作ってしまった）。
- **大きさ**: 関数 1 つ。egui の状態を 1 フレーム覚えるところだけ設計の判断が要る。

#### 2. プロンプトが埋まって開くなら、その初期テキストは選択済みで開くべき

- **何が起きたか**: 見つけたもの 3。`cd` は現在地で、一括リネームは `{name}{ext}` で
  埋まって開く。**どちらも「これを消してから打つ」が前提の初期値**なのに、選択されていない。
  30.1 を書いてあるとおりに試すと、`C:\Users\yuu06\AppData\LocalC:\Users\…` という
  ひどい行ができる。
- **どう変えるか**: `open_input` が初期テキストを入れるとき、
  `TextEditState` の `char_range` を **0..len** にして開く。打てば消え、`<C-v>` でも消え、
  `<End>` や右クリックで「残したい」ときは残せる。
- **なぜ**: rename は「拡張子だけ残して打ち直す」ので初期値が要るし、`cd` も
  「この下のどこか」へ行くときは要る。**要らないときに消す手間だけが今は毎回かかる。**
  エクスプローラーのアドレスバー（Alt+D）も、Windows のリネーム（F2）も、選択済みで開く。
- **大きさ**: `open_input` と、最初のフレームで状態を書くところ。**関数 1 つぶん。**
  ただし「rename は拡張子を除いて選ぶべきか」は持ち主の判断。

#### 3. 貼り付けの改行の畳みかたを、プロンプトとターミナルで 1 か所にするべき

- **何が起きたか**: 見つけたもの 2。CRLF が、プロンプトの右クリックでは空白 2 つ、
  `<C-v>` では 1 つ、ターミナルでは（正しく）`\r` 1 つになる。**同じクリップボードで 3 通り。**
- **どう変えるか**: `exec::get_clipboard()` が返す時点で `\r\n` を `\n` に畳む。
  そうすれば `overlay.rs:68` の `.replace(['\r','\n'], " ")` はそのままで正しくなり、
  `terminal.rs:591` の `.replace("\r\n", "\r")` も要らなくなる。
  **Windows のクリップボードは CRLF で来るのが普通**なので、入口で 1 回畳むのが素直。
- **なぜ**: いまは「右クリックで貼ったファイル名にだけ空白が 2 つ入る」という、
  見ても気づきにくい形で出る。この run はディスクの文字コードを見るまで気づかなかった。
- **大きさ**: 1 行 + 呼び出し側の整理。

#### 4. `--keys` の押下とマウスを、同じ口から入れられるようにするべき

- **何が起きたか**: この節は**全部の行にマウスが要る**。`--keys` は押せないので、
  右クリック・ドラッグは SendInput（入力デスクトップが要る）で入れ、
  「どの文字の間か」は**ピクセルを較正してから**当てた（`r1-cal.ps1` の 3 点）。
  較正だけで filer を 3 回起動している。
- **どう変えるか**: `--keys` の記法に**座標ではなく意味**でマウスを書けるものを足す。
  たとえば `<RClick:input@3>`（開いているプロンプトの 3 文字目の後ろ）、
  `<RClick:list@2>`（一覧の 2 行目）、`<Drag:input@2-4>`。
  filer は自分の矩形を知っているので、外から px を測るより**確かで短い**。
- **なぜ**: マウスの行は**実機レーンでしか測れない**のに、いまはその測定が
  「窓の位置 × DPI × フォント幅」に依存していて、窓の大きさが変わると全部やり直しになる。
  30 / 14 / 38 のようなマウスの節が順番表にまだ残っている。
- **大きさ**: 設計の判断が要る（記法と、どの要素に名前を付けるか）。
  `keyscript.rs` の拡張と、`raw_input_hook` に `PointerButton` のイベントを足すところ。

#### 5. 右クリックで貼れることを、どこかで一度は言うべき

- **何が起きたか**: この機能は README のキー表にもヘルプ（`~`）にも出てこない。
  キーではないので当然ではあるが、**プロンプトを開いた人がそれを知る道が無い。**
  この run は行とソースを読んで知った。
- **どう変えるか**: プロンプトの右端（`…` の出るところ）に、テキストが空のときだけ
  `right-click to paste` を薄く出す。あるいはヘルプの `[input]` の節に 1 行。
- **なぜ**: `<C-v>` を知っている人は右クリックを試さない。**「ターミナルと同じ」**という
  この機能の売りは、試してもらえないと伝わらない。
- **大きさ**: 描画 1 か所、**数行**。

### この run が触ったもの

- `TESTING-CHECKS.md`: 30.2 / 30.5〜30.10 / 30.12〜30.14 に `[x]`、
  `cargo run --example make-testcheck` で見出しと合計を書き直した（**182 / 401**、
  `-- --check` は `in sync`）。`make-keycheck -- --check` も `in sync`（243 / 247）。
- `QA-REPORT.md`: この節。
- ほかに変えたファイルは無い。`Cargo.toml` と `CHANGELOG.md` は役割定義のとおり触っていない。
  人の設定ファイル（`$PROFILE` など）には 1 つも触っていない——各 run の設定は
  run 専用の `FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` / `FILER_STATE_HOME`（証拠ディレクトリの下）に書いた。
- 止めたもの: `OLED Care Screensaver.scr`（この機械のスクリーンセーバ。idle で戻る）と、
  30.1 のために開いた Explorer の窓 1 つ（`WM_CLOSE` で閉じた）。
- 起動した `filer.exe` は **32 本**、いずれも run の最後に終わらせた。終了時の `Get-Process filer` は 0 本。

## TESTING.md section 31 — ホストの共有一覧を ARM64 で確かめた（8624ad5 / 0.54.14、ARM64 レーン）

Windows のセッション（`.claude/windows-role.md`、**ARM64 レーン、無人実行**）から。順番表の
ARM64 の表の先頭にあった section 31 を 1 つだけ進めた。

| | |
| --- | --- |
| 機械 | `PROCESSOR_ARCHITECTURE=ARM64`、Adreno X2-90（Vulkan）、窓はクライアント 1360x860 @ 1 |
| OS | `Windows 11 Home 26H1 (build 28000.2956)` |
| filer | 手元ビルド 0.54.14、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ |
| 昇格 | **無し**（`IsInRole('Administrators')` = False） |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |
| 設定 | **人の設定そのまま**（`%APPDATA%\yazi\config\yazi.toml` と `%APPDATA%\filer\keymap.toml`）。だから起動直後は `T` の重複束縛の警告トーストが 6 秒出る |
| `cargo test` | **509 / 0**（ARM64 でネイティブ実行、0.54.14） |
| `make-keycheck -- --check` | `in sync`（243 / 247）。TESTING-KEYS.md は触っていない |

生の証拠は `C:\dev\filer-evidence\arm-31\`（`shots\` に 24 枚、`out\` に各 run の
`--cwd-file` / `--chooser-file`、`scripts\` に足場の `fx31.ps1` と `run31.ps1` /
`watch31.ps1` / `refresh31.ps1` / `esc31.ps1` / `probe-host.ps1`）。

**13 行のうち 8 行に `[x]` を付けた**（31.1、31.3〜31.6、31.10〜31.12）。残り 5 行は付けていない:
**31.5a は行の参照が壊れている**、**31.9 は行と実装が食い違う**、**31.2 は半分だけ通った**、
**31.7 と 31.8 はこのネットワークに試験台が無い**。

### この節の試験台をどこから見つけたか

この節は「本物のファイルサーバが要る」と書いてある唯一の節で、無人の run に一番無い
ものがそれだった。順に潰した:

- **この機械自身は試験台にならない。**`Get-SmbShare` は `ADMIN$` / `C$` / `IPC$` の
  隠し共有しか持たず、`net view \\localhost` は `There are no entries in the list.`。
  filer で `\\localhost` を開くと `(empty)` / `0 items`（`shots\probe-localhost.png`）。
  **共有を作るには昇格が要る**（`New-SmbShare`）ので、この run では作れない。
- **WSL も無い。**`\\wsl$` は本物のネットワークプロバイダなので使えたはずだが、
  `wsl -l -v` は「Linux 用 Windows サブシステムがインストールされていません」。
- **LAN を 445 で当たった。**`192.168.0.1` / `.9` / `.38` / `.113`、Tailscale の
  `proxmox` / `couchdb` / `npm-gateway` — 全部閉じている。開いていたのは
  Tailscale の `desktop-pc`（`100.104.162.51`）だけで、そこは `net view` が
  **System error 5**。
- **見つけたのは資格情報マネージャ経由。**`cmdkey /list` に
  `Domain:target=192.168.0.150`（user `backup-user`）があり、当たってみると
  **Samba サーバ**だった。`net view \\192.168.0.150` が `Backup` / `backup-user` /
  `cache` / `VR_Video` の 4 つを返す。**この 1 台がこの節の試験台。**

つまり、この節を進めるには**この LAN に 192.168.0.150 が居ることが要る**。
順番表にそう書いておかないと、次に誰かが引いたとき同じ道を全部たどり直すことになる。

### 測り方 — 写真を数えない

- **一覧そのものは `--chooser-file` で読んだ。**`<C-a>`（`toggle_all --state=on`）で
  全行を選び、`q` で終わると**選択の絶対パスがファイルに書き出される**。
  共有が 4 つ並んでいることは、写真の行を数えるのではなく、このファイルと
  `net view` の出力を突き合わせて言っている。
- **行き先は `--cwd-file` と窓のタイトル**（`Filer: {cwd}`、クラス `Window Class`）。
  `\\` と `//` の綴りの違いはここで読める。
- **「固まらない」は `IsHungAppWindow`**。1 秒ごとに読んで、全 run で `False`。
- **「サーバに問い直す」は `\Redirector\Bytes Total/sec` の RawValue。**
  SMB リダイレクタ専用のカウンタなので、何もしていなければ動かない。
- **トーストは `PrintWindow`（`PW_RENDERFULLCONTENT`）の写真をテキストとして読んだ。**
  トーストは 6 秒で消える（`app.rs` の `toasts.retain`）ので、撮る時刻を選んでいる。
- **`cd` のプロンプトは現在地で埋まって開く**ので、UNC を打つ前に必ず `<C-a>` が要る
  （section 30 の 30.1 が見つけたのと同じ事実）。`--keys` は
  `g<Space><C-a>\\192.168.0.150<Enter>` の形になる。最初これを忘れて、
  `C:\…\home\\\192.168.0.150` を作って**黙って何も起きなかった**。

### 行ごとの結果

| 行 | 結果 | 根拠 |
| --- | --- | --- |
| 31.1 | **合** | `g<Space><C-a>\\192.168.0.150<Enter>` → タイトル `Filer: \\192.168.0.150`、`4 items`。`--chooser-file` が `\\192.168.0.150\Backup\` / `\VR_Video\` / `\backup-user\` / `\cache\` の 4 行。`net view \\192.168.0.150` の 4 つと**一致**（`shots\shot-31.1.png`） |
| 31.2 | **半分**（付けていない） | `//192.168.0.150` は届き、`--cwd-file` は **`\\192.168.0.150`**。綴りは戻されている。**ホスト名の側は測れない**: このサーバに届く名前は `proxmox.local`（mDNS、`[Net.Dns]` は 192.168.0.150 を返す）だけで、その綴りだと **Windows 自身が拒む**（`net view \\proxmox.local` も `Get-ChildItem` も error 5）。NetBIOS 名 `PROXMOX` は error 53。filer も同じく os error 5 のトーストを出した（`shots\shot-31.2-name.png`） |
| 31.3 | **合** | ホストの一覧で `l` → `--cwd-file` が `\\192.168.0.150\Backup\`、中身は `Job DESKTOP-7UBRVG9`。続けて `h` → `\\192.168.0.150` に戻り、共有 4 つが並ぶ |
| 31.4 | **合** | ホストで `h` を **3 回** → `--cwd-file` は `\\192.168.0.150` のまま、共有 4 つも同じ。`q` で普通に終わる（落ちていない） |
| 31.5 | **合** | 未使用アドレス `192.168.0.240`（ping も ARP も無応答）へ。**約 20 秒後**にタブが元の `…\s31\home` へ戻り、赤いトーストが **`\\192.168.0.240: ネットワーク パスが見つかりません。 (os error 53)`**（`shots\shot-31.5-dead-22s.png`）。その 28 秒の間 `IsHungAppWindow` は毎秒 `False` |
| 31.5a | **参照が壊れている**（付けていない） | 下の「見つけたこと 1」。中身は 31.1（一覧が出た）と 31.5（**os error 53**）で済んでいる |
| 31.6 | **合** | `\\100.104.162.51`（Tailscale の `desktop-pc`。445 は開いているが列挙を拒む）→ タブは動かず、赤いトーストが **`\\100.104.162.51: アクセスが拒否されました。 (os error 5)`**（`shots\shot-31.6-denied-4s.png`）。`\\proxmox.local` でも同じ形のトースト。どちらも **`net view` と同じ error 5** |
| 31.7 | **試験台が無い**（付けていない） | 1 画面（33 行）を超える数の共有を持つホストがこのネットワークに無い（192.168.0.150 は 4 つ）。作るには `New-SmbShare` = 昇格が要る |
| 31.8 | **試験台が無い**（付けていない） | 空白や非 ASCII を名前に含む共有が無い。同上、作るには昇格が要る |
| 31.9 | **行と食い違う**（付けていない） | 下の「見つけたこと 2」 |
| 31.10 | **合** | ホストの一覧で `<F5>`。リダイレクタのバイト数が **静止 5 秒 +136 → `<F5>` を挟んだ 5 秒 +2393 → もう一度静止 5 秒 +0**。つまり**サーバに問い直している**。一覧は 4 件のまま、`IsHungAppWindow` False、プロセスも生きている（`shots\shot-31.10.png`） |
| 31.11 | **合** | `<C-w>` → タブ帯が `1 192.168.0.150` / `2 192.168.0.150`、**両方のペインが同じ 4 つの共有**を並べる（`shots\shot-31.11-pane.png`）。2 つ目のタブは `t`（`Backup` で開く）から `h` で、タブ 2 が `\\192.168.0.150`、`--chooser-file` が 4 件（`shots\shot-31.11-tab2.png`） |
| 31.12 | **合** | ホストへ行ってから `g<Space><C-a>C:\…\s31\home<Enter>` で離れ、26 秒見張った。**トーストは 1 つも出ない**（6 / 14 / 20 / 26 秒の写真）。CPU も 0.6875 → 0.703125 でほぼ平ら |

### 見つけたこと

#### 1. 31.5a の「24.1 と 24.5」は、文字から数字へ直したときに壊れた参照

行はいま「**24.1 と 24.5** をもう一度、トーストが出るかを見ながら」と読める。24 は
「Awkward names」で、24.1 は CJK の名前、24.5 はごみ箱の行。**この節の中身と何の関係も無い。**

元は節に文字が振られていた頃の行で、`git show c27b81b:TESTING.md` にこうある:

    | R5a | R1 and R5 again, watching for a **toast** | v0.16.0 fell back to the parent in silence, …

`R` がこの節（いまの 31）で、**`R1` と `R5` は同じ節の 31.1 と 31.5**。数字化のときに
セルの中の相互参照だけが別の節番号に化けた（節の見出しはその後さらに繰り上がっている）。

- **直し方**: `24.1 と 24.5` → **`31.1 と 31.5`**。TESTING.md（英語が正）の
  `24.1 and 24.5 again` → `31.1 and 31.5 again`。
- **中身は済んでいる**: 31.1 は一覧が出た、31.5 は **os error 53** のトースト、
  31.6 は **os error 5**。行が直れば、この段落がそのまま `[x]` の根拠になる。
- 番号を勝手に書き換えないのが役割定義なので、**この run は行を直していない。**

#### 2. 31.9「サイズの列は空」は、いまの filer では成り立たない

`m` `s`（`linemode size`）でホストの一覧を見ると、共有 4 つに**子の数**が出る:

    Backup        1
    backup-user   3
    cache        10
    VR_Video      1

これは `Get-ChildItem \\192.168.0.150\<share>` の数（1 / 3 / 10 / 1）と**完全に一致**する。
つまり filer は**共有 1 つにつき 1 回、ネットワーク越しに `read_dir` を飛ばしている**。
行は「空。読むものが無い」と言っているので、そこは食い違う。

行の後半（「そこで数え続けてはいけない」）は**満たしている**: 8 秒の写真と 29 秒の写真が
同じで、CPU は 6 秒以降 **0.40625 で完全に平ら**（30 秒間 1 ティックも増えていない）。
既定の linemode（`none`）ではそもそも列が無く、フッタも共有の上では `drw` だけで
サイズも日付も出ない。

**どちらを直すかは持ち主の判断。**「共有の子の数が見えるのは便利」も十分あり得るので、
行のほうが古いのかもしれない。ただし遅い NAS では、ホストを開いた瞬間に共有の数だけ
ラウンドトリップが飛ぶ（下の提案 3）。

#### 3. 31.10 の `<C-r>` は、filer では redo

行は「`<C-r>` / 再読み込み」と書いてあるが、この keymap で `<C-r>` は **`redo`**
（`U` と同じ。`keymap.toml` に「Ctrl+R を redo にするほうが互換性より価値がある」と
書いてある）。再読み込みは **`<F5>`**。この run は `<F5>` で測った。
行の「`<C-r>` /」は yazi の名残なので、消すか `<F5>`（再読み込み）に直すのがよい。

#### 4. 届かないホストへ行くと、タブは**先に**そこへ移り、20 秒黙って待つ

31.5 の測定でいちばん目についたのはこれ。`\\192.168.0.240` を打った直後（1 秒後）に
もう窓のタイトルが `Filer: \\192.168.0.240`、タブ帯も `1 192.168.0.240`、
ヘッダも `\\192.168.0.240`、`0 items`、フッタ `0/0`。一覧の場所には小さな `…` が
1 つあるだけ（`shots\shot-esc-7s.png`）。**そのまま 20 秒**。21 秒目にタブが戻り、
そこで初めて os error 53 のトーストが出る。

- 窓は生きている（`IsHungAppWindow` が毎秒 `False`）。
- **`<Esc>` では抜けられない。**4 秒目に `<Esc>` を送っても、9 秒目に `j` を送っても、
  タイトルは `\\192.168.0.240` のまま 21 秒目まで変わらない（`esc31.ps1` の出力）。

行が要求する「固まらない」「元の場所に戻る」「理由が出る」は**全部満たしている**ので
`[x]` は付けた。ただし読む人の目には「**着いた**、中身が空だ」に見える 20 秒がある。
提案 1 に書いた。

### Proposals

#### 1. 着いていないうちは「着いた」と書かないでほしい

- **何が起きたか**: 上の 4。応答しないアドレスへ `cd` すると、タイトル・タブ・ヘッダ・
  `0 items` が**即座に行き先のもの**になり、20 秒後に全部戻る。待っている印は
  一覧の欄の小さな `…` ひとつだけで、`0 items` と `0/0` のほうがよほど大きく目に入る。
  無人の run でさえ「空の共有一覧が出た」と読み違えかけた。
- **どう変えるか**: スキャンが返るまでタブの移動を保留し、代わりに
  「`\\192.168.0.240` に問い合わせています…」をトーストか一覧の中央に出す。
  最低でも `0 items` を出さない（数が分かっていないのだから `—` か空）。
  この状態は `LoadState` が既に持っているので、描画側の分岐で足りるはず。
- **なぜ**: UNC は**失敗するのが普通の道**（この run でも 4 つ試して 3 つが失敗した）。
  そこで 20 秒間まちがったことを表示するのは、機能そのものの信頼を削る。
  遅いのは Windows の SMB タイムアウトで filer のせいではないが、**嘘を表示するかどうかは
  filer の側の選択**。
- **大きさ**: 描画とタブ移動の順序。`LoadState` を見る分岐が 2〜3 か所。

#### 2. その 20 秒を `<Esc>` で捨てられるようにするべき

- **何が起きたか**: 待っている間に `<Esc>` を押しても、`j` を押しても、21 秒目まで
  何も変わらない。アドレスを打ち間違えたと気づいても、**打ち直せない。**
- **どう変えるか**: 未完了のスキャンがあるときの `<Esc>` を「この `cd` を取り消して
  元の場所へ戻る」にする。newest-wins のワーカーは既に古い依頼を捨てる作りなので、
  待っているスレッドを止める必要はなく、**結果が来たときに捨てればよい**。
- **なぜ**: 打ち間違いは UNC でこそ起きる（数字が 12 桁並ぶ）。いまは 20 秒座って待つか、
  窓を閉じるかの 2 つしかない。
- **大きさ**: `EscapeWhat` に 1 つ足して、`Tab` の pending なスキャンを無効化する。

#### 3. サイズの列の数え上げを、UNC の行では控えるべき

- **何が起きたか**: 31.9。`linemode size` でホストの一覧を開くと、共有 4 つそれぞれに
  `read_dir` が飛び、子の数が出る。ここでは共有が 4 つで中身も小さいので一瞬だったが、
  **数える相手は `\\host\share` のルート**で、遅い NAS や広域の共有だと同じことが
  重い round trip になる。
- **どう変えるか**: ホストの一覧（`host_only_unc` の下の行）では子の数を数えない。
  あるいは UNC の行だけ、ホバーしたものに限って数える。
- **なぜ**: ホストの一覧は「どの共有に入るか選ぶ」ためだけの画面で、子の数は
  そこで要る情報ではない。いま数えているのは**画面に出ている全部**。
- **大きさ**: 数えるかどうかを決めている `counted` の呼び出し 1 か所に条件を足す。

#### 4. 「共有が 0 個」と「聞けなかった」を、同じ `(empty)` で書かないでほしい

- **何が起きたか**: `\\localhost` を開くと `(empty)` / `0 items` が出る。この機械は
  隠し共有しか持っていないので**これは正しい**のだが、画面からは
  「本当に 0 個」なのか「列挙が空を返した」のか「まだ待っている」のか区別が付かない。
  待っている最中（提案 1）の画面とも見分けが付かない。
- **どう変えるか**: ホストの一覧が**成功して 0 件**のときは `(no shares)` のように、
  ディレクトリの `(empty)` とは別の言葉にする。ついでに
  「隠し共有（`C$` など）は出ない」も同じ行に書けるとよい。
- **なぜ**: この節は「**言うことを聞かないときが本番**」と自分で書いている節で、
  いちばん多い「何も出ない」に説明が無い。
- **大きさ**: 空のときの文字列を 1 か所、ホストかどうかで分ける。**数行**。

#### 5. `cd` のプロンプトが現在地で埋まっているのは、UNC では毎回じゃまになる

- **何が起きたか**: `g` `<Space>` は現在地が入った状態で開く。`\\192.168.0.150` は
  現在地の**続き**ではないので、UNC を打つときは**必ず** `<C-a>` か `<C-u>` が要る。
  この run は最初それを忘れて、`C:\…\home\\\192.168.0.150` という無意味なパスを作り、
  **トーストも出ずに何も起きなかった**（`resolve_against` が相対として畳んでしまう）。
  section 30 の 30.1 が Explorer からの貼り付けで同じ壁に当たっている。
- **どう変えるか**: 開いた直後は**中身を全選択状態**にする（打てば置き換わり、
  `<End>` を押せば今までどおり続きを打てる）。yazi は空で開くが、そこまで変えなくても
  全選択で両方立つ。
- **なぜ**: 2 つの節が別々の入口から同じ所で転んだ。しかも**失敗が無音**なので、
  気づくまでに時間がかかる。
- **大きさ**: プロンプトを開くときにカーソルではなく選択範囲を置く。数行。

### Queue

`windows-role.md`「The ARM64 lane」の表から **section 31 の行を消す**。
残り 5 行（31.2 の名前の側 / 31.5a / 31.7 / 31.8 / 31.9）は**この機械では進められない**:
31.7 と 31.8 は `New-SmbShare`（昇格）か、そういう共有を持つサーバが要る。
31.2 の名前の側は Windows 自身が拒む。31.5a は行が直るまで、31.9 は持ち主の判断待ち。

次の行（**28. changes made from outside**）はそのまま。31 を引く人のために、
表の「What it is on ARM64」に**この LAN の試験台は `\\192.168.0.150`（Samba、共有 4 つ）
で、`net view \\192.168.0.150` が通ることが前提**と書き添えるとよい。

### この run が触ったもの

- `TESTING-CHECKS.md`: 31.1 / 31.3〜31.6 / 31.10〜31.12 に `[x]`、
  `cargo run --example make-testcheck` で見出しと合計を書き直した（**190 / 401**、
  `-- --check` は `in sync`）。`make-keycheck -- --check` も `in sync`（243 / 247）。
- `QA-REPORT.md`: この節。
- ほかに変えたファイルは無い。`Cargo.toml` と `CHANGELOG.md` は役割定義のとおり触っていない。
- **人の設定ファイルには 1 つも書いていない。**この run は人の設定を**読む側**で使った
  （run 専用の `FILER_CONFIG_HOME` は作っていない）ので、`%APPDATA%\filer` の
  状態ファイル（履歴）には今回の訪問先が残る。設定そのものは変えていない。
- ネットワークへは**読みだけ**: `net view` / `Test-NetConnection` / `Get-ChildItem` と、
  filer からの共有一覧。`New-SmbShare` も `net use` も `cmdkey /add` も実行していない。
- 起動した `filer.exe` は **19 本**、いずれも run の最後に終わらせた。終了時の
  `Get-Process filer` は 0 本。

---

## ARM64 の実機 — v0.55.0 が Q25〜Q31 で変えた行（v0.55.1、無人の run）

`.claude/windows-role.md` の「The ARM64 lane」の先頭、**v0.55.0, the answers to
Q25-Q31** の節。ネイティブ ARM64 で、`origin/main` の 4d0aae5（v0.55.1）を
自分でビルドしたバイナリを動かした。

- `filer env`: **Process arch aarch64** / OS arch aarch64 / Windows 11 Home 26H1
  (build 28000.2956) / Version 0.55.1。エミュレーションではない。
- `cargo test`: **523 passed; 0 failed**（0.54.14 の 509 から増えている）。
- ペインの ConPTY は `scripts/fetch-conpty.ps1` が入れた **1.24.260710001 (arm64)**。
- **昇格していない**（`IsInRole('Administrators')` = False）。この節に昇格の要る行は無い。
- 開始時に ASUS の OLED Care スクリーンセーバーが走っていた。役割定義のとおり
  `Get-Process | Where-Object ProcessName -match 'OLED Care' | Stop-Process -Force` で
  止め、以後 `OpenInputDesktop` は最後まで `Default`、
  `SPI_GETSCREENSAVERRUNNING` は False だった。

### 結果

| 行 | 結果 | 根拠 |
| --- | --- | --- |
| 1.31 | ○（x64 で `[x]` 済み。ARM64 でも同じ） | 40.15 の中で確認。lazygit の `?` で開いたキー一覧が `<Esc>` の **663 ms 後**に閉じていた |
| 1.32 | ○（同上） | ペインの pwsh プロンプトで `abc` → 画面に `> abc`、`<Esc>` → `> ` だけ |
| 1.33 | ○（同上） | `pwsh -File scripts\keyprobe.ps1 -Query -Log …` の記録が `\e[?2048;0$y\e[?1000;2$y\e[?1006;2$y\e[?9001;1$y` **`\e[?6c`**。`\e[?61;6;7;…c` ではなく、`{up:…}` も無い |
| 1.34 | ○（同上） | 続けて `lazygit` → いつもの 6 ペインで開き、**メニューは出ていない**（40.15 の 1 枚目） |
| 29.2 | **`[x]` を付けた** | `$PROFILE` に README のフックを追記 → ペインで `cd C:\dev` → `<A-Up>` → 窓のタイトルが `Filer: C:\Users\…\p30` から **`Filer: C:\dev`** へ |
| 29.6 | **`[x]` を付けた** | 既定（`[term] shell` 無し）: ペインで `$PSVersionTable.PSVersion.ToString()` をファイルに書かせて **`7.6.6`**。`filer env` も `pwsh … (terminal pane, the platform default)`。run 専用の `FILER_CONFIG_HOME` に `[term] shell = "powershell"` を置くと **`5.1.28000.2952`**、`filer env` は `powershell … (terminal pane, from [term] shell)` |
| 30.1 | **×（下の「見つけたこと 1」）** | Explorer のアドレスバー（`Alt+D` → `Ctrl+C`、クリップボードは事前に番兵で潰してある）でパスを取り、`g<Space>` の欄を右クリック → パスは**選択を置き換えず、クリック位置に挿入**された。`<Enter>` は `os error 123` のトーストで、移動しない |
| 30.15 | **×（5 つのうち 4 つは通る）** | `r` は `report` を選ぶ（`zz` と打って `zz.txt` になった）。`E` は `.zip` の前の `a` を選ぶ（`NEW` と打って `NEW.zip`）。`R` は `{name}{ext}` 全体を選ぶ（`pre-{name}{ext}` と打って `pre-a.txt` / `pre-NEW.zip`）。`cd` はパス全体を選ぶ（打ち直しで `p30-dest` へ移動）。**右クリックだけが置き換えない** |
| 40.14 | **`[x]` を付けた** | ペインを最大化した nvim（`winheight` 34）で、ホイール 1 ノッチ前後の `writefile([line('.'),line('w0')])` が **507/491 → 507/494**。表示は 3 行動き、カーソルは動かない。`FILER_PTY_LOG` の `in key` は `\e[<65;46;29M`、矢印（`\e[A` / `\eOA`）は**このログに 1 行も無い** |
| 40.15 | **`[x]` を付けた** | lazygit に `<S-End>` を 33 ms 間隔で 150 回 → `?` → 一覧が開く → `<Esc>` の直後に `<S-End>` を 14 回続けたまま **663 ms 後**のスクリーンショットで一覧は閉じていた（フッタが `実行: <enter> ¦ 閉じる/キャンセル: <esc>` から `ステージ: <space> ¦ …` に戻っている）。合計 354 回の `<S-End>` を送って固まらない |
| 40.16 | **`[x]` を付けた** | ログの 5 行目（`6 out`）に **`\e[c\e[?1004h\e[?9001h`**。`in key` はすべてレコード: `<C-c>` = `\e[67;46;3;1;8;1_`、`<C-Left>` = `\e[37;75;0;1;264;1_`、`<Tab>` = `\e[9;15;9;1;0;1_`、`<Esc>` = `\e[27;1;27;1;0;1_`、`<S-End>` = `\e[35;79;0;1;272;1_`。**`\e[1;5D` も `\e[1;2F` も 0 件**。`<C-c>` は ping を止め（統計が出て `Ctrl+C` が表示された）、`echo abcdef ghijkl` の途中で `<C-Left>` → `X` と打つと `echo abcdef Xghijkl` が実行され（1 語だけ戻っている）、`Get-Chi` + `<Tab>` は `Get-ChildItem` になった |
| 45.16 | **`[x]` を付けた** | `orig\ln` → `orig\t1`、`copy\ln` → `copy\t1` の 2 つのジャンクション（`LinkTarget` は絶対パスで互いに違う）を `<A-d>` で比較 → **`= ln`**、フッタは `0 only left · 0 only right · 0 differ · 4 match` |
| 21.4 | 触っていない | 自動テスト済み（`make-testcheck` が除外している） |
| 40.8 | 触っていない | 同上 |

### 見つけたこと 1（バグ）: 右クリックの貼り付けが、実際のクリックでは選択を置き換えない

**v0.55.0 で直したことになっている 30.3 の修正（`ui/overlay.rs` の `paste_over`）が、
人が押す長さのクリックでは効かない。** 30.1 / 30.3 / 30.15 の右クリック側が、
いま全部これで落ちる。

再現（3 通りとも同じ）:

1. `g<Space>` → `cd` のプロンプトがパス全体を選択した状態で開く。
2. 選択の上を右クリック（`SendInput` で押下 → **120 ms** → 離す）。
3. 欄は `C:\Users\yuu06\AppData\Loca` + クリップボードのパス + `l\Temp\filer-scratch\p30-dest\`。
   **選択は消えただけで、置き換わっていない。**

キーボードで作った選択でも同じ: `<Home>` → `<S-Right>` ×10 で先頭 10 文字を選び
（スクリーンショットで青く反転しているのを確認）、5 文字目を右クリック →
`C:\Us` + クリップボード + `ers\yuu06\…`。

**押下と離すを 1 回の `SendInput` にまとめると、正しく置き換わる**（欄がクリップボードの
パスだけになる）。ここが原因の証拠で、読みはこうなる:

- `input()` は `ui.put()` の**前**に `selection()` を呼び、「前のフレームの選択」を取る。
- `resp.secondary_clicked()` が真になるのは**離したフレーム**。押下のフレームで
  egui が選択をキャレットに畳んでしまうので、離したフレームから見た「前のフレーム」は
  **もう畳まれたあと**で、`selected` は幅 0 の範囲になる。
- `paste_over` は `s.start < s.end` を要求するので、そのまま `at`（クリック位置）に落ちる。
- 押下と離すが同じフレームに入ったときだけ、「前のフレーム」が本物の選択になる。

`overlay.rs` の doc コメント（「egui has collapsed any selection to the click by the
time the paste runs, so `at` is the click; `selected` is what the frame before had
selected」）は、押下と離すが 1 フレームに入る前提で書かれている。実際のクリックは
数フレームまたぐ。**直すなら、押下の時点（`resp.is_pointer_button_down_on()` が
立った最初のフレーム）の選択を覚えておいて、それを `paste_over` に渡す**のが
いちばん小さい。`<C-v>` は同じ選択をきちんと置き換える（確認済み）ので、
置き換えの経路そのものは動いている。

この run では直していない（役割定義のとおり）。いまのテストは `paste_over` の
単体テストだけで、「押下と離すが別フレーム」を通っていない。

### 見つけたこと 2（観察）: ホイール 1 ノッチとマウスレポートの数が 1 対 1 でない

40.14 で `SendInput` の `MOUSEEVENTF_WHEEL`（120 単位 = 1 ノッチ）を送った数と、
`FILER_PTY_LOG` に出た `\e[<64;…M` / `\e[<65;…M` の数が合わない。

| 送ったノッチ | ログのレポート | nvim が動いた行数（`mousescroll` は `ver:3`） |
| --- | --- | --- |
| 3 | 5 | 15 |
| 2 | 3 | 9 |
| 1 | 1 | 3 |

1 ノッチにつき 1.67 本という形なので、端数を溜めて出しているように見える。
40.8（「上に 1 ノッチ、下に 1 ノッチで元に戻る」）はこの溜めの上に立っている行で、
いまは自動テストになっているが、**溜めが残ったまま向きを変えたときに何が起きるかは
実機でしか出ない**。バグと断定はしない（この機械のホイール設定＝1 ノッチ 3 行と
egui の smooth scroll のどちらの影響かを、この run では切り分けていない）。

### 見つけたこと 3（TESTING.md の文言）: 40.14 は nvim の `scrolloff` に依存する

40.14 を素直に「ペインは 3 分の 1 のまま、nvim を開いてホイール」でやると、
**カーソルは必ず動く。** この機械の nvim は `scrolloff=4` で、3 分の 1 のペインは
`winheight=11`。カーソルが居られるのは `w0+4`〜`w0+6` の 3 行しかなく、1 ノッチが
3 行なので、どちらへ回してもはみ出して引き戻される（実測: 500/495 → 514/510）。

`<C-S-Enter>` でペインを窓いっぱいにする（`winheight=34`）と、行の文言どおり
カーソルは動かない。**行に「ペインを最大化してから」か「`:set scrolloff=0` で」を
足すべき。**そうしないと、通るかどうかが人の nvim 設定次第になる。

### Proposals

#### 1. 起動中の filer にキーを送る口がほしい（`filer send-keys`）

- **何が起きたか**: この節の 11 行のうち、`--keys` だけで済んだのは 45.16 の 1 行。
  残りは全部「押す → 画面かファイルを読む → 次を押す」で、`--keys` は
  `App::settled()` を待つ一括指定なので途中で読めない。結局 `SendInput` で
  駆動したが、`SendInput` は**窓を最前面にしないと届かない**し、スクリーンセーバーに
  取られる。`PostMessage` なら最前面が要らないが、**修飾キーが送れない**
  （winit は Ctrl / Shift / Alt を `GetKeyState` で読むので、ポストした
  `VK_CONTROL` は見えない。実際 `<C-v>` がただの `v` として届いた）。
  つまり無人の run から `<C-t>` や `<C-S-Enter>` を安全に送る道が今は無い。
- **どう変えるか**: 起動中のインスタンスにキー列を渡す口を 1 つ作る。
  `filer send-keys "<C-t>"` が名前付きパイプ（Windows）/ Unix ソケットで
  同じ `--keys` の記法を投げ、`filer send-keys --shot out.png` で
  そのときの窓を返す、くらいの形。
- **なぜ**: 実機の run はこれから何十節も残っていて、**全部が「押して読む」**。
  いまは毎回、座標の較正とスクリーンセーバー対策と修飾キーの回避策を書き直している。
  口が 1 つあれば、この役割の run がそのまま**再実行できるスクリプト**になる。
  TODO.md の「`--keys` でクリックする」と同じ問題の、もう半分。
- **大きさ**: 設計。受け口（1 スレッド）と、`Act` への流し込みは既にある。

#### 2. `cd` が 1 回失敗するとトーストが 3 つ出る

- **何が起きたか**: 30.1 で壊れたパス
  （`C:\Users\yuu06\AppData\LocaC:\Users\…\p30l\Temp\filer-scratch\p30-dest\`）に
  `<Enter>` を押したら、赤いトーストが**3 つ**出た。1 つ目は全文と
  `os error 123`、2 つ目は `Temp: …`、3 つ目は `filer-scratch: …`。
  いちばん下の 2 つはパスの途中の名前だけで、何を指しているのか読めない。
- **どう変えるか**: 1 回の `cd` の失敗は 1 つのトーストにする。中で複数の候補を
  試しているなら、いちばん外側の失敗だけを出す。
- **なぜ**: 失敗の理由（`os error 123` = 名前の構文が不正）は 1 つ目に全部出ている。
  あとの 2 つは画面を 2 行ぶん押し上げるだけで、**どこを直せばいいのかは増えない**。
  30.1 は「右クリックで貼ってから `<Enter>`」の行なので、ここは人が必ず通る。
- **大きさ**: トーストを出している所の 1 か所。数行。

#### 3. ペインのシェルが `pwsh` になったことを、`filer env` 以外でも言ってほしい

- **何が起きたか**: 29.6 で `[term] shell` を書かずに `<C-t>` すると `pwsh` が立つ。
  正しい。ただし**それが分かるのはバナー（`PowerShell 7.6.6`）を読んだときだけ**で、
  バナーを消している人には何も出ない。5.1 と 7 は `$PROFILE` が別ファイルなので、
  29 節の OSC 7 フックはここですれ違う（`config/mod.rs` のコメントが、まさにそう書いている）。
- **どう変えるか**: ペインを初めて開いたときのトーストに、起動したシェルの名前を
  入れる（`Started pwsh` くらい）。`<C-S-t>` の「Ended the shell」と対になる。
- **なぜ**: 「フックを入れたのに `<A-Up>` が効かない」の原因の 1 位がこれで、
  いまは `filer env` を開くまで気づけない。
- **大きさ**: トーストの文字列 1 つ。数行。

### Queue

`windows-role.md`「The ARM64 lane」の表から、**先頭の
「v0.55.0, the answers to Q25-Q31」の行を消す**。10 行のうち 8 行が済み
（1.31〜1.34 は x64 の `[x]` を ARM64 で追認、29.2 / 29.6 / 40.14 / 40.15 / 40.16 /
45.16 に `[x]`）、残った 30.1 と 30.15 は**上の「見つけたこと 1」が直るまで
進められない**。21.4 と 40.8 は自動テスト済みで、表に書かれていたが押す対象ではない。

次の行（**28. changes made from outside**）はそのまま。

### この run が触ったもの

- `TESTING-CHECKS.md`: 29.2 / 29.6 / 40.14 / 40.15 / 40.16 / 45.16 に `[x]`、
  `cargo run --example make-testcheck` で見出しと合計を書き直した（**196 / 405**、
  `-- --check` は `in sync`）。`make-keycheck -- --check` も `in sync`（243 / 248）。
- `QA-REPORT.md`: この節。
- ほかに変えたファイルは無い。`Cargo.toml` と `CHANGELOG.md` は役割定義のとおり触っていない。
- **人の設定ファイル**: 29.2 のために `$PROFILE` を触った。これは
  `C:\Users\yuu06\OneDrive\ドキュメント\PowerShell\Microsoft.PowerShell_profile.ps1` で、
  **`C:\dev\obsidian-notes\notes\config\PowerShell\…` への SymbolicLink**（git 管理下）。
  手順は役割定義のとおり:
  - 前: `LinkTarget` を記録、6486 バイト、SHA-256
    `07FBCB5CA3201101D027DA3106DD0BA791016F74DC4AA631A5EB6527634FFCF4`、
    `evidence\PROFILE.backup.ps1` に複製。
  - 中: `Add-Content` で**追記のみ**（`Set-Content` は使っていない）。
    `git -C C:\dev\obsidian-notes status` は ` M notes/config/PowerShell/…` になった。
  - 後: バックアップのバイト列を書き戻し、SHA-256 は**同じ値**、`LinkType` は
    `SymbolicLink` で同じ行き先、`git status` は**空**（clean）に戻った。
  - 29.6 の PowerShell 5.1 側は run 専用の `FILER_CONFIG_HOME` を使ったので、
    人の `filer.toml` は触っていない。
- 起動した `filer.exe` は **9 本**、いずれも run の最後に終わらせた。終了時の
  `Get-Process filer` は **0 本**。テスト用に開いた Explorer の窓も閉じた。
- 作業用のディレクトリはプロンプトが指定した
  `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い）。
  `TEMP` / `TMP` も同じ。スクリーンショットとログは同じ場所の `evidence\` に置いてあるが、
  **証拠はこの節とプルリクエストの本文に文字で写してある**。

---

## TESTING.md section 28 — 外から加えられた変更を ARM64 で確かめた（0c48810 / 0.55.2、ARM64 レーン、無人の run）

`.claude/windows-role.md`「The ARM64 lane」の順番表の先頭、**28. changes made from
outside**。ネイティブ ARM64 で、`origin/main` の 0c48810（v0.55.2）を自分でビルドした
バイナリを動かした。

- `filer env`: **Process arch aarch64** / OS arch aarch64 / Windows 11 Home 26H1
  (build 28000.2956) / Version 0.55.2。エミュレーションではない。
- `cargo test`: **523 passed; 0 failed**（0.55.1 と同じ本数）。
- ペインの ConPTY は `scripts/fetch-conpty.ps1` が入れた **1.24.260710001 (arm64)**。
  この節はペインを使わない。
- **昇格していない**（`IsInRole('Administrators')` = False）。この節に昇格の要る行は無い。
- 開始時に ASUS の OLED Care スクリーンセーバーが走っていた
  （`OpenInputDesktop` が `Screen-saver`、`SPI_GETSCREENSAVERRUNNING` が True）。
  役割定義のとおり
  `Get-Process | Where-Object ProcessName -match 'OLED Care' | Stop-Process -Force` で
  止め、以後は最後まで `Default` / False。`Get-Process LogonUI` は 0 本のまま。

### 測り方

7 行とも「落ちない」と「カーソルがどの行に乗ったか」なので、見た目は 1 つも要らなかった。

- **落ちない** = `Get-Process filer` が居て、**プロセス ID が削除の前後で同じ**こと。
  「居る」だけだと、落ちて別プロセスが立った場合と区別できない。
- **どの行に乗ったか** = `c` `c`（`copy path`）→ `Get-Clipboard`。クリップボードは
  毎回 GUID の番兵で潰し、**番兵が入ったことを読み返してから**キーを送る。
  番兵のままなら「何もコピーされなかった」。最初の 2 回、番兵を確かめずに読んで
  1 つ前の値を掴み、存在しないファイルのパスが返ってきたように見えた。
- **一覧の中身と件数** = `PrintWindow`（`PW_RENDERFULLCONTENT`）の 1 枚。
  右上の `N items` と行の名前は文字なので読める。
- **外からの削除** = `SHFileOperation`（`FO_DELETE` + `FOF_ALLOWUNDO`）。エクスプローラーが
  使うのと同じシェル API で、ごみ箱に入る。複数ファイルは `pFrom` に並べて **1 回の
  呼び出し**にした（28.2 が「一度に」と言っているのはここ）。GUI のエクスプローラーの
  窓は使っていない。
- **キー** = 起動時のものは `--keys`（`G` / `fm<Enter>G` / `<C-w>G`）、起動後は
  `PostMessageW`（`WM_KEYDOWN` + `WM_CHAR` + `WM_KEYUP`、`lParam = 1 | (MapVirtualKey(vk,0) << 16)`）。
  **`--keys` が `<C-w>` と絞り込みの入力を両方こなした**ので、この節では修飾キーを
  `PostMessage` で送る必要が無かった（v0.55.1 の提案 1 が挙げた穴に当たっていない）。

### 結果

7 行すべて `[x]` を付けた。`make-testcheck` を回して 28 節は **7 / 7**、全体は
196 / 405 → **203 / 405**、`-- --check` は `in sync`。

| 行 | 結果 | 根拠 |
| --- | --- | --- |
| 28.1 | **`[x]`** | `f1`〜`f6` の最終行 `f6.txt` にカーソル（`--keys G`、`c c` で確認）→ `SHFileOperation` で `f6.txt` をごみ箱へ → カーソルは **`f5.txt`**、pid は 21920 のまま |
| 28.2 | **`[x]`** | 続けて、カーソルが `f5.txt` に乗った状態で `f3` / `f4` / `f5` を **1 回の `SHFileOperation`** で削除 → カーソルは **`f2.txt`**、pid 21920 のまま。スクリーンショットは `2 items`、一覧は `f1.txt` / `f2.txt` |
| 28.3 | **`[x]`** | `n1`〜`n4` を一度に全部削除 → 一覧は **`0 items`**、中央に `(empty)`。`c c` は**何もコピーしない**（番兵のまま）。pid 同じ。操作も効く: `h` で親へ抜け、窓のタイトルが `Filer: …\s28` → **`Filer: …\filer-scratch`**、`c c` が `…\filer-scratch\s28` を返した |
| 28.4 | **`[x]`** | ディスク上は 6 件（`a-plain` / `b-plain` / `m1` / `m2` / `m3` / `z-plain`）。`--keys fm<Enter>G` で絞り込んだ最終行は **`m3.txt`**（絞り込みが無ければ `z-plain.txt` のはず）→ `m3.txt` を外から削除 → カーソルは **`m2.txt`**、pid 同じ。**絞り込みは保たれている**: スクリーンショットは `2 items` で `m1.txt` / `m2.txt` だけ（`m` が反転）、ディスクには 5 件残っている |
| 28.5 | **`[x]`** | (a) `--keys <C-w>G` で 2 ペイン（タブ帯が `1 s28` と選択中の `2 s28`）、右ペインの最終行 `k5.txt` → 外から削除 → **`k4.txt`**、両ペインとも `k1`〜`k4`、pid 21624 のまま。(b) `sub`（`p1`〜`p5`）にカーソルを置いてディレクトリ・プレビューを出し、`p4` / `p5` を外から削除 → **プレビューが `p1.txt` / `p2.txt` / `p3.txt` に変わった**、pid 30392 のまま |
| 28.6 | **`[x]`** | `h1`〜`h4` の最終行 `h4.txt` にカーソル → **`d`**（確認ダイアログは出ない）→ 一覧は `3 items`、カーソルは **`h3.txt`**、プレビューは `bin marker 3`。ディスクは `h1` / `h2` / `h3`。**ごみ箱に `h4.txt` が居る**（`Shell.Application` の Namespace(10)、`from=C:\Users\yuu06\AppData\Local\Temp\filer-scratch\s28`、`deleted=2026/10/01 4:18`）。pid 同じ |
| 28.7 | **`[x]`** | `g1`〜`g4` の `g2.txt` にカーソル（`--keys j`）→ 外から `g2.txt` → `g9.txt` に改名 → カーソルは **`g3.txt`**。行の言う「その場に留まる」ほう（行番号 2 のまま）で、名前には追従しない。pid 同じ、落ちない |

**この 7 行の読みは、すべて「キーを送ったあと」に取っている。**そこが次の「見つけたこと 1」で、
行の期待そのものは満たされているが、**満たされる時刻が「次に何か触ったとき」になっている。**

### 見つけたこと 1（バグ）: 放置した窓には、外からの変更がいつまでも映らない

**`app.dirty` が「フレームを回し続ける理由」に入っていない。**外で足したり消したりしても、
**窓に触るまで一覧が古いまま**になる。28 節はまさにこの動きの節なので、7 行全部の土台。

測った値（`filer.exe` を `s28`（`d1`〜`d5`）で起動、`--keys G` で `d5.txt` にカーソル、
以後**キーを 1 つも送らない**）:

| 時刻 | `(Get-Process filer).CPU` | 窓（`PrintWindow`） | ディスク |
| --- | --- | --- | --- |
| 起動 +6 s | 0.531 | `5 items`、`d5.txt` にカーソル | `d1`〜`d5` |
| +9 s（何もせず 3 秒） | 0.547（Δ **0.016**） | — | 同上 |
| `d5.txt` を外から削除して +6 s | 0.547（Δ **0.000**） | **`5 items`、`d5.txt` がまだ居る** | `d1`〜`d4` |
| `<Esc>` を 1 つ送って +2 s | 0.578（Δ 0.031） | **`4 items`、`d4.txt` にカーソル** | `d1`〜`d4` |

`<Esc>` はこの状態で他に何もしないキーで、**それだけで一覧が追いついた。**
つまりウォッチャのイベントは届いていて、**描き直しが予約されていないだけ**。

対照（トーストが出ていて窓が起きているとき）: `c c` でコピーのトーストを出し、
その直後に `e5.txt` を外から削除、**キーを 1 つも送らずに** 2.5 秒後の 1 枚を撮ると
**`4 items`、`e4.txt` にカーソル**。トーストが 6 秒間フレームを回すので追いつく。

読み:

- `app.rs` の `flush_dirty` は `t.elapsed() > Duration::from_millis(150)` の分だけ
  拾う。ウォッチャの `wake`（`ctx.request_repaint()`、`app.rs:1070`）で走るフレームは
  **`dirty` を入れた直後**なので、経過はまだ 0 ms。そこで 1 回だけ回って終わる。
- `main.rs:564` の「Keep the frame loop alive only while something is actually pending」
  が並べているのは `preview.pending_since` / `toasts` / `search` / 走っている `tasks` の
  4 つだけで、**`dirty` が入っていない。**だから 150 ms 後にフレームが来ない。
- 直すなら `flush_dirty` が拾い残したときに
  `ctx.request_repaint_after(150 ms)` を頼むか、`main.rs` のこの条件に
  `!self.app.dirty.is_empty()` を足す。**1 行**。
- **v0.54.2 より前は、たいてい隠れていたはず。**あの版が止めた「プレビューのデバウンス・
  タイマーが消えずに 16 ms ごとに再描画を頼む」状態だと、窓はずっと回っていたので
  150 ms 後のフレームが勝手に来る。47 節で CPU が 0 になったことと、この穴は表裏。

人が手で触るときは、エクスプローラーから filer に戻る操作そのものがフレームを作るので
気づきにくい。**効くのは、filer を見ているのに手を止めている時間**（別の窓でビルドを
回しながら出来上がりを待つ、`git pull` の結果を待つ、など）と、無人の run。

### 見つけたこと 2（観察、再現せず）: 空になった一覧で `h` が 2 段上がったことが 1 度ある

28.3 の 1 回目で、ウォッチャが空にした `s28` から `h` を 1 つ送ったら、タイトルが
`Filer: C:\Users\yuu06\AppData\Local\Temp`（**2 段上**）になり、カーソルは
`…\Temp\filer-scratch` に居た。同じ手順を 2 通りで追いかけたが再現しない:

- 最初から空のディレクトリで起動 → `h` 1 回 → 1 段（`…\filer-scratch`）。
- 中身をウォッチャに消させてから `h` 1 回 → 1 段（`…\filer-scratch`）。
- 中身のあるディレクトリで `h` 1 回 → 1 段、もう 1 回 → さらに 1 段。

`PostMessageW` の取りこぼし／二重配送かもしれないし、空の一覧での `leave` の何かかも
しれない。**バグとは言い切らない**が、次の run が同じものを見たときに気づけるよう
書き残す。

### 見つけたこと 3（TESTING.md の文言）: 28.6 に確認ダイアログは無い

28.6 は `d` を「押す」だけの行だが、実際の `d`（`remove`）は**確認を出さずに**
ごみ箱へ送る。行の期待（行が消える・カーソルが新しい最終行・落ちない）は満たすので
`[x]` にしてあるが、`<Enter>` が要ると思って手順を書くと詰まる。
**行に「確認は出ない」と足すか、そもそも確認を出すべきかは持ち主の判断**なので、
下の提案 2 に回した。

### Proposals

#### 1. 外からの変更は、触らなくても映ってほしい（見つけたこと 1 の裏返し）

- **何が起きたか**: 上のとおり。放置した窓は、外で消したファイルを 6 秒経っても
  出したままで、`<Esc>` を 1 つ送った瞬間に追いついた。
- **どう変えるか**: `main.rs:564` の「まだ用事があるか」の条件に
  `!self.app.dirty.is_empty()` を足す（`dirty` を `pub(crate)` にするか、
  `App` に `fn has_dirty(&self) -> bool` を生やす）。あるいは `flush_dirty` が
  拾い残したときに `ctx.request_repaint_after(150 ms)`。
- **なぜ**: ファイルマネージャの一覧が、**別の窓でやったことを映さない**のは
  一番効く種類の嘘で、しかも「古い」ことが画面から分からない。47 節で窓が本当に
  眠るようになったぶん、ここが効く時間が長くなっている。
- **大きさ**: 1 行。**ただしテストが要る**（47 節の「放置で CPU 0」と
  ぶつからないこと＝ `dirty` が空になったら止まること）。

#### 2. `d` が確認なしでごみ箱に送るのは、`D` との差が無さすぎる

- **何が起きたか**: 28.6 で `d` を 1 回押しただけで `h4.txt` が消えた。トーストも
  出ない（出たのは自分で押した `c c` のコピーのトーストだけ）。**押し間違いに
  気づく機会が 1 つも無い。**戻すにはごみ箱を開けるしかなく、filer の中に
  「今の削除を取り消す」は無い。
- **どう変えるか**: どちらか。(a) 削除したあとに「`h4.txt` をごみ箱へ」の
  トーストを出す（`c` のコピーと同じ扱い）。(b) 複数選択しているときだけ確認を出す。
  **(a) を推す** — 1 件の削除で毎回 `<Enter>` を要求するのは yazi 風の速さを殺す。
- **なぜ**: いま `d`（ごみ箱）と `D`（完全削除）は、押した直後の画面が**同じ**。
  どちらを押したか分からないまま次に進むことになる。トースト 1 つで、
  「ごみ箱に入った」＝戻せる、が読めるようになる。
- **大きさ**: トーストを 1 つ出すだけ。数行。

#### 3. `c` `c` が「何もコピーしなかった」ことを言わない

- **何が起きたか**: 空の一覧で `c` `c` を押すと、**何も起きない。**クリップボードは
  前の中身のまま、トーストも警告も出ない。この run では番兵を入れて区別したが、
  人は押したつもりで古いパスを貼ることになる。
- **どう変えるか**: 対象が無いときに「Nothing to copy」のトーストを出す。
  `c` の 4 つ（`path` / `dirname` / `filename` / `name_without_ext`）で同じ。
- **なぜ**: 空のディレクトリに降りた直後は、まさに「パスをコピーして端末に貼る」
  場面で、**前のディレクトリのパスが貼られると気づきにくい**（見た目が正しいパス）。
- **大きさ**: 空のときの分岐 1 つ。数行。

### 使った道具（再実行できるように）

作業用ディレクトリ `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM
ディスクは無い。`TEMP` / `TMP` も同じ場所）に 2 本置いた。

- `win28.ps1`: 窓探し（クラス `Window Class`、タイトルが `Filer:` で始まるもの）、
  `PostMessageW` でのキー送り、`PrintWindow` の 1 枚、入力デスクトップの確認、
  番兵付きの `Read-Cursor`。
- `shdel.ps1`: `SHFileOperation`（`FO_DELETE | FOF_ALLOWUNDO | FOF_NOCONFIRMATION |
  FOF_SILENT | FOF_NOERRORUI`）。複数パスは `pFrom` に NUL 区切りで並べて 1 回。

肝は 3 つだけで、あとは定型:

```powershell
# 1. 入力デスクトップ。GetUserObjectInformationW は CharSet=Unicode を明示しないと
#    UTF-16 の文字列を ANSI として読んで "Default" が "D" になる（この run で踏んだ）。
[W28]::DesktopName() -eq 'Default' -and -not [W28]::SaverRunning()

# 2. キー。scan code を lParam に入れないと winit が落とす。
$vk = [W28]::VkKeyScanW($ch) -band 0xFF
$lp = [IntPtr](1 -bor ([W28]::MapVirtualKeyW($vk, 0) -shl 16))
[W28]::PostMessageW($h, 0x0100, [IntPtr]$vk, $lp)          # WM_KEYDOWN
[W28]::PostMessageW($h, 0x0102, [IntPtr][int]$ch, $lp)     # WM_CHAR
[W28]::PostMessageW($h, 0x0101, [IntPtr]$vk, $lp)          # WM_KEYUP

# 3. 読む前に、必ず番兵を入れて読み返す。
$s = 'none-' + [guid]::NewGuid().ToString('N').Substring(0,8)
Set-Clipboard -Value $s; Start-Sleep -Milliseconds 250
if ((Get-Clipboard) -ne $s) { throw 'clipboard would not arm' }
```

**「見つけたこと 1」があるので、外から変えたあとの読みは `c c` を 2 回、1 秒あけて
送ってから取っている**（1 回目がフレームを起こし、2 回目のときには 150 ms の
デバウンスが明けている）。これを入れるまで、2 秒待って 1 回読むやり方は
半分くらい古い値を返した。

### 後始末

- 起動した `filer.exe` は **17 本**、いずれもこの run の中で終わらせた。終了時の
  `Get-Process filer` は 0 本。
- 触ったのは `filer-scratch\s28`（毎回作り直して消した）と、ごみ箱に入った
  テスト用の `.txt`（`a01` / `b3` / `c1` / `f3` / `h4` などで、すべて
  `…\filer-scratch\s28` 由来）。**人の設定ファイルは 1 つも触っていない**
  （`$PROFILE` も `FILER_CONFIG_HOME` も、この節では不要だった）。
- スクリーンショットは `filer-scratch\shots\` に置いてあるが、**そこから読んだ文字は
  この節とプルリクエストの本文に写してある**。

---

## TESTING.md section 44 — ディスク使用量、ARM64 実機で確かめた（f56ce72 / 0.55.3、ARM64 レーン）

無人実行（`auto-wintest.ps1 -Lane arm`）。ARM64 レーンの順番表の先頭が 44 だったので、
13 行すべてを当たった。**12 行が通り、44.7 が落ちた。**44.7 は仕様上の穴で、この節で
一番価値のある結果なので先に書く。

`filer env` は `Process arch aarch64` / `OS arch aarch64`、版は 0.55.3。ConPTY は
`fetch-conpty.ps1` で `1.24.260710001 (arm64)` を `target\release` に置いた。
RAM ディスクは無いので scratch は `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`
（`TEMP` / `TMP` もそこ）。管理者ではない（この節には要らなかった）。

**`cargo test` は 523 passed / 0 failed**（ARM64 ネイティブ、0.55.3）。

### 測るための下ごしらえ

`gu` のビューは既定の設定では**棒しか出ない。**`linemode` が `None` のままで、
既定キーマップに `linemode usage` の束縛が無いからで、バイト数を読むには
`yazi.toml` に `linemode = "usage"` を手で書くしかない（見つけたこと 2）。
そこで run 専用の `FILER_CONFIG_HOME` を作り、

```toml
[mgr]
ratio = [1, 6, 3]
linemode = "usage"
```

だけを置いた。これで各行の合計が**読めるテキスト**になり、この節の判定は
ほとんどがスクリーンショットから読んだ数字と `Get-ChildItem -Recurse -Force |
Measure-Object Length -Sum` の突き合わせになっている。

フィクスチャは `fx44.ps1`（`s44\proj` / `hidden` / `links` / `keys` / `dest` / `hard`）と
`fxbig.ps1`（320 ディレクトリ × 1000 ファイル = 320,000 ファイル、81 秒）。
どちらも本文の末尾に貼ってある。

### 落ちた行

#### 44.7 `C:\` で `gu` — 上位のフォルダが妥当ではない

| 読んだもの | 値 |
| --- | --- |
| filer の `Windows` 行 | **`0 B`** |
| `Get-ChildItem C:\Windows -Recurse -Force -File` | **179,442 ファイル / 36.1 GB** |
| filer の合計（トースト） | `83 G in total (walk cut short; totals are floors)` |
| `(Get-PSDrive C).Used` | 149.7 GB |

`Windows` だけではない。`Recovery` / `System Volume Information` / `PerfLogs` /
`OneDriveTemp` / `inetpub` も全部 `0 B` で並んだ。上位に来たのは `dev` 21 G、
`Program Files (x86)` 20 G、`Users` 13 G、`hiberfil.sys` 13 G で、ここまでは妥当。
**ドライブで一番大きいフォルダが空として並ぶ**ので、「WizTree やエクスプローラーと
見比べて妥当」は成り立たない。

原因は `fs/usage.rs` の `BUDGET: usize = 200_000` が**全部の子で 1 つの予算を共有して
いる**こと。`read_dir` が返した順に使い切るので、先に当たった子が全部持っていき、
後ろの子は 1 エントリも歩かれずに 0 で確定する。`C:\Windows` 単体で 179,442 ファイル
あるから、ドライブ直下では予算が 1 つの子で尽きる勘定になる。

さらに悪いのは**並び順が大きい順だ**ということで、歩かれなかったフォルダは
必ず一番下に沈む。「どのフォルダが容量を食っているか」を答えるためのビューで、
歩かれなかった＝答えかもしれないフォルダが、いちばん目に入らない場所に置かれる。

「下限値である」とは言っている（トーストは正しい）。ただし**トーストは数秒で消え、
そのあとは画面のどこにも下限だと書いていない。**25 秒後のスクリーンショットは
`Windows 0 B` とだけ出ている。提案 2 を参照。

同じことは合成したツリーでも読めた（44.3 の 320,000 ファイル）。`d0000`〜`d0198`
あたりが `16 K`、`d0200` から下は全部 `0 B`、合計は `3.0 M`（実際は 4.9 M）。
プレビューには `d0319` の中身（`f0000.bin` …）がちゃんと出ているので、
**中身のあるフォルダが 0 B と表示されている**ことがその場で読める。

### 通った行（ARM64 での結果と根拠）

| # | 結果 | 根拠 |
| --- | --- | --- |
| 44.1 | ○ | `s44\proj` で `gu` → `node_modules 5.7 M` / `target 1.5 M` / `.git 27 K` / `src 6.0 K` / `README.md 1.0 K` / `.gitignore 21 B` の順。`node_modules` 自身のエントリは 2 子で、合計 6,000,000 B とは桁が違う。棒はどの行にも引かれ、長さが数字どおり（`node_modules` が満杯、`target` が約 1/4、以下ほぼ空）。**棒が目立つかどうかは持ち主の判断**で、この行を支えているのは順序と数字 |
| 44.2 | ○ | カーソル: `--keys "jjcf"` → クリップボード `ccc`、`--keys "jjgu<Esc>cf"` → `ccc`。走査の停止: `big`（320,000 ファイル）の走査中 1.2 秒で `<Esc>`、`(Get-Process filer).CPU` は 1.797 のまま 10 秒動かず（完走させると 2.3〜2.4 CPU-s かかる測定済み） |
| 44.3 | ○ | 320,000 ファイルで `gu` → 3 秒で完了し、トーストが `3.0 M in total (walk cut short; totals are floors) — <Esc> to leave`。320 items すべて並ぶ |
| 44.4 | ○ | `proj\.gitignore` は `target/` と `node_modules/` を書いている（`git init` 済み）。どちらも一覧に出て `1.5 M` / `5.7 M` を持つ。合計 `7.3 M` = 7,634,443 B（`Get-ChildItem` と一致） |
| 44.5 | ○ | `s44\hidden\.cache` の属性は `Hidden, Directory`（`Get-Item -Force` で確認）。`gu` → `.cache 3.8 M` が 1 行目、`visible 98 K`、合計 `3.9 M` = 4,100,000 B。`show_hidden` は既定の off のまま |
| 44.6 | ○ | `s44\links\link` は `LinkTarget = …\links\real` のジャンクション。`gu` → 3 items で `real 2.9 M` / `tiny.txt 1.0 K` / `link -> 0 B`、合計 `2.9 M` = 3,001,000 B。**3 MB のツリーを二重に数えていない**。3 秒以内に完了（固まらない） |
| 44.8 | ○ | `\\192.168.0.150\Backup`（LAN の Samba）で `gu` → `Job DESKTOP-7UBRVG9 409 G`。`Get-ChildItem -Recurse -Force` は 438,769,771,777 B = 408.6 GiB で一致。走査中 1.5 秒で `<Esc>` → タイトルは `Filer: \\192.168.0.150\Backup\` に戻り、CPU はその後 10 秒で 0 上昇、`Get-Process filer` は生きている |
| 44.9 | ○ | `j`/`k`: `--keys "gujjkcf"` → `bbb`。選択: `--keys "gu<Space><Space>"` → ヘッダが `2 selected · 4 items`、カーソルは `3/4`。`y`: `--keys "guy<Esc>hkklp"` → `s44\dest\aaa\one.bin` が 30,000 B で出来た。`d`: `--keys "gud"` → 一覧が 3 items になり、ごみ箱に `aaa`（元の場所 `…\s44\keys`、削除 2026-10-01 5:22）が入っていた |
| 44.10 | ○ | 使用量ビューで `<Enter>` → タイトルが `…\s44\proj` から `…\s44\proj\node_modules` になり、普通の一覧（2 items: left-pad, react）になる。そこで `gu` → `react 4.1 M` / `left-pad 1.6 M`、合計 `5.7 M` = 6,000,000 B（`Get-ChildItem` と一致） |
| 44.11 | ○ | `--keys "gugu"` → 赤いトーストで `Usage: leave this view first`。一覧はそのままで `<Esc> to leave` も出たまま |
| 44.12 | ○ | `--keys "gu,a"` → `Sort: alphabetical` で `node_modules` / `src` / `target` / `README.md`（ディレクトリ先・名前順）に変わる。`--keys "gu,a<Esc>gu"` → 大きい順に戻る（6 items、トーストが `×2`） |
| 44.13 | ○ | 丸め: `proj` 合計 `7.3 M` ↔ 7,634,443 B、`node_modules` `5.7 M` ↔ 6,000,000 B、共有 `409 G` ↔ 438,769,771,777 B。ハードリンク: 1 MiB のファイルに 3 本のハードリンクを張ったツリーで `gu` → `tree 4.0 M`。`fsutil hardlink list` は 4 つの名前が同じ実体を指すと出るので、**ディスクは 1 MiB しか使っていないのに 4.0 M と出る**＝文書どおり多めに出る |

### ほかに見つけたこと

#### 1. 予算切れのフォルダが `0 B` と表示される（44.7 の中身）

上に書いた。`0 B` は「空だった」と「歩いていない」を区別しない。`capped` は
`Msg::Done` で分かっているのに、どの行が歩かれなかったかは持っていない。

#### 2. 既定の設定では、使用量ビューに数字が出ない

`start_usage` は `linemode` を触らない。既定キーマップ（`src/config/defaults/keymap.toml`
の「Line mode」の節）には `m s` / `m t` / `m b` / `m p` / `m n` はあるが
**`linemode usage` の束縛が無い。**`Linemode::Usage` は `parse` にも
`serde` にもあるので、`yazi.toml` に書けば効く。つまり**設定ファイルを編集しないと
バイト数が一度も読めない。**しかも `linemode = "usage"` を常設すると、
普通の一覧でもフォルダの行が全部 `0 B` になる（`usage_bytes()` が `None` のため）。

#### 3. 並べ替え直すと、隠しフォルダの行が消える

`s44\proj` で `gu` → 6 items（`.git` と `.gitignore` を含む）。`,a` を押すと
**4 items になる**（`Sort: alphabetical`）。合計のトーストは `7.3 M` のままなので、
27 K ぶんを説明する行だけが無くなる。`show_hidden` が off のまま再構築されるからで、
44.5 が保証している「隠しフォルダも数に入り、表示もされる」が、並べ替え 1 回で崩れる。

#### 4. 使用量ビューの `,s`（サイズ順）は、測った合計で並ばない

`--keys "gu,s"` → `Sort: size (reverse)` と出て、`node_modules 5.7 M` /
`src 6.0 K` / `target 1.5 M` / `README.md 1.0 K` の順になる。`target` が `src` より
下にいるので、並んでいるのは**エントリ自身の長さ（ディレクトリは子の数）**であって
測った合計ではない。使用量ビューで「サイズ順」を押した人が期待するのは合計のほう。

#### 5. `<Esc>` で抜けても「Measuring…」のトーストが残る

走査は止まる（44.2 で測ったとおり）のに、`Measuring… <Esc> to leave` は出たまま
時間切れまで消えない。抜けた直後のスクリーンショット（`44_2-after-esc`）に写っている。
「まだ測っている」と読めてしまう。

#### 6. TESTING.md 44.12 の文言が 44.11 と食い違う

44.12 は「もう一度 `gu` すれば大きい順に戻る」と書いてあるが、44.11 が
「ビューを開いたまま `gu` は断られる」を定めている。実際、`<Esc>` を挟まないと
`gu` は `Usage: leave this view first` で断られる。**この run は
`gu` → `,a` → `<Esc>` → `gu` として通した。**行の文言に `<Esc>` を足すのが正しいと思うが、
番号と文言は報告に回す規則なので直していない。

#### 7. 【実機セッション向け】`PostMessage` で送った平文字は 2 回効く

この run で一番危なかったのはこれで、**役割定義（`windows-role.md`）の
「Prefer `PostMessage` to `SendInput` for keys」がそのままでは正しくない。**

同じキー列を 2 つの経路で送った結果:

| キー列 | `--keys`（プロセス内） | `PostMessage`（`WM_KEYDOWN` + `WM_CHAR` + `WM_KEYUP`） |
| --- | --- | --- |
| `cf` | `aaa` | `aaa` |
| `jcf` | **`bbb`** | **`ccc`** |
| `jjcf` | **`ccc`** | **`C:\…\s44\keys\ccc`**（`c` が `c` `c` になって「パスをコピー」が走った） |

`j` が 1 回で 2 行動き、`c` が `c` `c` の和音になる。**名前付きキー
（`<Esc>` / `<Enter>` / `<Space>`）は二重にならない** — `keys::from_egui` が
平文字のキーイベントを落とし、テキストだけを見る作りだから、二重になるのは
「テキストになるキー」だけ。

逃げ道は無かった:

- `WM_CHAR` だけを送る → **何も届かない**（4 パターンとも `<none>`）。
- `WM_KEYDOWN` + `WM_KEYUP` だけを送る（`VkKeyScanW` で正しい仮想キー、scan code も
  `lParam` に入れた）→ **何も届かない**。

直前の ARM64 run（#108, section 28）が同じ `WM_KEYDOWN` + `WM_CHAR` の手順を
報告しているが、あの run が送ったのは `c` `c` のように**二重にしても結果が変わらない**
キーだったので、表に出なかったのだと思う。

**この run の結論: 平文字は `--keys` で送る。`PostMessage` は
`<Esc>` / `<Enter>` のような名前付きキーと、`--keys` では間に合わない
（`settled()` を待てない）タイミング用に限る。**
実際この節では、44.2 の走査中 `<Esc>`、44.8 の走査中 `<Esc>`、44.10 の `<Enter>`、
44.3 の末尾への `G` 以外はすべて `--keys` で送っている。
最初に `PostMessage` で取った 44.2 のカーソルの結果（`aaa`、つまり「戻っていない」）は
**この二重入力による誤りで、`--keys` で取り直したら `ccc` で正しく戻っていた。**

なお `App::settled()` は `self.usage` を見ていないので、`--keys` は走査の完了を
待たない。`gu` の直後のキーは走査中に入る。この節の範囲では実害が無かった
（`proj` の走査は数ミリ秒）が、大きいツリーで `--keys` を使うときは注意が要る。

### Proposals

#### P1. `gu` の間だけ `linemode` を `usage` にする（見つけたこと 2）

- **何に当たったか**: 既定の設定で `gu` を押すと、44px の棒が並ぶだけで数字が 1 つも
  出ない。この節を測るために `FILER_CONFIG_HOME` を作って `linemode = "usage"` を
  書くまで、「どのフォルダが何バイトか」が読めなかった。
- **どう変えるか**: `start_usage` で `tab.linemode` を退避して `Usage` にし、
  ビューを抜けるとき（`<Esc>`、`<Enter>` での移動、`,` での並べ替え直し）に戻す。
  常設の `linemode = "usage"` を要らなくする。
- **なぜ**: 「どのフォルダが容量を食っているか」に棒だけで答えるのは、
  2 つの行を見比べる用にはなっても、**数字を報告する用にはならない**。
  常設の `linemode = "usage"` は普通の一覧でフォルダが全部 `0 B` になるので、
  代わりにならない。
- **大きさ**: 関数 1 つぶん（`Tab` に 1 フィールド）。

#### P2. 予算が尽きたフォルダを `0 B` と書かない（見つけたこと 1 / 44.7）

- **何に当たったか**: `C:\` で `gu` → `Windows` が `0 B`。実際は 36.1 GB。
  トーストが消えたあとは、画面のどこにも下限だと書いていない。
- **どう変えるか**: 3 つあって、どれか 1 つでも入れば 44.7 は通ると思う。
  1. `usage::Msg::Sized` に「この子は最後まで歩けたか」を足し、歩けなかった行は
     `0 B` ではなく `≥ 0 B` か `?` で出す。**0 と「測っていない」を区別する**のが本質。
  2. `capped` の間はヘッダかフッタに印を出し続ける（トーストは消える）。
  3. 予算を子ごとに割る（`BUDGET / 子の数` を下限付きで）。
     先に当たった子が全部持っていく今の形は、**`read_dir` の順番で答えが変わる**。
- **なぜ**: 大きい順に並べるビューで、歩かれなかったフォルダは必ず最下段に沈む。
  「容量を食っているのはどれか」を聞いた人に、**答えがいちばん見えない場所に置かれる。**
  ドライブ直下は、この機能をいちばん使いたい場所でもある。
- **大きさ**: 1 と 2 は関数 1 つぶん。3 は設計の判断（持ち主に聞くことかもしれない）。

#### P3. 使用量ビューでは、隠しの絞り込みを外したままにする（見つけたこと 3）

- **何に当たったか**: `gu` の 6 行が `,a` を押すと 4 行になる。
- **どう変えるか**: 使用量ビュー（`in_search_view()` が真で `usage_max > 0` の間）は
  `rebuild` で `show_hidden` を無視する。
- **なぜ**: 走査は隠しフォルダを数えている（44.5 はそれを保証している）のに、
  行だけ消えると合計と行の和が合わなくなる。**`.cache` や `.git` が容量を食っている
  というのは、この機能で一番よくある答え**でもある。
- **大きさ**: 1 行〜数行。

#### P4. 使用量ビューの「サイズ順」は測った合計で並べる（見つけたこと 4）

- **何に当たったか**: `gu` のあと `,s` を押すと `target 1.5 M` が `src 6.0 K` より
  下に来る。
- **どう変えるか**: 使用量ビューにいる間は `SortBy::Size` を `usage_bytes()` で
  比べる（`,S` は逆順）。
- **なぜ**: 使用量ビューで「サイズ順」を押すのは、**今見ている数字で並べ替えたい**とき。
  別の量で並ぶのは、押した人には壊れて見える。`,a` や `,n` は意味が変わらないので
  そのままでよい。
- **大きさ**: 数行。

#### P5. `<Esc>` で抜けたら「Measuring…」を消す（見つけたこと 5）

- **何に当たったか**: 44.2 と 44.8 で走査中に `<Esc>` を押したあと、
  `Measuring… <Esc> to leave` が出たまま残った。走査は止まっている。
- **どう変えるか**: ビューを抜けるときに、そのトーストを取り下げる（か、
  `Stopped` に差し替える）。
- **なぜ**: 出ているトーストが唯一の進行表示なので、**残っていると
  「まだ動いている」と読める。**ネットワーク共有のように時間のかかる走査では、
  抜けたのか抜けられなかったのか分からなくなる。
- **大きさ**: 1 行。

#### P6. `windows-role.md` の「Prefer `PostMessage`」に但し書きを足す（見つけたこと 7）

- **何に当たったか**: `PostMessage` で送った `j` が 2 行動き、44.2 を一度
  「落ちた」と読み違えた。`--keys` で取り直すまで気づかなかった。
- **どう変えるか**: 「Prefer `PostMessage` to `SendInput` for keys」の節に、
  **平文字は二重になるので `--keys` を使う**ことと、
  `PostMessage` は名前付きキー専用だと書く。`WM_CHAR` 単独も
  `WM_KEYDOWN` 単独も届かないことも（両方試した人が次にまた試さないように）。
- **なぜ**: 無人の run は人が見ていない。**二重に効くキーは「動いている画面」を
  作るので、この文書がいちばん警戒している取り違えそのものになる。**
- **大きさ**: Markdown 数行。

### 後始末と、証拠の置き場所

- 起動した `filer.exe` は 30 本ほど、すべてこの run の中で `Stop-Process` した。
  終了時の `Get-Process filer` は 0 本。
- 触ったのは `filer-scratch` の下だけ（`s44` / `big` / `cfg` / `shots`）と、
  ごみ箱に入った `s44\keys\aaa`。**人の設定ファイルは 1 つも触っていない**
  （`$PROFILE` も `%APPDATA%\yazi` も `%APPDATA%\filer` も読んだだけ。
  この節の設定は run 専用の `FILER_CONFIG_HOME` に置いた）。
- スクリーンショットは `filer-scratch\shots\` にあるが、**そこから読んだ文字は
  この節とプルリクエストの本文に写してある。**

フィクスチャを作り直す手順（`fx44.ps1` の要点）:

```powershell
# バイト数を正確に決めたいので WriteAllBytes で作る。
Fill "$p\node_modules\left-pad\index.js"          200000
Fill "$p\node_modules\left-pad\dist\bundle.js"   1500000
Fill "$p\node_modules\react\cjs\react.dev.js"    2400000
Fill "$p\node_modules\react\deep\a\b\c\blob.bin" 1900000
Fill "$p\target\debug\app.exe"                    900000
Fill "$p\target\debug\deps\lib.rlib"              700000
Fill "$p\src\main.rs" 4096; Fill "$p\src\lib.rs" 2048; Fill "$p\README.md" 1024
Set-Content "$p\.gitignore" -Value "target/`nnode_modules/" -NoNewline
git -C $p init -q

(Get-Item "$h\.cache").Attributes = 'Directory, Hidden'          # 44.5
New-Item -ItemType Junction -Path "$l\link" -Target "$l\real"    # 44.6
New-Item -ItemType HardLink -Path "$H\tree\copy1.bin" -Target "$H\tree\original.bin"  # 44.13

# 44.3 / 44.2 用（予算 200,000 を超える）。81 秒かかる。
for ($d = 0; $d -lt 320; $d++) { for ($i = 0; $i -lt 1000; $i++) { … } }   # 320,000 ファイル
```

## TESTING.md section 47 — 放置した窓の CPU を 0.55.4 で測り直した（01a2b0e / 0.55.4、ARM64 レーン、無人の run）

ARM64 レーンの 18 本目（`.claude/windows-role.md`「The ARM64 lane」、`auto-wintest.ps1 -Lane arm`、
無人実行）。順番表の先頭「**47. an idle window uses no CPU, again**」。47.1〜47.3 は #103 が
0.54.12 で `[x]` にした行で、この run は**順番表が求めた測り直し**——0.55.0 がペインの入力と
ホイールを作り替えたあと、まだ一度も測っていなかった。

**結果は 0.54.12 と同じ。**47.1〜47.3 のどの測り方でも 10 秒の窓の差は **0〜0.0156 CPU-s**
（`Get-Process` の粒度 15.6 ms と同じか、その 1 つ分）で、#86 が測った 1.0 CPU-s/s は
**この機械のどの手順でも再現しない**。47.4 は条件（「47.1〜47.3 でまだ増えるとき」）が
成り立たないので `[ ]` のまま。**チェックの追加は無い**（3 行はすでに `[x]`、47.4 は条件未成立）。
`make-testcheck -- --check` は `in sync`（**215 / 405**）、`make-keycheck -- --check` も
`in sync`（243 / 248）。`cargo test` は **523 passed; 0 failed**（ネイティブ ARM64、debug 2.63 s）。

測り直しのついでに 0.55.x の変更点（ペイン・ホイール・プロンプト）も同じ物差しで測り、
**ホイールがファイル一覧をスクロールできない**ことと、**`--keys` の素のスペースがペインを
抜け出させる**ことを見つけた。見つけたもの 4 件、提案 3 件。

### この機械

| | |
| --- | --- |
| 機械 | Snapdragon X2 Elite (X2E88100, Oryon)、Adreno X2-90（Vulkan, IntegratedGpu）、窓 1360x860 |
| OS | `Windows 11 Home 26H1 (build 28000.2956)`、`PROCESSOR_ARCHITECTURE=ARM64` |
| filer | 手元ビルド 0.55.4 release、`filer env` が `OS arch aarch64` / `Process arch aarch64` / `Debug false` |
| ConPTY | `scripts/fetch-conpty.ps1` で 1.24.260710001 (arm64) を `target\release` へ |
| シェル | ペインの既定が `pwsh` 7.6.6 になっている（v0.55.0 の変更。starship のプロンプト付き） |
| 昇格 | **無し**（`IsInRole('Administrators')` = False。この節は昇格を要らなかった） |
| 一時ディレクトリ | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（この機械に RAM ディスクは無い） |
| 入力デスクトップ | 測った全区間で `Default` / `ScreenSaver False` / `LogonUI 0`（前後で確認） |

生の証拠は `C:\dev\filer-evidence\arm-47b\`（`out-step*.txt` に**この run の読みが全部**、
足場の `s47.ps1` / `shot2.ps1` / `wake.ps1` / `desk.ps1`、`env47.txt`、PTY ログ 4 本、
スクリーンショット 16 枚）。

### 測り方 — 0 を測定値にする

期待値がゼロの節なので、**陽性対照を先に取る**のは #103 と同じ。同じプロセス・同じ
`Measure-Cpu` で 100 ms おきに `j` / `k` を 100 回投げると

```
control = 1.1719 CPU-s / 11.54 s = 0.1015 CPU-s/s   （t47a）
control = 1.0781 CPU-s / 11.11 s = 0.0970 CPU-s/s   （fx）
```

で、手を離すと同じ窓が 3 秒後には 0 に戻る。#86 の 1.0 CPU-s/s の **10 分の 1** の活動が
見えているので、以下の 0 は盲点ではない。

キーは `PostMessageW`（`lParam = 1 | (MapVirtualKey(vk,0) << 16)`）。**1 回の投函が 1 回の
押下になることを 0.55.4 で確かめ直した**（#109 が `c` の二重発火を見ているため）:
ディレクトリだけの木 `rows47` で `filer <dir> --keys "j<Enter>"` と「`j` を 1 投函 + `<Enter>`」が
どちらもタイトルを `...\rows47\r2` にした。カーソルの居場所は**窓のタイトル**で読む。

### 行ごと

| # | 結果 | 根拠（10 秒の窓、CPU-s） |
| --- | --- | --- |
| 47.1 | 期待どおり（すでに `[x]`） | `t47a`（サブフォルダ 2・ファイル 4）で開いて 10 秒放置 → **0 / 0**。`fx`（15 項目、プレビューあり）で **0 / 0**。どちらも陽性対照 0.1 CPU-s/s の同じ窓で測っている |
| 47.2 | 期待どおり（すでに `[x]`） | 既定設定で `j`（a.txt）→ `k`（dir2）の対、間隔 **22.67 ms**（`Stopwatch`）: 前 **0**、後 **0 / 0 / 0**。投函を続けて撃った間隔 **0.45 ms** の版でも 後 **0.0156 / 0.0156 / 0**。`<Enter>` でタイトルが `...\t47a\dir2` になり両方のキーが届いたことを確認。**行の文言どおりの `j` → `j`** も `sort_dir_first = false` の run 専用設定で踏んだ（`t47b`、間隔 22.25 ms）: 前 **0.0156**、後 **0.0156 / 0.0156 / 0**、`<Enter>` で `...\t47b\czdir` |
| 47.3 | 期待どおり（すでに `[x]`） | 47.2 の対（間隔 **23.37 ms**）のあと `ShowWindow(SW_MINIMIZE)`。`IsIconic` は測る前も後も True。**0 / 0 / 0.0156**。復元して `<Enter>` → `...\t47a\dir2` |
| 47.4 | `[ ]` のまま | **条件が成り立たない**（47.1〜47.3 のどれも増えていない）。読みは取った: `fx` の上位 3 スレッドの `TotalProcessorTime` を 10 秒あけて 2 回読み、`2068 0.5000000` / `13188 0.0312500` / `24116 0.0156250` が**両方の読みで 1 ティックも動かなかった**（プロセスの CPU も 0.5625 のまま） |

#103 の 0.0156 CPU-s は「1 ティック」なので、**0 と 0.0156 の差は測定の分解能**であって
プログラムの差ではない。3 行とも #86 の 1.0 CPU-s/s から 2 桁以上離れている。

### 節の行ではないが、0.55.x のために測ったもの

順番表が測り直しを求めた理由（「0.55.x changed the pane's input and the wheel」）に
そのまま答える形で、47.1 と同じ物差しを当てた。

| 何を | 10 秒の窓 | 補足 |
| --- | --- | --- |
| **ペインを開いた窓**（`--keys "<C-t>"`、pwsh 7.6.6 が prompt で待っている） | **0 / 0 / 0** | 子プロセスが `OpenConsole.exe --headless` と `pwsh` で、PTY ログに `\e[?9001h`（win32-input-mode）まで出ている状態。**ペインを開いたまま放置しても増えない** |
| ホイールをファイル一覧の上で 8 / 30 ノッチ回したあと | **0 / 0 / 0** | 見つけたもの 1 のとおり一覧は動かないが、**回したこと自体で描き続けるようにはならない** |
| ホイールをペインの上で 6 ノッチ回したあと | **0 / 0 / 0** | |
| ホイールをプレビュー（`long.rs`、4000 行）の上で 6 ノッチ | 測定なし | スクロールは**する**（見つけたもの 1 の対照） |
| **プロンプトを開いたまま**の窓（`--keys "f"`） | **0.2969 / 0.0938 / 0.1406** | #103 の見つけたもの 3 が 0.55.4 でもそのまま |
| **選択付きで開くプロンプト**（`--keys "jr"`、v0.55.0 の変更） | **0.2812 / 0.3125 / 0.2344** | 空のプロンプトより安定して高い。同じ窓で何も開いていないときは **0 / 0.0156 / 0** |

### 見つけたもの

#### 1. ホイールがファイル一覧をスクロールできない（`scrolloff` が毎フレーム打ち消す）

**500 件のフォルダでホイールを 30 ノッチ下に回しても、一覧の表示は 1 行も動かない。**
カーソルが 2 行進んで（`1/500` → `3/500`）、そこで止まる。上に回すと**何も起きない**
（スクリーンショットのハッシュが 1 ビットも変わらない）。

証拠（`out-step7-wheel2.txt` / `out-step8-scrolloff.txt`、`so-b*.png` / `so-c*.png`）:

| 条件 | ノッチ | 結果 |
| --- | --- | --- |
| 既定（`scrolloff = 5`） | 下 8 | 先頭行 `item-001` のまま、カーソル `2/500` |
| 既定（`scrolloff = 5`） | 下 30 | 先頭行 `item-001` のまま、カーソル `3/500` |
| 既定（`scrolloff = 5`） | 上 8（下 8 のあと） | **画面が 1 ピクセルも変わらない**（PNG のハッシュが同じ） |
| `scrolloff = 0`（run 専用の設定） | 下 10 | 先頭行が **`item-027`**、カーソル `27/500`。1 ノッチ ≒ 2.6 行 |
| プレビュー（`long.rs`）の上 | 下 6 | 先頭行が `fn block_2()` から `let value_16 …` へ。**ホイールの経路自体は正しく働いている** |

原因は読める範囲では `src/ui/mod.rs`:

- 627 行 `app.tabs[idx].current.clamp_offset(rows, scrolloff)` を**毎フレーム、描く前に**呼ぶ。
- 672〜676 行 ホイールは**描いたあと** `Folder::scroll(delta, rows)` を呼び、`offset` を進めてから
  カーソルを可視範囲へ clamp する（`src/core/folder.rs:187`）。
- 次のフレームの `clamp_offset` が `cursor < offset + scrolloff` を見て `offset = cursor - scrolloff`
  に引き戻す（`folder.rs:178`）。カーソルは `offset` の縁までしか引きずられないので、
  **2 つは不動点になる。**`scrolloff`（既定 5）より小さいスクロールは毎回打ち消される。

`git log -S` で見るかぎり `clamp_offset` の呼び出しは初回コミットから、一覧のホイールは
**v0.26.5**（`d1920e5`）から同じ形なので、**0.55.x の退行ではない**。TESTING.md の
**19.3**（「ファイル一覧の上でホイール → 同じ〔1 ノッチずつスクロールする〕」）は
`[ ]` のままなので、これはその行が初めて踏まれた結果にあたる。19 節はこの run の担当外なので
チェックは付けない。

キーボードの `j` / `k` / `<C-f>` は `arrow` を通るので影響を受けない。**壊れているのは
ホイールだけ**で、マウスで一覧を送る手段が事実上無い。

#### 2. `--keys` の素のスペースは、ペインの外へキーを弾き出す

`filer <dir> --keys "<C-t>echo hi<Enter>"` で、シェルに届いたのは `e c h o` **だけ**
（`FILER_PTY_LOG`）。スペースも `h` も `i` も `<Enter>` も届かず、**一覧のほうが
`C:\dev\filer-armtest`——ペインのシェルが居るディレクトリ——へ飛んでいた。**

切り詰めて撮った（`out-step12-spaceroute.txt`、`pty-route*.log`）:

| `--keys` | シェルに届いた | 窓のタイトル |
| --- | --- | --- |
| `<C-t>echo` | `echo` | `…\t47a`（動かない） |
| `<C-t>echo ` | `echo` | `…\t47a`（**スペースは単に消える**） |
| `<C-t>echo h` | `echo` | `C:\dev\filer-armtest` |
| `<C-t>echo i` | `echo` | `C:\dev\filer-armtest` |
| `<C-t>ab cd` | `ab` | `C:\dev\filer-armtest` |
| `<C-t>echo<Space>hi<Enter>` | `echo hi` + `\e[13;28;13;1;0;1_` | `…\t47a`（**正しく動く**） |

つまり **`<Space>` と書けば通り、素のスペースだと (a) シェルに届かず (b) その後ろの
キーが全部一覧に行く。**飛び先がシェルの cwd なのは「ペインから抜けると一覧がシェルに
追いつく」挙動（29 節）と一致するので、**スペースがペインのフォーカスを落としている**のだと
思う——ただしそこまでは推測で、確かめたのは上の表の観測だけ。

`src/keyscript.rs:20` の `parse` は 1 文字 = 1 キーなので、スペースは `Key::parse(" ")` を
通って `Event::Text(" ")` になる（`events()` の 44〜47 行）。文字が `Text` で入るのは
他の文字も同じなのに、スペースだけがペインに入らない。

**影響**: 実機セッションの足場がまさにこの形で書かれる。この run も
`--keys "<C-t>Get-ChildItem C:\Windows\System32 -Name<Enter>"` でペインに打ち込んだつもりが、
シェルには `Get-ChildItem` だけが入り、残りが一覧を `C:\Windows\System32`（4501 項目）へ
歩かせていた。**黙って別のことをする**ので、気づかないまま測定を続けられる。

#### 3. プロンプトを開いたままの窓は、0.55.4 でも 0 ではない（#103 の見つけたもの 3 の追試）

上の表のとおり、`f` のプロンプトで **0.09〜0.30 CPU-s / 10 秒**、v0.55.0 で選択付きに
なった `r` のプロンプトで **0.23〜0.31 CPU-s / 10 秒**。何も開いていない同じ窓は 0。
`r` のほうが安定して高いのは、選択の描画かキャレットの点滅が毎フレーム再描画を
頼んでいるからだと思う。#86 の 1.0 CPU-s/s とは 1 桁半違うので**節の合否は動かない**。

#### 4. 前の run が残したダイアログが画面に居座っていた（この run の外）

最初のスクリーンショットに `Windows Script Host` のエラーダイアログが写っていた。
`wscript.exe C:\Users\yuu06\AppData\Local\Temp\filer-scratch\runenv.vbs`、作成 **2026/09/30 14:36**——
前日の無人 run の置き土産で、16 時間ほど前面に居た（スクリプト本体はもう無い）。
この run で `Stop-Process` して片付けた。#103 の提案 4 が言っていたのと同じ事象が
実際に起きているので、証拠として書いておく。

### Proposals

#### 1. ホイールで一覧を送れるようにする（`clamp_offset` と `scroll` の順番）

**何に当たったか**: 上の見つけたもの 1。500 件のフォルダで、マウスだけで一覧を下へ送る
方法が無い。ホイールを 30 回回しても先頭行が `item-001` から動かないので、
「ホイールが効かないアプリ」に見える。

**どう変えるべきか**: ホイールのスクロールを `clamp_offset` の**前**に適用するか、
`Folder::scroll` がカーソルを `offset + scrolloff` へ寄せる（縁ではなく `scrolloff` の
内側へ置く）かのどちらか。後者なら `scroll` の最後の 1 行

```rust
self.cursor = self.cursor.clamp(lo, hi);
```

を `pad` を見込んだ範囲に変えるだけで、呼び出し側の順番を触らずに済む。

**なぜ**: yazi も含め、一覧のホイールは「カーソルを置いたまま視界を送る」ものとして
期待される。いまは `scrolloff` を 0 にした人だけが正しい挙動を得ている。

**大きさ**: 関数 1 つ。`core::folder` に `scroll` と `clamp_offset` を続けて呼ぶテストを
足せば、UI 抜きで固定できる。

#### 2. `--keys` は素のスペースを受け付けないほうがいい

**何に当たったか**: 見つけたもの 2。スペースが黙って消え、後続のキーが別の面
（一覧）に流れ込む。この run では測定 1 本が丸ごと無駄になった。

**どう変えるべきか**: どちらかに倒す。

1. **`parse` が素のスペースを拒む**（`filer: --keys: " " cannot be typed; write <Space>`）。
   `--keys` は「窓が開く前に言え」という設計（`main.rs:103`）なので、この形が筋が通っている。
2. スペースを `<Space>` と同じキーとして通す。ただし `[mgr]` の `<Space>` は
   `["toggle", "arrow 1"]` なので、一覧で押したときの意味が変わらないか要確認。

推奨は **1**。書くほうは `<Space>` と打てばよく、間違いが**起動時に**分かる。

**なぜ**: `--keys` は実機セッションのための機能（Q24）で、使うのはほぼ全部スクリプト。
黙って別の動きをするのが一番高くつく。

**大きさ**: 1 行 + テスト 1 本（`keyscript::tests` に `assert!(parse("a b").is_err())`）。

#### 3. 順番表と役割定義に「スペースは `<Space>`」を書く

**何に当たったか**: 同上。`.claude/windows-role.md` の「Unattended runs」には
「**Plain characters go through `--keys`, not `PostMessage`**」と書いてあるので、
次の run も同じ書き方でペインに打ち込み、同じところで転ぶ。

**どう変えるべきか**: その行に一文足す——「`--keys` のスクリプトで**スペースは `<Space>` と
書く**。素のスペースは消え、そのあとのキーはペインではなく一覧に行く（#110）」。
提案 2 が入れば消せる。

**なぜ**: いま実機の足場を書く全員が踏む。

**大きさ**: 1 行。

## TESTING.md section 24 — 扱いにくい名前（dc66742 / 0.55.5、ARM64 レーン）

ARM64 の Windows ノート PC（Windows 11 Home 26H1 build 28000.2956、`filer env` の
`Process arch aarch64`）で、無人 run として 24 節を通した。ビルドは
`target\release\filer.exe` 0.55.5（同梱 ConPTY 1.24.260710001 arm64 を
`scripts\fetch-conpty.ps1` で配置）。スクラッチは RAM ディスクが無い機械なので
`C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（`TEMP` / `TMP` もそこ）。
設定は `…\s24cfg\{yazi,filer}` に隔離して**既定のキーマップ**で回した。昇格なし
（`IsInRole('Administrators')` = False）。

`cargo test` はネイティブ ARM64 で **523 passed / 0 failed**（0.55.5）。

**チェックしたのは 24.4 と 24.5 の 2 行。**24.2 は下の理由で残した。

### 24.4 — `<A-t>` の引用は 3 シェルとも 1 語で届く（チェックした）

カーソルを `quote'in-name.txt` に置き（`--keys "ggjjjcf"` → クリップボードが
`quote'in-name.txt`）、`<C-t>` でペインを開いて代入の左辺を打ち、`<C-t>` で一覧に戻って
`<A-t>`、`<Enter>`。ペインが送った・受け取ったバイトは `FILER_PTY_LOG` で読んだ。

| `[term] shell` | ペインが実際に動かしたもの | `in key`（filer が打った引用） | シェルの答え |
| --- | --- | --- | --- |
| 既定（`pick_default_shell` が `pwsh` を選ぶ） | `pwsh.exe` 7.6.6 arm64（`Microsoft.PowerShell_7.6.6.0_arm64__8wekyb3d8bbwe`） | `'C:\…\awkward names\quote''in-name.txt'` | `$a.Count` → `1`、`Test-Path -LiteralPath $a` → `True` |
| `powershell` | `Windows PowerShell`（5.1） | 同じ `'…quote''in-name.txt'` | `$a.Count` → `1`、`Test-Path` → `True` |
| `cmd` | `C:\WINDOWS\SYSTEM32\cmd.exe` | `"C:\…\awkward names\quote'in-name.txt"` | `dir /b` が `quote'in-name.txt` を 1 行で返した |

- PowerShell 系は `'` を**二重化**（`quote''in-name.txt`）、cmd は `"…"` で囲んで `'` を
  そのまま残す。`Quoting::for_shell` の分岐どおりで、v0.47.34 が直した形が ARM64 の
  ConPTY でもそのまま通っている。
- 生ログは `…\filer-scratch\s24-pty-default.txt` / `s24-pty-ps51.txt` / `s24-pty-cmd.txt`。
- 補足: 既定シェルは `powershell`（5.1）ではなく `pwsh` だった。`terminal.rs:179` の
  `pick_default_shell` が「PowerShell 7 が入っていれば `pwsh`」なので仕様どおり。

### 24.5 — `d` → `u` で日本語名がそのまま戻る（チェックした）

`awkward names\` で `G`（最下行 = `ひらがなとカタカナ.txt`、`cf` のクリップボードが
`U+3072 U+3089 U+304C U+306A U+3068 U+30AB U+30BF U+30AB U+30CA U+002E U+0074 U+0078 U+0074`）、
`--keys "Gd<Enter>u"`。60 ms ごとにディレクトリを見ていた。

```
BEFORE exists=True hash=ACF10B94D77FD5E0C848AF65D22FF7338B36FC311BB264D8E45A10840E345691 len=33
     3 ms  exists=True
   543 ms  exists=False        ← d（ごみ箱へ）
   606 ms  exists=True         ← u（戻った）
AFTER  exists=True hash=ACF10B94D77FD5E0C848AF65D22FF7338B36FC311BB264D8E45A10840E345691 len=33
ごみ箱の件数 62 → 62（入って出た）
```

名前・長さ・SHA-256 が一致。ごみ箱を経由していることは別の使い捨てディレクトリで
切り分けた（`--keys "Gd<Enter>"` だけを流し、`u` を押さずに見た）:

```
after d: exists=False
IN BIN: name=[ひらがなとカタカナ.txt] from=[C:\…\filer-scratch\s24bin-071117]
```

ごみ箱の中でも名前は化けていない。この run で `元に戻す` して片付けた。

### 24.2 — 残した（行の文言と実装が食い違っているのは変わらない）

`ui::awkward_names::a_very_long_name_is_cut_down_to_its_column` は ARM64 でも緑だが、
この行が言う「**真ん中が**省略され、**拡張子が読める**」は実装がしていないこと
（末尾を `…` にする）で、前の節（v0.45.0 の変換）で報告済み。ARM64 で新しく分かった
ことは無いので、見た目の行としてではなく**未解決の食い違い**として残す。

163 文字の名前そのものは ARM64 でも問題なく扱えている: 下の見つけたもの 1 を回避して
本来の名前のファイルを置き、`Gk` `cf` でカーソル行の名前を取ると 163 文字が
そのまま返り（`-ceq` で一致）、ウィンドウも生きたままだった。

### 見つけたもの 1: `make-fixtures.ps1` が「非常に長い名前」を作っていない（24.2 の土台が無い）

- **再現**: `scripts\make-fixtures.ps1` を回して `awkward names\` を見る。
- **実測**: 本来 1 つのはずの `very-long-…-name.txt`（163 文字）が無く、代わりに
  **`very-`（5 文字）、`long-`×30 の 150 文字、`name.txt`** の**3 ファイル**ができる。
  `upper.txt` も作られない（下の見つけたもの 2）。
- **原因**: `scripts/make-fixtures.ps1:145`

  ```powershell
  'very-' + ('long-' * 30) + 'name.txt',
  ```

  PowerShell では `,`（カンマ演算子）が `+` より**強く**結合する。配列リテラルの中で
  この式は `(… , 'very-') + ('long-'*30) + ('name.txt', …)` と読まれ、1 つの文字列では
  なく 3 要素に割れる。手元で同じリテラルを評価すると要素数は 6 ではなく **8** になる。
- **影響**: 24.2 は**そもそも対象のファイルが無い状態で** 4 回以上の run に出されていた。
  誰も気づかなかったのは、自動テストのほうが Rust 側で名前を組み立てているため。
- **直し方（直していない）**: 式を丸ごと括る。

  ```powershell
  ('very-' + ('long-' * 30) + 'name.txt'),
  ```

### 見つけたもの 2: `UPPER.TXT` / `upper.txt` は既定の NTFS では 2 つにならない（24.3 の手順が成り立たない）

- **再現**: 同じく `make-fixtures.ps1` のあとの `awkward names\`。
- **実測**: `UPPER.TXT` だけがあり、`upper.txt` は無い。スクリプトの
  `if (-not (Test-Path -LiteralPath $p))` は Windows では**大文字小文字を区別しない**ので
  `UPPER.TXT` を見つけて 2 つめを作らない。仮にこのガードを外しても、既定の NTFS
  ディレクトリでは同名として上書きされる。
- **影響**: 24.3「`UPPER.TXT` と `upper.txt` → 両方出て、両方開ける」は、
  `fsutil file setCaseSensitiveInfo <dir> enable`（既定では昇格が要る）を踏まない限り
  **手では再現できない**。自動テスト側（`ui::awkward_names`）は 2 行をメモリ上で作るので
  通っている。
- **どうするべきか（判断は持ち主）**: フィクスチャ生成に
  `fsutil file setCaseSensitiveInfo` を足して本当に 2 つ作るか、24.3 の行に
  「大文字小文字を区別するディレクトリでのみ」と条件を書くか。**行の文言は触っていない。**

### Proposals

#### 1. `make-fixtures.ps1` に「作ったものを数えて確かめる」1 行を入れる

**何に当たったか**: 見つけたもの 1 と 2。フィクスチャが**黙って**違うものを作り、
それが何 run も気づかれずに残った。`awkward names\` は 6 ファイルのはずが、実際は
7 個（3 つに割れた長い名前 + 消えた `upper.txt`）だった。

**どう変えるべきか**: 各グループの最後に、期待する件数との突き合わせを 1 行足す。

```powershell
$want = 6
$got = (Get-ChildItem -LiteralPath $awkward -Force).Count
if ($got -ne $want) { Write-Warning "awkward names: $got files, expected $want" }
```

`Write-Host` の「できたよ」の行のすぐ隣に置けば、次に壊れたときその run で分かる。

**なぜ**: いま `make-fixtures.ps1` の出力は「作った**つもり**」を印刷するだけで、
ディスクを一度も見ていない。実機 run はこの出力を信じて先へ進む。

**大きさ**: グループあたり 2 行。全部で 20 行ほど。

#### 2. 24.2 の行を、実装が答えられる形に決め直す

**何に当たったか**: 24.2 は「真ん中で省略・拡張子が残る」と書いてあるが、実装は末尾を
`…` にする。v0.45.0 の報告から版が 10 個進んで、まだ誰も決めていない。この run でも
「見た目だから残す」ではなく「文言が実装と違うから残す」として素通りするしかなかった。

**どう変えるべきか**: どちらかに倒して、QUESTIONS.md ではなく TODO.md に落とす。

1. **実装を行に合わせる**: `src/ui/list.rs` の `name_job()` で、幅が足りないときは
   `stem` の途中を `…` に置き換えて拡張子を必ず残す。ファイル一覧の省略としてはこちらが
   普通で、`IMG_0001.jpg` と `IMG_0002.jpg` が見分けられる利点もある。
2. **行を実装に合わせる**:「列に収まるところで切られる」に書き換え、拡張子の話を落とす。

推奨は **1**。ファイルマネージャで拡張子が消えるのは実用上つらい。

**なぜ**: どちらでもよいが、**決まっていないこと**が一番高い。行が 1 つ、10 版にわたって
「毎回読んで、毎回残す」対象になっている。

**大きさ**: 1 なら関数 1 つ + `ui::awkward_names` のテスト 1 本の書き換え。2 なら 1 行。

#### 3. `<A-t>` がペインを開いていないときは、開いてから送ってほしい

**何に当たったか**: 24.4 の足場を組むとき、`<A-t>` を単独で押すと
「The terminal is not open」で何も起きない（`app.rs:4218`）。`--keys` のスクリプトは
`<C-t>` `<C-t>` を前に置いて、ペインを開いてから一覧へ焦点を戻す必要があった。
手で使うときも、パスをシェルに渡したい場面は「まだペインを開いていない」ことが多い。

**どう変えるべきか**: `term_send_paths` が `self.term` を `None` で見つけたら、
`terminal`（= `<C-t>` が走らせるもの）と同じ経路でペインを開き、シェルが立ち上がってから
送る。エラーを出すのは、ペインを開くこと自体が失敗したときだけにする。

**なぜ**: いまの「開いていないと叱られる」は、キーの意味が
「**選択中のパスをシェルへ渡す**」であることに対して余計な前提を足している。
`<C-t>` を先に押さなければならないと覚えるのは、このキーを使う人だけが払うコスト。

**大きさ**: 関数 1 つ。シェル起動は非同期なので、送るバイトを「開いたら流す」キューに
積む形になる（`Terminal::send` の呼び出しを 1 段遅らせる）ので、設計の判断が 1 つ要る。

---

## TESTING.md 44.14 / 31.9 — v0.56.0 が Q32・Q33 で入れたものを ARM64 で確かめた（a07e5c2 / 0.56.2、ARM64 レーン、無人の run）

ARM64 の Windows ノート PC（Windows 11 Home 26H1 build 28000.2956、`filer env` の
`Process arch aarch64`）で、無人 run として ARM64 の順番表の先頭「v0.56.0, Q32 and Q33」を
通した。ビルドは `target\release\filer.exe` 0.56.2（同梱 ConPTY 1.24.260710001 arm64 を
`scripts\fetch-conpty.ps1` で配置）。スクラッチは RAM ディスクが無い機械なので
`C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（`TEMP` / `TMP` もそこ）。設定は run ごとに
`…\arm-v056\runs\<tag>\{cfg-filer,cfg-yazi,state}` に隔離して**既定のキーマップ**で回した。
昇格なし（`IsInRole('Administrators')` = False）。スクリーンセーバーが入力デスクトップを
握っていた（`Desk::Name()` = `Screen-saver`）ので、キーは `--keys` と `PostMessage`、画面は
`PrintWindow(PW_RENDERFULLCONTENT)` だけで触っている（#88 以来の形）。

`cargo test` はネイティブ ARM64 で **525 passed / 0 failed**（0.56.2）。

**チェックしたのは 44.14 と 31.9 の 2 行**（順番表が挙げていた 2 行そのもの）。
証拠一式は `C:\dev\filer-evidence\arm-v056\`（`lib.ps1`、`fx56.ps1`、`step1-4414.ps1`、
`step2-4414-midwalk.ps1`、`step3-319.ps1`、`step4-inview-linemode.ps1`、`out-step*.txt`、
`shots\*.png`）。

### 44.14 — `gu` の間だけサイズになり、`<Esc>` で日付に戻る（チェックした）

`fx56.ps1` が作った木で測った。行の文言が例に挙げている数字がそのまま出るように、
中身の合計を合わせてある。

| 行 | 中身 | バイト |
| --- | --- | --- |
| `big\` | `blob.bin` | 1572864（= `1.5 M`） |
| `medium\` | `mid.bin` | 6144（= `6.0 K`） |
| `loose.txt` | — | 2048（= `2.0 K`） |
| `small\` | `tiny.bin` | 100（= `100 B`） |

mtime は `2026-03-01 10:01` … `2026-03-04 10:04` を 1 日ずつずらして入れた（日付と
サイズを取り違えようがない形にするため）。

| 読んだもの | 画像 | 右の列 |
| --- | --- | --- |
| `--keys "mt"`（`linemode mtime`） | `a1-mtime-list.png` | `2026-03-01 10:01` / `2026-03-02 10:02` / `2026-03-03 10:03` / `2026-03-04 10:04` |
| `--keys "mtgu"`（使用量ビュー） | `b1-usage.png` | `1.5 M` / `6.0 K` / `2.0 K` / `100 B`、大きい順、棒つき |
| 上の窓に `<Esc>` を post、417 ms 後 | `b2-after-esc.png` | 日付に戻り、カーソルも `big` に戻っている |

- トーストは `b1` の時点で **`1.5 M in total — <Esc> to leave` と `Measuring… <Esc> to leave` の
  2 つ**が並んでいた（下の見つけたもの 2）。`b2` では `Measuring…` だけが消え、合計のほうは
  残っている。トーストの寿命は 6 秒で、`b1` が起動から 1.5 秒・`b2` が 1.9 秒なので、
  消えたのは時間切れではなく `<Esc>` による。
- **歩いている最中の `<Esc>`** も別に測った（行の「すぐ `<Esc>`」はこちら）。`C:\dev` で
  `--keys "mtgu"`、900 ms 後の `c1-midwalk.png` ではトーストは `Measuring… <Esc> to leave`
  だけ、行は `4.5 G` / `1.5 K`。`<Esc>` を post して 322 ms 後の `c2-midwalk-esc.png` では
  `Measuring…` は消え、`C:\dev` の普通の一覧（`Filer` / `filer-armtest` / `filer-evidence` /
  `obsidian-notes`）が日付つきで戻っている。
- 歩きが止まったことも読んだ: `<Esc>` の後の CPU は **1.0625 → 1.0625 s（10 秒で 0）**。

### 31.9 — ホストの一覧ではサイズの列が空（チェックした）

`\192.168.0.150`（LAN 上の Samba、`net view` が `Backup` / `backup-user` / `cache` /
`VR_Video` の 4 つを返す）に `--keys "ms"`（`linemode size`）で入った。

| 読んだもの | 画像 | 右の列 |
| --- | --- | --- |
| ホストの一覧、`linemode size` | `d1-host-size.png` | 4 つの共有すべて**空** |
| `--keys "msjjj"` で 4 つの共有を順にホバー | `d2-host-hover.png` | 空のまま（カーソルは `VR_Video`、`4/4`） |
| 同じビルド・同じ `m s` を普通のフォルダで（対照） | `d4-control-size.png` | `big 1` / `medium 1` / `small 1`、`loose.txt 2.0 K` |

- **対照を取ったのは、空の列と「そもそも描かれていない列」を区別するため。**同じ
  `m s` が普通のフォルダでは子の数を出しているので、ホストの一覧の空欄は空欄である。
- #105（0.54.14）では同じサーバで `Backup 1` / `backup-user 3` / `cache 10` と出ていた。
  Q32 の答え（`App::ensure_dir_sizes` が `util::host_only_unc` で早帰りする）が、実機でも
  効いている。
- 「そこで数え続けてはいけない」も読んだ: 一覧を出したまま放置して CPU は
  **0.234375 → 0.234375 s（10 秒で 0）**。

### 見つけたもの 1: 44.14 の行が言う `m m` は割り当てが無い（`m t`）

TESTING.md 44.14（と TESTING-CHECKS.md の訳）は「タブを `linemode mtime`（`m m`）にして」と
書いているが、既定のキーマップに `m m` は**無い**。`linemode mtime` は `m t`。

- 根拠: `src/config/defaults/keymap.toml` の `m` 配下は `s` / `t` / `b` / `p` / `n` の 5 つだけ
  （`out-step4.txt` に列挙を残した）。README 205 行も `m`+`t` と書いている。
  `cargo run --example make-keycheck -- --check` は `in sync`（243 / 248）で、
  TESTING-KEYS.md 側も `m t` と書いてある。
- どちらの間違いか: **TESTING.md の行**。`m m` は yazi の綴り（yazi は mtime に `m m` を当てる）
  なので、行がそこから来たのだと思う。
- この run は `m t` で通した。番号を動かさない修正なので、行の `m m` を `m t` に直すだけで済む。
  filer 側に `m m` を足すなら yazi 互換が増えるが、それはキーの割り当ての判断なので
  持ち主のもの。

### 見つけたもの 2: 歩き終わっても `Measuring…` のトーストが残る

使用量ビューの歩きが終わると `X in total — <Esc> to leave` が出るが、`Measuring… <Esc> to
leave` は消えず、画面に**同時に並ぶ**（`b1-usage.png`。合計が出ているのだから、もう
measuring ではない）。残りは寿命（6 秒）で消える。

- 根拠: `b1-usage.png`（起動から 1.5 秒、小さい木なので歩きは終わっている）。
- どこ: `App::drain_usage` の `if let Some((total, capped)) = done` の枝。
  `exit_search_view` は #109 の修正でここを `self.toasts.retain(|t| !t.text.starts_with("Measuring"))`
  しているが、歩きが自然に終わる側には同じ行が無い。
- #109 が直したのと**同じ食い違い**（「まだ測っている」と言い続ける）が、`<Esc>` ではなく
  完走の経路に残っている。1 行で揃う。
- 44.14 のチェックには影響しない（行が見ているのは `<Esc>` の後で、そこでは消えている）。

### Proposals

#### 1. `linemode usage` に既定のキーが無いので、ビューの中で列を変えると戻せない

**何に出くわしたか**: `--keys "mtgumt"`（`linemode mtime` → `gu` → ビューの中で `m t`）。
ビューの中で linemode を変えると、Q33 の答えどおりそのビューの間だけ効く——が、
`e1-inview-mtime.png` のとおり**棒だけが残って数字が消え**、サイズに戻すキーが無い。
`m` 配下は `s` / `t` / `b` / `p` / `n` の 5 つで、`usage` はどれでもない（`m s` を押すと
フォルダは子の数になり、使用量ではない）。ビューを `<Esc>` で出て `gu` で入り直すしかなく、
入り直すと歩きも最初からになる。

**どう変えるべきか**: 既定のキーマップに `m u` = `linemode usage` を足す。
`gu` が内部で使うモードに、人が戻れる入口を 1 つ付けるだけ。

**なぜ**: v0.56.0 で `usage` は「設定を書かないと選べない内部モード」から「`gu` で
日常的に入るモード」になった。入れるのに押せないモードが 1 つだけ残っているのは、
`m` の列から見ると穴に見える。コストを払うのは、ビューの中で `m` を押してしまった人。

**大きさ**: keymap.toml に 4 行（`[[mgr.keymap]]` / `on` / `run` / `desc`）。TESTING-KEYS.md が
生成なので数が 1 つ増える。キーの割り当てなので、持ち主の判断が要る（`m u` でよいか）。

#### 2. 歩いている間の「N items」が、測り終えた数であってフォルダの件数ではない

**何に出くわしたか**: `C:\dev` で `gu`。900 ms の時点の `c1-midwalk.png` は右上が
**`2 items`**、一覧も 2 行（`Filer` と `.claude`。隠しフォルダも数えるのは 44.5 のとおり）。
`<Esc>` 後の `c2-midwalk-esc.png` は `4 items`（`Filer` / `filer-armtest` / `filer-evidence` /
`obsidian-notes`）。歩きの途中では「このフォルダには 2 つある」と読める表示になっていて、
**まだ測り終えていないだけの数が、何の断りもなく件数として出ている**。大きい木ほど
長く嘘になる。

**どう変えるべきか**: 歩いている間はカウンタを件数として出さない——`2 items` ではなく
`2 measured` のように、まだ増えることが読める形にする。あるいは `Measuring…` の
トースト側に進捗を持たせ（`Measuring… 2 so far`）、カウンタは歩きの間だけ伏せる。

**なぜ**: いま画面には「終わったのか、まだ増えるのか」を言うものが `Measuring…` の
トーストしかなく、それは 6 秒で消える。消えたあとの使用量ビューは、歩き終わったものと
途中のものが見分けられない。`44.3`（30 万ファイルの木）のように数分かかる木で効く。

**大きさ**: 言葉を変えるだけなら関数 1 つ（`drain_usage` が受け取っている件数を、
歩きが終わるまで別の文言で出す）。`2 / 5` のような分母を出すなら、子の総数を歩きの
最初に数えて `Msg` に 1 つ足す必要があるので、`fs::usage` の設計に 1 つ判断が要る。

## TESTING-KEYS.md の残り 5 キー — ARM64 実機で押して、他に何も起きないことまで読んだ（08dc635 / 0.57.2、ARM64 レーン、無人の run）

ARM64 の Windows ノート PC（Windows 11 Home 26H1 build 28000.2956、`filer env` の
`Process arch aarch64`）で、無人 run として ARM64 の順番表の先頭「TESTING-KEYS.md, the 5
unchecked keys」を通した。ビルドは `target\release\filer.exe` 0.57.2（同梱 ConPTY
1.24.260710001 arm64 を `scripts\fetch-conpty.ps1` で配置）。スクラッチは RAM ディスクが
無い機械なので `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（`TEMP` / `TMP` もそこ）。
設定と状態は run ごとに `…\arm-keys\runs\<tag>\{cfg-filer,cfg-yazi,state}` に隔離し、
**既定のキーマップ**で回した（この機械の `keymap.toml` には `T` の割り当てがあるため）。
昇格なし（`IsInRole('Administrators')` = False）。スクリーンセーバーが入力デスクトップを
握っていた（`Desk::Name()` = `Screen-saver`、`SPI_GETSCREENSAVERRUNNING` = True）ので、
キーは **`--keys` だけ**、画面は `PrintWindow(PW_RENDERFULLCONTENT)` だけで触っている。

`cargo test` はネイティブ ARM64 で **538 passed / 0 failed**（0.57.2）。

**5 キーすべてにチェックを入れた。これで TESTING-KEYS.md は 248 / 248。**
`cargo run --example make-keycheck -- --check` は `in sync with src/config/defaults/keymap.toml
(248 / 248 checked)`。ファイルは再生成していない（`[ ]` → `[x]` と、生成器が数えなおす
見出しの数だけを手で直し、`--check` のバイト比較で同じになることを確かめた）。

証拠一式は `C:\dev\filer-evidence\arm-keys\`（`lib.ps1`、`fx.ps1`、`step1-spot-C.ps1`、
`step2-diff-z.ps1`、`step3-spot-enter.ps1`、`step4-term-max.ps1`、
`step5-term-configreload.ps1`、`probe-*.ps1`、`out-*.txt`、`runs\*-pty.log`、`shots\*.png`）。

**「他に何も起きていない」の読み方**は 5 キーとも同じ形にした。同じ前置きのキー列を
2 本流し、片方にだけ目的のキーを足して、窓のタイトル・クリップボード（毎回
`SENTINEL-<guid>` を入れてから押す）・作業ツリーの全ファイルの SHA-256・プロセスの生存を
突き合わせる。カーソルの位置は `c` `f` の結果で、ペインに渡ったかどうかは
`FILER_PTY_LOG` の `in key` 行の本数で読んだ。

### `[spot]` `C` — パネル全体をラベル付きでコピー（チェックした）

| 読んだもの | 結果 |
| --- | --- |
| `--keys "<Tab>"`（押さない側） | クリップボードは `SENTINEL-82ae…` のまま |
| `--keys "<Tab>C"` | 14 行。`File` / `Name<TAB>L` / `Path<TAB>…\keys\L` / `Kind<TAB>Directory` / … / `Files' size<TAB>13 B (13 bytes)` |
| 1 行目のバイト列 | `78,97,109,101,9,76`（`Name` と `L` の間は **9 = TAB**） |
| トースト | `Copied the spot panel: 11 rows`（`a2-spot-C.png`） |

- 他に何も: タイトル・ツリーのハッシュ・プロセスの生存はすべて同じ。`a1` と `a2` の
  キャプチャはトースト以外に 1 か所も違わない（ヘッダ `3 items`、下段 `1/3`、
  カーソルは `L`、パネルは開いたまま）。
- **パネル側のカーソルも動いていない**: `<Tab>c` が `L` を返し、`<Tab>Cc` も `L` を返す
  （`c` は「カーソルのセル」をコピーするので、`C` が行を動かしていたら別の値になる）。

### `[diff]` `z` — フォルダ比較の一致行を隠す / 戻す（チェックした）

`L` と `R` を `<Space><Space>` で選んで `<A-d>`。木は `same.txt`・`also.txt`（一致）、
`differ.txt`（4 B → 5 B）、`onlyleft.txt` / `onlyright.txt`（片側だけ）。

| 読んだもの | 画像 | 行とフッタ |
| --- | --- | --- |
| `<Space><Space><A-d>` | `b1-diff-open.png` | 5 行（`=` 2 つを含む）、`1 only left · 1 only right · 1 differ · 2 match` |
| 同じ + `z` | `b2-diff-z.png` | `=` の 2 行が消えて 3 行、フッタに **`· matches hidden (z)`** が付く。トースト `Hiding matching rows` |
| 同じ + `zz` | `b3-diff-zz.png` | `b1` と同じ 5 行・同じフッタ。トースト `Showing matching rows` |

- 他に何も: タイトル・クリップボード（sentinel のまま）・ツリー・生存すべて同じ。
  数（`1 / 1 / 1 / 2`）は隠しても変わらず、ヘッダの `2 selected · 3 items` と下段の `3/3`、
  カーソル（`differ.txt`）も動いていない。

### `[spot]` `<Enter>` — プルリクエストの行では開き、ほかではディレクトリに入る（チェックした）

説明の 2 つの半分を別々に読み、**隣の行では何も起きない**ことも取った。

| 読んだもの | 結果 |
| --- | --- |
| `--keys "<Tab>"`（押さない側） | タイトル `Filer: …\filer-scratch\keys` |
| `--keys "<Tab><Enter>"`（ディレクトリの行） | タイトル **`Filer: …\filer-scratch\keys\L`**。他は全部同じ |
| `<Tab>` + `<A-j>`×15（`Came in via  #115  580bd5e` の行）、`<Enter>` なし | `pull/` を含むブラウザのプロセスは **0** |
| 同じ + `<Enter>` | トースト **`Opened https://github.com/uchmk/filer/pull/115`**（`c4-spot-enter-pr.png`）。`Win32_Process` に `chrome.exe --single-argument https://github.com/uchmk/filer/pull/115` |
| `<A-j>`×16（`From branch` の行）+ `<Enter>` | ブラウザのプロセスは **1 つも増えない**、トーストも無し（`c5-spot-enter-frombranch.png`） |

- 対象は `C:\dev\filer-armtest\README.md`（`Came in via #115` / `From branch
  claude/task-09i0cs` / `Pull request https://github.com/uchmk/filer/pull/115`）。
- 他に何も: プルリクエストを開いた側でも、タイトル・クリップボード・ツリー・生存は
  押さない側と同じ。開いた窓はこの run が閉じ、URL を持つプロセスは 0 に戻した。
- パネルの行数と画面の行数が合わないように見えるが、これは**パネルが 18 行で切れていて
  カーソルに追従してスクロールする**ためで、`C` が 24 行を写すのと矛盾しない
  （`probe2-cursor-20.png` で `Text` の節までスクロールして確かめた）。

### `[mgr]` `<C-S-Enter>` — ペインに窓を渡す（チェックした）

**シェル自身のサイズ報告**で測った。`h.ps1` は `$Host.UI.RawUI.WindowSize` をファイルに
追記するだけのスクリプトで、どの run も前置きは同じ:
`<C-t>` → `h.ps1` のパスを打つ → `<Enter>` → `<C-t>`（ペインは開いたまま、キーは一覧に戻る）。

| run | 足したキー | シェルが報告した大きさ |
| --- | --- | --- |
| d1 | （なし） | `159x12` |
| d2 | `<C-S-Enter>` | `159x35` |
| d3 | `<C-S-Enter>` + もう一度報告 | `159x35` , `159x35` |
| d4 | `<C-S-Enter>` + 報告 + `<C-t>` `<C-t>` + 報告 | `159x35` , `159x12` , `159x12` |

- 窓は 899 px。12 行はペインが下から 1/3、35 行は窓いっぱい（`d1-pane-third.png` /
  `d2-pane-max.png`。d2 では一覧が消え、下段は `drw … 1/3` のまま）。
- `--keys` はシェルの起動より速いので、打った行が実行されるのは**キー列が全部入った
  後**になることがある。だから数字は「シェルが読みに来た時点のペインの大きさ」で、
  測っているのは**同じ前置きの 2 本の差**（12 行 対 35 行）である。
- 他に何も: タイトル・クリップボード・ツリー・生存すべて同じ。`in key` の行は
  **d1 も d2 も 54 本**（打ったパス 53 文字 + `<Enter>`）で、**このキーはシェルに
  1 バイトも送っていない**。カーソルも `c` `f` で両方 `L`。
- 「または戻す」の半分は `[mgr]` からは**押せない**。下の見つけたもの 2。

### `[term]` `<C-F5>` — 設定を読み直す（チェックした）

設定は起動時に 1 回読まれるので、**読み直さない限り知りようのない割り当て**を測りに
使った。run 専用の `FILER_CONFIG_HOME` は空で始め、**ペインのシェル自身**に
`mk.ps1` を走らせて `<F8>`（既定では未割り当て）= `cd …/keys/L` を書かせる。
前置きは `<C-t>` → `mk.ps1` のパス → `<Enter>` → `#` と `z`×199（書き込みとキーの間に
8 秒ほど置くためのコメント 1 行。`f2-at-9000ms.png` で、フィラーを打っている途中には
すでに `mk.ps1` が走り終わっていることを確かめた）。

| run | 足したキー | タイトル |
| --- | --- | --- |
| e1 | `<C-t>` `<F8>` | `Filer: …\filer-scratch\keys`（動かない） |
| e2 | **`<C-F5>`** `<C-t>` `<F8>` | **`Filer: …\filer-scratch\keys\L`**、トースト `Reloaded 1 config file(s)` |

- 他に何も（`<C-F5>` だけを差し替えた対）: e3（前置きのみ）と e4（前置き + `<C-F5>`）で
  タイトル・クリップボード・ツリー・生存はすべて同じ。`in key` は**どちらも 255 本**、
  `15;5~`（Ctrl+F5 の CSI）は**どちらも 0 件** — キーはシェルに渡っていない。
  カーソルも `c` `f` で両方 `L`。
- e2 だけ `in key` が 256 本になるが、256 本目は `cd C:\…\keys\L\r` で、これは `<F8>` が
  一覧を動かした後に `Terminal::follow` が送ったもの（`app.rs:4386`）。キーのせいではない。
- `e4-prefix-chord-at11000.png` のペインに `cd C:\…\keys\L` が見えるのは PSReadLine の
  履歴予測（`\e[97;2;3m` の薄い斜体）で、入力ではない。`in key` が 255 本のままなのが根拠。

### 見つけたもの 1: キーマップのコマンドに書いた Windows のパスは `\` が消える

`keymap.toml` に

```toml
[[mgr.prepend_keymap]]
on  = "<F8>"
run = 'cd C:\Users\yuu06\AppData\Local\Temp\filer-scratch\keys\L'
```

と書いて `<F8>` を押すと、`C:Usersyuu06AppDataLocalTempfiler-scratchkeysL: 指定された
パスが見つかりません。(os error 3)` というエラーが出る（`f1-F8-bound-at-start.png`）。

- どこ: `src/config/cmd.rs` の `lex()`。引用符の外では `'\\' => { 次の 1 文字を push }` で、
  **バックスラッシュが落ちて次の文字だけが残る**。POSIX のクォート規則としては正しいが、
  Windows のパスはこれで全滅する。
- 回避はできる: コマンド文字列の中で**シングルクォート**で囲む（`run = "cd 'C:\…'"`、
  `lex` は `'` の中では `\` を落とさない）か、**スラッシュで書く**
  （`run = 'cd C:/Users/…'`。この run はこちらで通した）。
- 何が悪いか: **エラーが出す名前が、書いた名前と違う**。`C:Usersyuu06…` を見た人は
  自分の打ち間違いを疑うので、クォートの話に辿り着けない。`cd` だけでなく
  `reveal` / `shell` など位置引数にパスを取るもの全部に効く。
- 直し方の候補は 2 つあり、どちらも設計の判断なので**提案ではなく報告**にしておく:
  (a) Windows では引用符の外の `\` をエスケープとして扱わない、
  (b) 落としたバックスラッシュがあったときだけ、エラーに「`'…'` で囲んでください」を足す。
- README にも「キーマップのパスはクォートするか `/` で書く」という記述は無い。

### 見つけたもの 2: `[mgr]` の `<C-S-Enter>` は「戻す」側を押せない

説明は「Give the terminal pane the window, or hand it back」だが、`[mgr]` の側から
**戻すことはできない**。`Act::MaxTerm` は最大化と同時に `term_focus = true` にし
（`app.rs:2586`）、ペインから出る唯一の道である `Act::Close` / `Act::Escape` が
`max_term = false` にする（`app.rs:4424`）。つまり **`max_term` が真のときキーは必ず
ペイン側にある**ので、`[mgr]` の `<C-S-Enter>` が押せるのは「渡す」方向だけ。

- これは `app.rs` のコメントが意図として書いているとおりの動作で、**不具合ではない**。
  ただし 1 つの説明文を 2 つの割り当てが共有しているので、`[mgr]` の行だけを読むと
  押せない半分が書いてあることになる。
- 「戻す」半分は `[term]` の `<C-S-Enter>`（すでにチェック済み）が持っている。この run の
  d4 でも、ペインから出る `<C-t>` で `159x35` → `159x12` に戻ることを読んでいる。
- チェックは入れた。**押せる方向については説明どおりで、他に何も起きていない**ため。

### Proposals

#### 1. `--keys` に「待つ」トークンが要る（ペインを使う行がことごとくこれで詰まる）

**何に出くわしたか**: `<C-S-Enter>` と `<C-F5>` の両方で、`--keys` が
`App::settled()` しか待たないせいで**ペインの中のシェルより速く打ち終わってしまう**。
`h.ps1` を打って `<Enter>` した直後に `<C-S-Enter>` を押すと、シェルが行を実行するのは
最大化の**後**で、報告された大きさは前後が入れ替わる。`<C-F5>` では
「シェルに設定ファイルを書かせてから読み直させる」という順番そのものが必要だったので、
`#` と `z` を 199 個打って 8 秒稼ぐという形になった（`step5-term-configreload.ps1`）。

**どう変えるべきか**: `--keys` の記法に待ちを 1 つ足す。`<Wait500>` のように
ミリ秒を書けるトークンが素直で、`keyscript::parse` が `Key` ではなく
`Step::{Key, Wait(Duration)}` を返す形になる。`raw_input_hook` は今も
「`settled()` かつ 2 フレーム後」で送っているので、そこに「かつ待ちが明けた」が
増えるだけ。

**なぜ**: ペインの中で起きることを外から測る行は、`[term]` だけで 16 行、TESTING.md の
section 1 がまるごとそれにあたる。いまはどの run も**フィラーのキーで時間を稼ぐ**という
同じ工夫を再発明していて（#110 の `<Space>` と同じ種類の落とし穴）、しかも
「何秒稼げたか」はキーの本数から推測するしかない。払っているのは実機のセッションで、
1 行あたり数回の試行になる。

**大きさ**: `keyscript.rs` の `parse` と `events` の戻り値、`main.rs` の
`raw_input_hook` の条件 1 つ、`--help` の 1 行。関数 2 つぶん。

#### 2. キーマップのコマンドでパスが壊れたとき、エラーがそう言うべき

**何に出くわしたか**: 見つけたもの 1 そのもの。`run = 'cd C:\Users\…\L'` を書いて
`<F8>` を押し、`C:Usersyuu06AppDataLocalTempfiler-scratchkeysL` が無いと言われた。
しばらく「読み直しが効いていない」方を疑って、`<F8>` を**起動時から割り当てた**対照を
取って初めて切り分けられた（`probe-f8.ps1`）。

**どう変えるべきか**: `lex()` が引用符の外でバックスラッシュを落としたことを覚えておき、
その引数を使うコマンドが `NotFound` で失敗したときに
`cd: C:Users… （書いたパスに \ が含まれています。'…' で囲むか / で書いてください）`
のように足す。あるいは Windows ではそもそも引用符の外の `\` をエスケープにしない。

**なぜ**: いまのエラーは**書いていない名前**を出すので、原因に辿り着く道がない。
これに当たるのは「yazi の設定を Windows に持ってきた人」全員で、しかも最初に試すのは
たいてい `cd` か `shell` でパスを渡す行である。

**大きさ**: (b) なら `lex` の戻り値に 1 つ（落としたかどうか）足して、エラー文を組む所で
使うだけ。(a) は `#[cfg(windows)]` で分岐する 3 行だが、**既存の設定の意味が変わる**ので
持ち主の判断が要る。

#### 3. spot の `From branch` でも `<Enter>` で開けてよい

**何に出くわしたか**: `<Enter>` の「何も起きない」側を取るために `From branch` の行で
押した（c5）。`Came in via` と `Pull request` は開き、その**間に挟まれた**
`From branch  claude/task-09i0cs` だけが無反応で、トーストも出ない。隣り合う 3 行のうち
真ん中だけが黙っているので、押した側には「キーが効いていない」と見える。

**どう変えるべきか**: `From branch` の行でも、その枝の GitHub のページ
（`…/tree/<branch>`、`git::pull_request_url` と同じ remote から作れる）を開く。
枝がもう消えていることはあるので、開けない場合は `Deleted branch` などのトーストを出す。

**なぜ**: 「どの枝から来たか」を見ている人が次に見たいのは、たいていその枝である。
いま同じ情報に行くには、`c` でコピーして自分でブラウザの URL を組むしかない。
行が既にあって値も正しいので、足りないのは入口だけ。

**大きさ**: `spot_pr_url` の兄弟として `spot_branch_url` を 1 つ（`fs::git` に remote から
URL を作る関数は既にある）と、`spot_act` の `Act::Enter` の枝に 1 つ。
枝が消えていたときの文言は決めが要る。

## TESTING-KEYS.md の最後の 1 キー `m u` — ARM64 実機で押して、歩き直していないことまで読んだ（f8b8c84 / 0.58.2、ARM64 レーン、無人の run）

ARM64 の Windows ノート PC（Windows 11 Home 26H1 build 28000.2956、`filer env` の
`Process arch aarch64`、Adreno X2-90 / Vulkan）で、無人 run として ARM64 の順番表の先頭
「TESTING-KEYS.md, `m u`」を通した。ビルドは `target\release\filer.exe` 0.58.2
（`filer --version` は `filer 0.58.2 (aarch64)`。同梱 ConPTY 1.24.260710001 arm64 を
`scripts\fetch-conpty.ps1` で配置）。スクラッチは RAM ディスクの無い機械なので
`C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（`TEMP` / `TMP` もそこ）。設定と状態は
run ごとに `…\arm-keys-mu\runs\<tag>\{cfg-filer,cfg-yazi,state}` に隔離し、**既定のキーマップ**で
回した（この機械の `keymap.toml` には `T` の割り当てがあるため）。昇格なし
（`IsInRole('Administrators')` = False）。スクリーンセーバーが入力デスクトップを握っていた
（`Desk::Name()` = `Screen-saver`、`SPI_GETSCREENSAVERRUNNING` = True）ので、キーは
**`--keys` だけ**、画面は `PrintWindow(PW_RENDERFULLCONTENT)` だけで触っている。

**`m u` にチェックを入れた。これで TESTING-KEYS.md は 249 / 249 で、全部埋まった。**
`cargo run --example make-keycheck -- --check` は
`in sync with src/config/defaults/keymap.toml (249 / 249 checked)`。ファイルは再生成していない
（`[ ]` → `[x]` と、生成器が数えなおす 2 か所の数だけを手で直した）。
**同じ証拠で TESTING.md 44.17 も埋まるので、そちらもチェックした**（下の「44.17」の節）。

`cargo test` はネイティブ ARM64 で **553 passed / 0 failed**（0.58.2）。ただし**この run の
1 回目だけ 1 件落ちた**ので、下の見つけたもの 2 に書いた。

証拠一式は `C:\dev\filer-evidence\arm-keys-mu\`（`lib.ps1`、`fx.ps1`、`step1-mu.ps1`、
`step2-mu.ps1`、`step3-bigtree.ps1`、`crop.ps1`、`out-*.txt`、`shots\*.png`）。

### 測った木

`fx.ps1` が作る 4 エントリ。**どれも `util::human_size` で別々の文字列になる大きさ**にして、
日付も 1 つずつ変えてある（`m t` が入れ替える側も読めるように）。

| 行 | バイト | 出るはずの文字列 | 更新日時 |
| --- | --- | --- | --- |
| `big\`（`data.bin` 1.5 MiB + `inner\more.bin` 0.5 MiB） | 2,097,152 | `2.0 M` | 2026-09-01 10:11 |
| `mid\` | 307,200 | `300 K` | 2026-09-02 12:13 |
| `small\` | 6,144 | `6.0 K` | 2026-09-03 14:15 |
| `note.txt` | 1,024 | `1.0 K` | 2026-09-04 16:17 |
| 合計 | 2,411,520 | `2.3 M` | — |

### `[mgr]` `m u` — Line mode: disk usage (gu)（チェックした）

`--keys` の 1 本道で、同じ前置きのうち片方にだけ `m u` を足す。`j` を 1 つ挟んでいるのは
**カーソルを 2 行目に置いておくため**で、これが「歩き直していない」の一番強い読みになる
（`start_usage` は空のフォルダに差し替えるので、歩き直せばカーソルは必ず先頭に戻る。
`gu` 直後が 1/4 であることを r1 で取ってある）。

| run | キー | 読んだもの |
| --- | --- | --- |
| r1 | `gu` | 大きい順に `big 2.0 M` / `mid 300 K` / `small 6.0 K` / `note.txt 1.0 K`、見出し `4 items`、下段 `1/4`、トースト `2.3 M in total — <Esc> to leave`（`r1-usage.png`） |
| r2 | `guj` | 同じ数字でカーソルが `mid`、下段 `2/4`（`r2-cursor-down.png`） |
| r3 | `gujmt` | 数字が **`2026-09-01 10:11` / `09-02 12:13` / `09-03 14:15` / `09-04 16:17`** に替わり、棒は残る。カーソルは `mid` のまま（`r3-mtime.png`） |
| r4 | `gujmtmu` | **数字が戻る**。`2.0 M` / `300 K` / `6.0 K` / `1.0 K`、見出し `4 items`、下段 `2/4`（`r4-usage-back.png`） |

- **r4 の画像は r2 の画像と SHA-256 まで同じ**（`6AB663FF640C503738DDB466A816719D4753624EBF057057D1ED639CB523449E`）。
  つまり `m t` → `m u` を通った窓は、**一度もモードを離れなかった窓と 1 ピクセルも違わない**。
  400 ms おきの 4 枚（`r4-after-1..4.png`）も同じハッシュで、後から何も起きていない。
- 他に何も: r3 と r4 の対で、窓のタイトル・クリップボード（毎回 `SENTINEL-<guid>` を入れてから押す）・
  作業ツリーの全ファイルの SHA-256・プロセスの生存が**すべて同じ**。`SendMessageTimeout(WM_NULL)` は 0 ms。
- カーソルも動いていない: `gujmtcc` と `gujmtmucc` が**どちらも**
  `C:\Users\yuu06\AppData\Local\Temp\filer-scratch\mu\mid` を返す。
- `Measuring…` のトーストは r4 のどの枚にも無い。出ているのは最初の走査の
  `2.3 M in total — <Esc> to leave` が 1 つだけで、2 つ目は増えていない。

#### 「歩き直していない」を、歩くのが高い木で測った

4 エントリの木では歩き直しても CPU はほぼ 0 なので、それだけでは弱い。**`target\` を持つ
このリポジトリ（`gu` で 21 G）**で同じ対を回し、プロセスの CPU を 0.5 秒おきに 32 秒読んだ
（`step3-bigtree.ps1`）。`--keys` は `App::settled()` しか待たず、走査中も真なので、
**`k` を 600 回打って `m t` を 20 秒後まで遅らせた**（カーソルは先頭にあるので `k` は何もしない。
`gucc` と `gukkkkkkkkkkcc` が同じ行を返すことを前に確かめた）。

| run | キー | CPU（秒） | 終わりの画面 |
| --- | --- | --- | --- |
| bigA | `gu` + `k`×600 + `mt` | 0.219 → 2.812、**19.1 秒以降は平ら** | 日付の列、`30 items`、トースト無し（`bigA-end.png`） |
| bigB | `gu` + `k`×600 + `mtmu` | 0.188 → 2.844、**19.9 秒以降は平ら** | `target 21 G` / `docs 5.5 M` / `src 1.5 M` / `QA-REPORT.md 560 K` …、`30 items`、トースト無し（`bigB-end.png`） |

- **物差し**: この木の走査そのものは最初の 1 秒で約 0.7 CPU 秒（0.219 → 0.922）。
  `m u` の後の 12 秒で bigB が使ったのは **0.016 秒**で、歩き直していればまず隠れない差。
- 2 本の合計は 2.812 対 2.844。差 0.032 秒は、起動のばらつき（最初の 2 秒で A 0.734 / B 1.156）の
  範囲内で、`m u` の後ではない。
- 見出しは両方 `30 items`（`N measured so far` ではない）。**トーストも出ていない** —
  歩き直していれば `Measuring…` が出て、6 秒は残る。

#### `gu` を通っていない一覧で押したとき

| run | キー | 読んだもの |
| --- | --- | --- |
| p0 | （なし） | 普通の一覧、列は空 |
| p1 | `mu` | 列に **`big 0 B` / `mid 0 B` / `small 0 B` / `note.txt 1.0 K`**（`p1-plain-mu.png`） |

- 他に何も: タイトル・クリップボード・ツリー・生存はすべて p0 と同じ。
  **CPU は 10 秒で 0.000**（`0.297` → `0.297`）で、**このキーは走査を始めない**。
- フォルダが `0 B` に見えるのは `Entry::usage_bytes` の仕様どおり（測っていなければ
  ディレクトリは 0）。ただし読む側には「空のフォルダ」と区別が付かない。下の提案 1。

### TESTING.md 44.17 もチェックした

行の 3 つの言い分を、上と同じ run で全部読んでいる。

1. 「`m t` で数字が日付に替わり棒だけが残る」 → r3。棒は 4 本とも残っている。
2. 「`m u` で歩き直さずにサイズが戻る（`Measuring…` が出ない）」 → r4（画像が r2 と同一）と
   bigA / bigB（CPU とトースト）。
3. 「`<Esc>` でタブ自身の表示に戻る」 → e1 `mpgujmt<Esc>` と e2 `mpgujmtmu<Esc>`。
   どちらも**権限の列（`drw` / `drw` / `drw` / `-rw`）に戻り**、カーソルは `big`、下段 `1/4`。
   **2 枚の画像は SHA-256 まで同じ**（`8FD9730D11B0260B9C87A0AC0A0D14B9734AAC9C10136AAAF59A3D8F1D14B023`）、
   `cc` で読んだカーソルも両方 `…\mu\big`。つまり `m u` は**タブが覚えている表示を壊さない**。

44.16（歩いている最中の `N measured so far`）は**取れていない**。この機械ではリポジトリの
21 G ですら 2 秒の時点で `30 items` / `21 G in total` になっていて（`bigA-t02-head.png`）、
走査中の見出しを捉えられなかった。チェックは付けていない。

### 見つけたもの 1: 一覧で `m u` を押すと、中身のあるフォルダが `0 B` に見える

`gu` を通っていない一覧で `m u` を押すと、`2.0 M` 入っている `big\` が `0 B` と出る
（`p1-plain-mu.png`）。ファイルだけは自分の大きさを出す（`note.txt 1.0 K`）ので、
**「フォルダは空、ファイルは中身あり」という読み方ができてしまう。**

- どこ: `src/fs/entry.rs:207` の `usage_bytes()` が、測っていないディレクトリに 0 を返す。
  `src/ui/list.rs:420` はそれを `human_size` に通すので `0 B` になる。
- `gu` のビューの中では正しい（そこでは必ず測ってある）。問題は**ビューの外で押せること**。
- 不具合として直すか、下の提案 1 のように出し方を変えるかは設計なので、**報告にとどめる**。

### 見つけたもの 2: `app::escape_and_max_preview::ending_a_busy_shell_asks_first` が、重い機械で落ちることがある

この run の**最初の `cargo test`** で 1 件だけ落ちた（552 passed / 1 failed）。

```
thread 'app::escape_and_max_preview::ending_a_busy_shell_asks_first' (6524) panicked at src\app.rs:7151:9:
a running program is asked about
```

- その後: 単体で 1 回落ち、**単体 10 連続は 10 回とも通り**、全件も 2 回とも 553 / 0。
  **機械が忙しいときだけ出る**（1 回目はリリースビルドの直後だった）。
- 筋: `Terminal::busy()` は「シェルの pid に子がいるか」でしかない（`src/terminal.rs:648`）。
  落ち着かせる待ちは 5 秒で諦める作りなので（`app.rs:7134`）、**プロファイル実行の短命な子**を
  掴んだまま先へ進み、`t.send(ping…)` の後の `while !busy()` がその子で即座に抜けて、
  `Act::Terminal(Some(false))` の時点では ping がまだ立ち上がっていない、という順番があり得る。
  テスト自身のコメントが「最初の版はこれで引っかかった」と書いているのと同じ穴で、
  **5 秒の諦めが残している分**だと思う。
- CI（x64）では見ていない。ARM64 固有ではなく**負荷の問題**だと思うが、この lane は毎回
  `cargo test` を回すので、再発したらここに積み増す。

### Proposals

#### 1. `gu` の外の `m u` は、測っていないフォルダを `0 B` と言うべきではない

**何に出くわしたか**: 「他に何も起きない」側を取るために、`gu` を通さず一覧で `m u` を押した。
`2.0 M` 入っている `big\` が `0 B`、`mid\` も `small\` も `0 B` と出た（`p1-plain-mu.png`）。
走査は始まらないので CPU は 10 秒で 0.000 — つまり**押しても何も起きないのに、嘘の数字が増える**。

**どう変えるべきか**: どちらかひとつ。
(a) 測っていないディレクトリは空欄か `—` にする（`linemode_text` の `L::Usage` に
`entry.usage.is_none() && entry.is_dir_like()` の枝を 1 つ）。
(b) `gu` のビューの外で `linemode usage` が来たら、列を変えずに
`Usage: press gu to measure` のトーストを出す。

**なぜ**: いまの `0 B` は**読める嘘**で、しかも「`m s`（サイズ）ではフォルダが空欄なのに、
`m u` では 0 と出る」という形の不一致になっている。`m` の和音を順に試す人は必ず通る。
`gu` を知らない人には、`m u` が壊れているようにしか見えない。

**大きさ**: (a) なら 1 行＋テスト。(b) は「列を変えないのに押した意味は残す」ので少し設計が要る。
推すのは (a)。

#### 2. `App::settled()` は走査中を「落ち着いた」と言うべきではない

**何に出くわしたか**: 「歩き直していない」を CPU で測るには、**走査が終わってから** `m u` を
押す必要がある。ところが `--keys` が待つ `App::settled()`（`app.rs:1832`）は、
読み込み中のフォルダとプレビューしか見ていないので、**21 G を歩いている最中でも真**になる。
結果、`k` を 600 回打って 20 秒稼ぐという形になった（`step3-bigtree.ps1`）。

**どう変えるべきか**: `settled()` の最初の条件に `self.usage.is_some()` を足す。
走査中のビューは「次のキーを受けて良い状態」ではない — 行が増え続けているので、
`j` も `c` も、その瞬間に何行目だったかで結果が変わる。

**なぜ**: `--keys` で `gu` の後に何かを押すスクリプトは、**いま全部この罠にかかる**
（押したキーが走査の最中に入り、結果が走査の速さで変わる）。#119 が出した `<Wait500>` の提案と
同じ穴を別の側から見たもので、こちらは**条件 1 つ**で、書く人が待ち時間を当てずに済む。

**大きさ**: 1 行。`--keys` には「5 秒で諦める」規定があるので、歩き続ける木でもスクリプトは止まらない。

#### 3. 使用量ビューにいることが、6 秒経つと画面のどこにも書いていない

**何に出くわしたか**: `bigA-end.png` / `bigB-end.png` は、トーストが消えた後の使用量ビューである。
見ると**普通の一覧にしか見えない** — タブの札は `1 filer-armtest`、パンくずは
`C:\dev\filer-armtest\target`、見出しは `30 items`、下段は `1/30`。ここが「`<Esc>` で抜ける
特別なビュー」で、`Enter` が普通の移動ではなくビューを抜ける動き（44.10）になっていることは、
**`21 G in total — <Esc> to leave` のトーストが生きている 6 秒の間しか画面に書かれていない。**

**どう変えるべきか**: ビューが開いている間ずっと残る印を 1 つ置く。タブの札を
`1 filer-armtest (usage)` にする、見出しを `30 items · 21 G total` にする、
`NORMAL` の隣にモードの札を出す、のどれか。合計を見出しに出すなら、**消えたトーストの
情報も戻る**ので一石二鳥。

**なぜ**: 大きい木ほど走査に時間がかかり、**読み始めるころにはトーストが消えている。**
そのまま `j` で降りていって `Enter` を押すと、本人は「フォルダを開いた」つもりで
ビューを抜けている。測り直すには `gu` をもう一度打つしかない（ビューの中では断られる、44.11）。

**大きさ**: 見出しに足すだけなら `ui::summary` に 1 引数と 1 行。札を増やすなら見た目の決めが要る。

## TESTING.md 24.2 と 1.37 — v0.57.0 が Q34・Q35 で入れたものを ARM64 で確かめた（a219071 / 0.59.2、ARM64 レーン、無人の run）

ARM64 の Windows ノート PC（Windows 11 Home 26H1 build 28000.2956、`filer env` の
`Process arch aarch64`、Adreno X2-90 / Vulkan）で、無人 run として ARM64 の順番表の先頭
「v0.57.0, Q34 and Q35」を通した。ビルドは worktree `C:\dev\filer-armtest` の
`target\release\filer.exe` 0.59.2（同梱 ConPTY 1.24.260710001 arm64 を
`scripts\fetch-conpty.ps1` で配置）。スクラッチは RAM ディスクの無い機械なので
`C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（`TEMP` / `TMP` もそこ）。昇格なし
（`IsInRole('Administrators')` = False）。

起動時はスクリーンセーバーが入力デスクトップを握っていた（`OpenInputDesktop` →
`Screen-saver`、`SPI_GETSCREENSAVERRUNNING` = True）。止めてから
`Default` / False に戻ったことを確かめ、以降の押下はすべて **`--keys`**、画面は
`PrintWindow(PW_RENDERFULLCONTENT)` だけで触っている（`PostMessage` は使っていない）。

`cargo test` はネイティブ ARM64 で **558 passed / 0 failed**（0.59.2）。
#122 で 1 度だけ落ちた `ending_a_busy_shell_asks_first` は、この run では落ちていない。

**チェックを入れたのは 1.37 の 1 行。**24.2 は TESTING-CHECKS.md に箱が無い
（「自動テスト済みなので下には出していない: 24.1, 24.2, 24.3」）ので、結果はここに置く。
`cargo run --example make-testcheck` を回して数を取り直し、`-- --check` は
`in sync with TESTING.md (220 / 420 checked, 0 untranslated)`。
`make-keycheck -- --check` も `in sync (249 / 249 checked)` のまま。

証拠一式は `C:\dev\filer-evidence\arm-q34-q35\`（`shots\*.png`、`1-37-pty.log`、
`1-37-proof.txt`、`win.ps1`）。

### 24.2 長い名前は語幹の中で省略され、拡張子が残る（Q34、v0.57.0）— 通った

`scripts\make-fixtures.ps1 -Path …\filer-scratch\filer-fixtures` で作った
`awkward names\` を、1360x860 の既定の窓で開いた（`shots\24-2-full.png`、
その行を 3 倍に拡大したものが `shots\24-2-row.png`）。

| 読んだもの | 値 |
| --- | --- |
| 画面に描かれた名前 | `very-long-long-long-long-long…long-long-name.txt` |
| 同じ行で `c` `f`（`--keys "jjj<Wait:500>cf"`） | `very-long-…(略)…-long-name.txt` **163 文字**（`Get-Clipboard`、押す前は `SENTINEL-24-2-B`） |

- **拡張子 `.txt` が残っている。**v0.45.0 以来この行が残っていた理由（末尾を切って
  `.txt` が消える）は無くなった。
- `…` は語幹の中（先頭 `very-long-long-long-long-long`、末尾 `long-long-name.txt`）。
  `elide_at` の「残す文字の 2/3 を前に、1/3 を後ろに」のとおり。
- おまけに**同じ窓の親の列**で `compare-left.txt` / `compare-right.txt` が
  `compar…ft.txt` / `compar…ht.txt` と出ていて、**狭い列でも 2 つが別物として読める**
  （Q34 が `report-….pdf` と `report-….docx` で挙げていた狙いが、実際の窓で見えている）。

**TESTING.md 24 節の前書きが古くなった（直していない）。**「**24.2 は半分だけ確かめられている**
… 実装は末尾を切って拡張子を落とすので、この行が求めているものではない。決まるまでこの行は残る
（QA-REPORT.md）」は v0.57.0 で解決済み。マージする側で書き換えてほしい。
24.2 の文言「Elided in the middle, with the extension still readable」は**そのままで合っている。**

### 1.37 ペインを閉じたまま `<A-t>`（Q35、v0.57.0）— 通った（チェックした）

`…\filer-scratch\t137\` に `a file 137.txt`（名前に空白）を 1 つ置き、`FILER_PTY_LOG` を
立てて、**ペインを一度も開かずに** `--keys "<A-t><Wait:8000>|<Space>Out-File<Space>proof.txt<Enter><Wait:3000>"`
で起動した。

| 行の半分 | 読んだもの |
| --- | --- |
run は 2 本。**run A** が上の `--keys` 一本道（`t137\`、`pty.log` と `proof.txt` を残す）、
**run B** が `--keys "<A-t>"` だけで止めて 1.5 秒後に撮ったもの（`t137b\`、トーストを読むため）。

| 行の半分 | 読んだもの |
| --- | --- |
| ペインが開く | run A: `pty.log` が生まれ、`== pane opened at 1790823247835` から始まる。run B: `shots\1-37-t1500.png` にペインが写っている |
| 「The terminal is not open」と言わない | run B の `<A-t>` 1.5 秒後の画面に出ているトーストは `Started pwsh — <C-t> back to the list` だけ（`shots\1-37-t1500.png`） |
| 引用されたパスが行に入る | run A: `pty.log` 10 行目 `90 in key 'C:\Users\…\t137\a file 137.txt'`。run B の画面のプロンプトにも同じ 1 行が入ったまま（`shots\1-37-t1500.png`） |
| プロファイル読み込み中に消えない | run A のシェルは `個人プロファイルとシステム プロファイルの読み込みに 2488 ミリ秒かかりました。`（`shots\1-37-after.png`）。パスが送られたのはその **90 ms** の時点なのに、**8 秒後に `<Enter>` した時点で行は無傷**で、`proof.txt` の中身が `C:\Users\yuu06\AppData\Local\Temp\filer-scratch\t137\a file 137.txt`（69 バイト、1 行）。run B（プロファイル 530 ms）でも 1.5 秒後の行は無傷 |
| キーはペインに移る | run A で `<A-t>` の後に打った `| Out-File proof.txt` が**ディスクに `proof.txt` を作った。**一覧に届いていればファイルはできない。`pty.log` の `in key` も 1 文字ずつ並んでいる（122〜359 行） |

- **送られた時刻**: ペインが開いて **90 ms**。その 14 ms 前（76 ms）にシェルが
  `PowerShell 7.6.6\r\n` を書いている。`has_drawn()` が見ているのはこれで、
  **プロンプトではない**（run A ではプロファイルだけで 2488 ms かかっている）。それでも
  ConPTY が入力を溜めてくれるので行は無傷だった。下の「提案 1」を参照。
- 他に何も: 窓のタイトルは前後とも `Filer: C:\…\t137`、作業フォルダに増えたのは
  `proof.txt` と `pty.log`（どちらもこの検査が作らせたもの）だけ。

### 見つけたもの

#### 1. 相対パスを引数にすると、タブが相対パスのまま動けなくなる（この run で踏んだ）

`filer t137`（cwd は `…\filer-scratch`）で起動すると、**中身は正しく出るのに**
タブが相対パス `t137` を持ったままになる（`shots\arg-e-rel-good.png`）。

| 読んだもの | 出たもの | 出るべきもの |
| --- | --- | --- |
| 窓のタイトル | `Filer: t137` | `Filer: C:\Users\yuu06\AppData\Local\Temp\filer-scratch\t137` |
| 見出しのパス | `t137\a file 137.txt` | 絶対パス |
| 親の列 | **空**（1 行も無い） | `filer-scratch\` の中身 |
| `c` `c`（`--keys "<Wait:1200>cc"`） | `t137\a file 137.txt` | 絶対パス |
| `h`（上の階層へ）を押した後のタイトル | `Filer: t137` のまま（`shots\arg-f-rel-up.png`） | 親へ移る |

**`h` が効かないので、この窓はキーでは上に抜けられない。**

読んだ原因（ソース、走らせて確かめたわけではない）: `main.rs:180` が
`cli.path.as_deref().map(util::normalize)` で、`util::normalize` は**字面だけを整える**ので
cwd と繋がない。`g<Space>` の側は `util::resolve(base, input)` を通るので絶対パスになる。
同じ `resolve` を起動時にも通せば済むように見える。

#### 2. 存在しない**相対**パスを引数にすると、名前の無いエラーが出る（23.6 の抜け）

同じ原因の表の面。cwd を `…\filer-scratch\t137` にして:

| 起動の仕方 | 出たトースト |
| --- | --- |
| `filer "C:\…\filer-scratch\t137\tpyo"`（絶対） | `No such file or folder: tpyo — showing C:\Users\yuu06\AppData\Local\Temp\filer-scratch\t137`（`shots\arg-c-abs-bad-toast.png`）— **23.6 のとおり** |
| `filer zzz-not-a-thing`（相対） | `: 指定されたパスが見つかりません。 (os error 3)`（`shots\arg-d-rel-bad-toast.png`）— **コロンの前が空で、OS の生のメッセージ** |

窓はどちらも cwd に落ちるので、**間違っているのは言い方だけ**。
読んだ原因: 相対パス 1 要素の親は `""` で、その走査が失敗し、
`app.rs:1532` の `format!("{}: {error}", util::file_name(&path))` の
`util::file_name(Path::new(""))` が空文字列を返す（`util.rs:336`、`file_name()` が `None`、
UNC 共有でもないので「パスそのもの」＝空が返る）。1 を直せば 2 も消えるはず。

#### 3. 2 つ目の位置引数が黙って無視される（1 の入り口）

`filer <good dir> zzz` は**後から来たほうを採る**（`main.rs:161` が毎回 `cli.path` を
上書きする）ので、窓は cwd に落ちる（`shots\extra-arg.png`、タイトル `Filer: C:\dev\filer-armtest`）。
**引用し忘れた空白入りのパスがちょうどこの形になる。**この run でも
`filer C:\…\awkward names` を引用せずに投げて、窓が別の場所に開いた
（`shots\arg-b-unquoted.png`）。`--keys` が「打てないキー」を起動前に断るのと同じ扱いで、
2 つ目の位置引数も断ってよいはず。

### Proposals

#### 提案 1: `<A-t>` が待つのは「何か描かれた」ではなく「プロンプトが出た」にする

**踏んだこと**: 1.37 の `pty.log` で、パスが送られたのはペインが開いて **90 ms** のところ。
その直前にシェルが書いたのは**バージョンの行**（`PowerShell 7.6.6`）だけで、プロンプトが
出たのはずっと後（同じ機械の別の run ではプロファイルに **2488 ms** かかっている）。
今回は ConPTY が入力を溜めてくれたので行は無傷だったが、**待っている条件と、消えない
理由が別物**になっている。

**どう変えるべきか**: `has_drawn()` を「最初の文字が来た」から、**OSC 7 かシェルの最初の
プロンプト**（`Terminal::shell_cwd` が埋まる、または `in reply` の後の最初の入力待ち）まで
引く。あるいは今の条件を残したうえで、**プロンプトが出るまでは送らずに溜め続ける**
上限 5 秒を、`has_drawn` ではなくプロンプトに掛ける。

**なぜ**: 今のままだと、プロファイルの長いシェル、バナーを先に出すシェル（nu、starship を
積んだ pwsh）、`Read-Host` を踏むプロファイルで、**たまたま消える**日が来る。
`<A-t>` は「送ったのに消えた」が一番分かりにくい壊れ方で、しかも再現が環境依存になる。
TESTING.md 1.37 の文言も「once the shell's prompt is up」と書いているので、**行のほうが
実装より正しい。**

**大きさ**: `Terminal::has_drawn` の隣にもう 1 つ述語を足して `pump_terminal` の条件を
差し替える程度。プロンプトの検出をどう定義するかが設計の判断（OSC 7 に寄せるなら
pwsh / bash / zsh で違う）。

#### 提案 2: 起動時のパスを `g<Space>` と同じ `resolve` に通す

**踏んだこと**: 上の「見つけたもの 1」。`filer t137` で親の列が空になり、`h` が効かず、
`c` `c` が相対パスを配った。

**どう変えるべきか**: `main.rs:180` の `util::normalize` を
`util::resolve(&home, p)` にする。`home` はその 4 行上で既に組んである。

**なぜ**: シェルから `filer .` や `filer ..\other` と打つのは**一番自然な呼び方**で、
補完が出すのも相対パス。今はそれが「上に戻れない窓」になる。`c` `c` が相対パスを
配るのは、貼った先で意味が変わるので実害もある。

**大きさ**: 1 行。`util::resolve` には既にテストがある。

#### 提案 3: 位置引数は 1 つだけ受け、2 つ目は起動前に断る

**踏んだこと**: 上の「見つけたもの 3」。引用し忘れで窓が黙って別の場所に開いた。

**どう変えるべきか**: `parse_cli` で `cli.path` が既に `Some` なら
`filer: too many paths: "names" (did you forget to quote a path with a space?)` と
言って `exit(2)`。`--keys` が打てないキーを起動前に断っているのと同じ形。

**なぜ**: 空白入りのパスは Windows では普通（`C:\Program Files`、`awkward names`）で、
引用し忘れは**必ず起きる**。今は「開いた、でも場所が違う」で、原因が引数だと気づくのに
時間がかかる。断れば 1 行で分かる。

**大きさ**: `parse_cli` に 4 行。

---

## TESTING.md section 25 — `filer env` を Linux で確かめた（6b7ad58 / 0.59.6、Linux lane、無人の run）

Ubuntu x86_64 のクラウドコンテナ、Xvfb（X11）と lavapipe（`llvmpipe (LLVM 20.1.2, 256 bits) (Vulkan, Cpu)`）。
debug ビルド（`target/debug/filer`）。設定と状態は `FILER_CONFIG_HOME` / `YAZI_CONFIG_HOME` /
`FILER_STATE_HOME` をスクラッチの空ディレクトリに向けて、毎回の状態を自分で作った。

結果: 30 行のうち `[x]` 17、`[-]` 9、`[ ]` 4（25.4 / 25.4a / 25.8b / 25.11）。証拠は PR 本文に 1 行ずつ。

### 見つけたもの 1: Linux の `filer env` が、ペインで起動しないシェルを書く（25.4a）

`[term] shell` を設定していないとき、Tools の行は

```
sh  : /usr/bin/sh   (terminal pane, the platform default)
```

と出る。ところが同じ環境（`SHELL=/bin/bash`）でペインを開き `echo $0 $PWD >shell.txt` を打つと、
ファイルには `/bin/bash /…/scratchpad/pane` が入った。**ペインで動くのは `$SHELL`（bash）で、`sh` ではない。**
25.4a の「実際に起動するほう — 推測ではない」が Linux では崩れている。

原因は `envreport::tools` が `terminal::default_shell()`（Windows の `pwsh` 判定だけ）が `None` のとき
`DEFAULT_SHELL = "sh"` に倒すこと。ペインの最初のトーストに出る `terminal::shell_label` は
`$SHELL` を見ていて正しい。**同じ問いに 2 か所が別の答えを出している。**macOS（`$SHELL` は普通 zsh）でも同じはず。

`[term] shell = "pwsh"` を入れた半分は正しい（`pwsh : not found   (terminal pane, from [term] shell)`）。

### 見つけたもの 2: Linux（と macOS）ではフォントを 1 つも探さない

`install_fonts` の候補は `%LOCALAPPDATA%\Microsoft\Windows\Fonts` と `C:\Windows\Fonts` だけ。Linux では
`fc-list` に DejaVu / Liberation / FreeFont が並んでいても、`[ui] fonts` を書かない限り

```
Fonts : none loaded — this is why icons are boxes
Bold  : none found; bold is faked by overstriking
```

になり、egui の組み込みフォントだけで描く。**`fonts-noto-cjk` を入れても日本語の名前は豆腐のまま**のはず
（linux-role.md の「`fonts-noto-cjk` を入れるまで箱」という記述は、この実装では成り立たない。
入れても読まれない）。`[ui] fonts = [".../LiberationMono-Regular.ttf"]` を書けば読まれる（25.10 で確認）。

### 見つけたもの 3: `-Regular` の付かない名前は太字の兄弟が見つからない

`[ui] fonts = ["/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"]` では `Bold : none found` だった。
同じディレクトリに `DejaVuSansMono-Bold.ttf` がある。`bold_siblings` は語幹が `-Regular` で終わるときしか
`-Bold` を試さない。`LiberationMono-Regular.ttf` では `LiberationMono-Bold.ttf` が見つかった。
Linux の配布物には `-Regular` を付けない名前が多い（DejaVu、Noto の一部）。

### 見つけたもの 4: spot がディレクトリを「Hardlink, Links 2」と言う（13 節に関係）

25.16 の `filer <fixtures> --keys "<Tab>C"` で、先頭の `awkward names`（ただのディレクトリ）の spot が

```
Link
Kind	Hardlink
Links	2
```

だった。Unix のディレクトリは `.` と親からの名前で、作った直後から link count が 2 以上ある（サブディレクトリが
増えるたびに 1 増える）。ハードリンクではない。Windows の NTFS ではディレクトリの link count は 1 なので、
Linux / macOS でだけ出る誤り。13 節を Linux で回すときに当たる。

### 見つけたもの 5: keymap.toml の警告だけ、どのディレクトリのファイルかを言わない

壊した `yazi.toml` は `…/fcfg/yazi.toml: TOML parse error at line 1, column 5` とフルパスで出るのに、
壊した `keymap.toml` は `keymap.toml: TOML parse error at line 4, column 14` だけだった
（`config/keymap.rs:194`）。設定ディレクトリは 2 つあり、どちらの `keymap.toml` か分からない。
また Warnings の値の最後に空行が 1 つ付く（toml のエラー文の末尾の改行がそのまま残る）。

### TESTING.md の行として古いもの

- **25.4**: Tools の節に `pdftoppm` `ffmpeg` `ffprobe` `pwsh` は**もう出ない**。今の Tools は `git`、ペインのシェル、
  `[[preview]]` と opener が名指すプログラムだけ（`envreport::tools` のコメントのとおり、プレビューは
  プロセス内で済む）。行の書き直しが要るので `[ ]` のまま。
- **25.1**: 「The four sections」だが、今は Filer / Config / Last run / Tools / Variables の **5 つ**。
- **25.19**: Windows 以外の `os_line` は arch を `Process arch` の 1 行しか出さない（設計どおり。
  `bugreport.rs:228` のコメント）。`filer env | grep arch` は 1 行。行が Windows 前提なので `[-]` にした。

### 付けなかった行

- **25.8b**: `WINIT_X11_SCALE_FACTOR=1.5` で起動すると、X の `xwininfo` が `2040 x 1290` を返し、
  `filer env` も `2040 x 1290 px (1360 x 860 pt @ 1.5)` と出た（**例の数字と完全に一致**）。ただし
  行の後半「右端も下端も切れていない」は見た目なので付けていない。
- **25.11**: 文言は出た（`none found; bold is faked by overstriking`）。ただ、機械には太字の
  フォントがあった（見つけたもの 2・3 のせいで探していないだけ）。行の前提「ボールド体がどこにも無い」
  を満たしていないので付けていない。

### Proposals

#### 提案 1: `filer env` のシェルの行を `shell_label` と同じ規則にする

**踏んだこと**: 見つけたもの 1。診断が、動いているシェルと違う名前を出した。

**どう変えるべきか**: `envreport::tools` で、`[term] shell` も `default_shell()` も無いときは
Windows 以外では `$SHELL`（無ければ `sh`）を引く。`shell_label` と 1 つの関数にまとめるのが確実。

**なぜ**: v0.28.1 の「動いているものを壊れていると言う」の裏返しで、こちらは「動いていないものを動いていると
言う」。zsh の設定が効かないという報告で `filer env` を貼られたとき、`sh` と書いてあれば読む側が迷う。

**大きさ**: 数行と、`name_shell` 型の純粋関数のテスト 1 つ。

#### 提案 2: Linux / macOS の既定フォントを探す

**踏んだこと**: 見つけたもの 2・3。

**どう変えるべきか**: `#[cfg(target_os = "linux")]` で `/usr/share/fonts`・`~/.local/share/fonts` の
よく知られた名前（Noto Sans Mono CJK、DejaVu Sans Mono、Nerd Font 各種）を、`#[cfg(target_os = "macos")]` で
`/System/Library/Fonts`（Hiragino、Menlo）を候補に足す。`bold_siblings` は `-Regular` が無いときも
`{stem}-Bold.{ext}` を試す。`fontconfig` を呼ぶ案は C 依存なので、Pure Rust 優先の方針なら固定の一覧が先。

**なぜ**: いま Linux で日本語のファイル名を正しく表示する手段は `filer.toml` に手でパスを書くことだけで、
README にもそう書かれていない。

**大きさ**: 候補の一覧 2 つと兄弟探索に 2 行。どの名前を入れるかは持ち主の判断（QUESTIONS.md 向き）。

#### 提案 3: spot の「Hardlink」をディレクトリでは出さない

**踏んだこと**: 見つけたもの 4。

**どう変えるべきか**: link count からハードリンクを言うのはファイルだけにする（ディレクトリはハードリンクを
作れない）。

**大きさ**: `spot.rs` の条件 1 つ。13 節の Linux レーンの前に直すと、その run が誤りで止まらない。

#### 提案 4: keymap.toml の警告にもフルパスを付ける

**踏んだこと**: 見つけたもの 5。

**どう変えるべきか**: `config/keymap.rs:194` を `at(dir, "keymap.toml")` の形にし、エラー文の末尾の改行を
`trim_end` する（他の 3 ファイルも同じ）。

**大きさ**: 1〜2 行。

#### 提案 5: linux-role.md の apt-get の行に描画に要るライブラリを足す

**踏んだこと**: 書かれた `apt-get install -y -q xvfb xdotool xclip x11-utils imagemagick` だけでは、
最初の起動が `Library libxkbcommon-x11.so could not be loaded.` で panic し、`xrun.sh` は
`no window appeared` で終わった。`libxkbcommon-x11-0 mesa-vulkan-drivers libvulkan1 libegl1
libgl1-mesa-dri` を足したら起動した（lavapipe は `mesa-vulkan-drivers` が入れる）。

**どう変えるべきか**: role の 1 行に足す。`xrun.sh` の道具の確認に `ldconfig -p | grep -q libxkbcommon-x11`
を足せば、panic の backtrace ではなく 1 行で分かる。

**大きさ**: 2 行。`.claude/` は Linux レーンが触れないので、マージする側に頼む。

#### 提案 6: TESTING.md の 25.1 / 25.4 / 25.19 を今の出力に合わせる

上の「TESTING.md の行として古いもの」。25.4 は Tools の今の中身（git、シェル、preview / opener の名指す
プログラム）で書き直し、25.19 には「Windows 以外は arch が 1 行」を書き添える。

---

## TESTING.md v0.57.3〜v0.59.5 の静かなキー 24 行 — ARM64 実機で確かめた（6b7ad58 / 0.59.6、ARM64 レーン、無人の run）

ARM64 レーンの順番表（`.claude/windows-role.md`「The ARM64 lane」）の先頭、
**「v0.57.3 to v0.59.5, the quiet keys」24 行**を 1 本で片付けた。24 行のうち
**22 行が合格**（うち 21 行を TESTING-CHECKS.md でチェック、1 行は既に `[x]` だった）、
**1 行は昇格権限が無くて半分しか押せず**（13.17）、**1 行は節の文言どおりの経路では再現しない**
（29.8。別経路では文言どおりに出たのでチェックは付けた。下の「見つけたもの 2」）。

### 走らせたもの

| | |
| --- | --- |
| 機械 | `(Get-CimInstance Win32_ComputerSystem).SystemType` = `ARM64-based PC`、Windows 11 Home 26H1（build 28000.2956）、PowerShell 7.6.6 |
| 昇格 | **なし**（`IsInRole('Administrators')` = `False`）。開発者モードも無効（`AllowDevelopmentWithoutDevLicense` 未設定） |
| 入力デスクトップ | `OpenInputDesktop` → `Default`、`SPI_GETSCREENSAVERRUNNING` = `False`（run 中に 2 回確認） |
| ネイティブ build | `cargo build --release` → PE machine `0xAA64`。`filer env` の `Process arch` = `aarch64` |
| x64 build | `cargo build --release --target x86_64-pc-windows-msvc` → PE machine `0x8664`、`Process arch` = `x86_64 (emulated on aarch64)` |
| ConPTY | `scripts\fetch-conpty.ps1` → **1.24.260710001 (arm64)** を `target\release` に |
| 作業場所 | `C:\Users\yuu06\AppData\Local\Temp\filer-scratch`（**この機械に RAM ディスクは無い**。`auto-wintest.ps1` が決めて `TEMP` / `TMP` に入れた） |
| `cargo test` | **566 passed; 1 failed** → 下の「見つけたもの 1」。設定ディレクトリを隔離すると **567 passed; 0 failed**（3.14s） |

### 見つけたもの 1 —— `cargo test` が「その機械に設定ファイルがあるか」で落ちる（本物の不具合）

`ui::overlay::help_frame::help_from_the_pane_lists_the_panes_keys_first` が
**この機械では必ず落ちる**。同じコミット（6b7ad58）の CI（Windows x64 ランナー）は緑。
ARM64 かどうかではなく、**走らせた人の `%APPDATA%` に yazi / filer の設定ファイルがあるか**で
決まる。

- 落ちる場所: `src/ui/overlay.rs:1653` の
  `at("keys in the list (<C-t> to get there)").expect("the list's heading")`
- `f.texts` を出させたところ、ヘルプのパネルに描かれたのは
  `config` の節 → `the mouse` → `keys in the terminal pane` → ペインのキー 13 個で終わり。
  **`keys in the list` の見出しはフレームに 1 度も現れない。**
- `config` の節が長いのが原因。この機械には実物の設定があり、パネルは
  `C:\Users\yuu06\AppData\Roaming\yazi\config\` の `yazi.toml` / `keymap.toml` と
  `C:\Users\yuu06\AppData\Roaming\filer\` の `keymap.toml` を、
  それぞれ「見出し＋`on disk, not read yet — <C-F5> re-reads config`」の 2 行で並べる。
  その分だけ下が押し出され、ハーネスの画面からはみ出す。
- **証拠**: 同じコミット・同じバイナリで、環境変数だけ変えた 2 回。

```
cargo test help_from_the_pane_lists_the_panes_keys_first
  -> test result: FAILED. 0 passed; 1 failed      (3 回連続、毎回同じ)

$env:YAZI_CONFIG_HOME = <空のディレクトリ>; $env:FILER_CONFIG_HOME = <同じ>
cargo test help_from_the_pane_lists_the_panes_keys_first
  -> test result: ok. 1 passed; 0 failed
cargo test                                         (全件)
  -> test result: ok. 567 passed; 0 failed
```

- **どちらの間違いか**: プログラム（テスト）側。`ui::harness::Screen` は一時ディレクトリに
  ツリーを作るが、**設定ディレクトリは実物を読んでいる**。CI のランナーには設定が無いので
  緑のまま通り、**設定を持っている人の手元でだけ落ちる**。この run は「手元で全部回す」
  規則に従って回したので見つかった。
- **直し方の見当（実装はしない）**: `Screen::open` が `YAZI_CONFIG_HOME` /
  `FILER_CONFIG_HOME` をテスト用の空ディレクトリに向けるか、`Config` を
  「ファイルを読まない」形で組んでから差すか。どちらにせよ **1 か所**で、
  このテストだけの問題ではない（ヘルプのパネルを見る他のテストも同じ地雷を踏みうる）。
- ARM64 固有ではない。x64 の機械でも `%APPDATA%\yazi\config\yazi.toml` を置けば落ちるはず。

### 見つけたもの 2 —— 29.8 の `powershell (Windows PowerShell 5.1)` は `[term] shell` 経由では出ない

29.8 は「`powershell`（5.1）として開いたペインで `<A-Up>`」と書いてある。pwsh が入っている
この機械でそれをやる自然な方法は `filer.toml` に `[term] shell = "powershell"` と書くことだが、
**その経路では版が付かない。**

- `[term] shell = "powershell"` のとき、トーストは
  `` `powershell` has not said where it is (no OSC 7). PowerShell: set LocationChangedAction in that shell's $PROFILE — the line is in the README ``
- `PATH` から `WindowsApps`（pwsh のエイリアス）を外し、`[term] shell` を書かずに起動したとき、
  `` `powershell (Windows PowerShell 5.1)` has not said where it is (no OSC 7). … ``
  ——**節の文言どおり。**

理由は `src/terminal.rs:193` の `name_shell`:

```rust
match program {
    Some(p) => crate::util::file_name(Path::new(p)),                      // 設定で名指ししたとき
    None if windows => "powershell (Windows PowerShell 5.1)".to_owned(),  // 既定に落ちたとき
    ...
}
```

版が付くのは `None`（＝ pwsh が見つからず、プラットフォーム既定に落ちた）のときだけ。
`src/app.rs:6374` の単体テスト `it_names_the_shell` は
`no_osc7("powershell (Windows PowerShell 5.1)")` を渡しているので、
**作者の意図は「版付きの札が来る」だったのに、実際にそう来る経路が 1 つしかない。**

- **どちらの間違いか**: 設定で `powershell` と名指しした人にも「5.1 のほうの `$PROFILE`」と
  言ってやるのが #101 の趣旨なので、**プログラム側を直すのが筋**
  （`shell_label` が `powershell` / `pwsh` を見て版を足す）。直さないなら 29.8 の文言に
  「pwsh が入っていない機械で」と条件を足すべき。
- チェックは付けた（条件を満たす形で文言どおりに出たので）。ただし**上の条件付き**。

### 見つけたもの 3 —— 23.5 のトーストは「打ったパス」ではなく「その親」を名指す

23.5 で `g<Space>` に `C:\Temp\a|b\c\d` と打って `<Enter>` を押すと、トーストは **1 つ**で
`C:\Temp\a|b\c: 指定されたパスが見つかりません。 (os error 3)`。
節が求めるのは「**1 つ**のエラートースト、まるごとのパスを名指す」で、断片（`b: …`、`c: …`）
ではない。`C:\Temp\a|b\c` は根からのまるごとのパスなので**合格**だが、打ったのは `…\c\d` で、
最後の 1 段が落ちている。実害は小さいが、節の次の改訂で「どの段を名指すか」を決めるなら
記録しておく価値がある。押してから 1.1 秒と 1.8 秒の 2 枚を撮り、トーストが 1 つしか
積まれていないことも確かめた。

### 見つけたもの 4 —— 13.17 の半分は昇格が要る

`mklink /J` は通る（`Junction created for …`）が、`mklink /D` は
`You do not have sufficient privilege to perform this operation.`。
無人の run は昇格していないし、開発者モードも無効。だから 13.17 は
**ジャンクションの半分だけ確かめ、チェックは付けていない**。
45.11 と同じ理由で、人か昇格した run のための行。

### 1 行ずつ

`<Tab>` で spot を開き `C` で貼った板、`filer env` の出力、`FILER_PTY_LOG`、
`(Get-Process filer).CPU`、窓のタイトル、`PrintWindow(PW_RENDERFULLCONTENT)` の画面を読んだ。
画面は `C:\Users\yuu06\AppData\Local\Temp\filer-scratch\shots\` に残っているが
**それは消える場所**なので、読んだ文字列はここと PR 本文に写してある。

| # | 結果 | 読んだもの |
| --- | --- | --- |
| 12.13 | 合格・チェック済み | 1 つで `Trashed t1.txt — u to undo`、2 つ選んで `Trashed 2 item(s) — u to undo`。ディスク上でも `t1` `t2` `t3` が消えた |
| 12.14 | 合格・チェック済み | タスク板が `Trash 5 item(s)  [running]`（動詞は 1 回）、その下が `0/5 files`。`0 B / 0 B` の行は無い。**400 MB × 5 でないと捕まらない**（提案 5） |
| 13.17 | **半分だけ**・チェック無し | ジャンクションの spot は `Link` 節の `Kind` が `Junction`、`Target` と `Resolves` が実体のパス。`mklink /D` は昇格が無くて作れず、`Symlink` の半分は未確認 |
| 15.9 | 合格・チェック済み（**`R:` の代わりに UNC**） | `Hardlink: a.txt: hardlinks can't cross drives (\\192.168.0.150\Backup → C:). Use p to copy instead`。Windows の「ファイルを別のディスク ドライブに移動できません」ではない。コピー先は空のまま。生の `CreateHardLinkW` も error 17（`ERROR_NOT_SAME_DEVICE`）を返す＝同じ分岐 |
| 20.7 | 合格・チェック済み | `keymap.toml` に `run = 'cd C:\Windows\System32'`（中に引用符なし）→ `<F8>` で窓のタイトルが `Filer: C:\Windows\System32`。`filer env` の `Warnings` は `none` |
| 23.5 | 合格・チェック済み | トースト **1 つ**、`C:\Temp\a|b\c: 指定されたパスが見つかりません。 (os error 3)`（見つけたもの 3） |
| 23.6 | 合格（**この行は自動テスト済みで、押す一覧には無い**） | `filer <fx>\tpyo` → 親が開き、赤いトースト `No such file or folder: tpyo — showing C:\…\fx`。`filer <fx>\notes.md` → `fx` が開いて `notes.md` にカーソル（10/15）、トーストは無し |
| 24.6 | 合格・チェック済み | `make-fixtures.ps1` を新しいフォルダで走らせると `警告: awkward names: 5 entries on disk, expected 6 (UPPER.TXT and upper.txt are one file in a case-insensitive folder; 24.3 needs fsutil file setCaseSensitiveInfo <dir> enable)` と `警告: 1 group(s) did not come out as intended`。節が言う例外どおり（警告が出ない側は `fsutil` に昇格が要るので未確認） |
| 25.21 | 合格・チェック済み | ネイティブ `Executable : C:\dev\filer-armtest\target\release\filer.exe` / `Process arch : aarch64`。x64 `Executable : …\x86_64-pc-windows-msvc\release\filer.exe` / `Process arch : x86_64 (emulated on aarch64)`。PE machine は `0xAA64` と `0x8664` |
| 25.22 | 合格・チェック済み | PTY ログで、プロンプト `❯` が `out` に出るのが **t=1009 ms**、最初の `in key` が **t=1878 ms**（`<Wait:2000>`）。`hi` が `out` に t=2277 ms。`<C-S-Enter>` でペインが窓を取った。`filer --keys "<Wait:1.5s>"` は窓を出さず exit 2、`` filer: --keys: `<Wait:1.5s>` is not a wait; write milliseconds up to 60000, as `<Wait:500>` `` |
| 25.23 | 合格・チェック済み | ペインを開いて `<C-S-Enter>` → 終了 → `filer env` が `Terminal pane : 35 x 159 (lines x columns)`。ペインを開かなかった run のあとは `Terminal pane : not opened in that run` |
| 25.24 | 合格・チェック済み | `filer .` → タイトルが絶対パス `Filer: C:\…\fx\many`、親の列あり、`h` で `Filer: C:\…\fx`。`filer ..` → `Filer: C:\…\fx`。`filer two words` → 窓を出さず exit 2、`filer: more than one path: "two" and "words" (a path with a space in it needs quotes)` |
| 29.7 | 合格・チェック済み | `[term] shell` 無し: `Started pwsh — <C-t> back to the list`、ペインの表示は `PowerShell 7.6.6`。`shell = "powershell"`: `Started powershell — <C-t> back to the list`、ペインで `$PSVersionTable.PSVersion` が `Major 5 / Minor 1` |
| 29.8 | 合格・チェック済み（条件付き） | `` `powershell (Windows PowerShell 5.1)` has not said where it is (no OSC 7). PowerShell: set LocationChangedAction in that shell's $PROFILE — the line is in the README ``。5.1 の `$PROFILE`（`…\WindowsPowerShell\Microsoft.PowerShell_profile.ps1`）はこの機械に存在しない＝フックは無い。条件は見つけたもの 2 |
| 31.13 | 合格・チェック済み | `net view \\YUU06` が `There are no entries in the list.`＝共有を出さないホスト。`g<Space>\\YUU06<Enter>` → タイトル `Filer: \\YUU06`、一覧は `(no shares)`。`(empty)` ではない |
| 31.14 | 合格・チェック済み | `g<Space>\\192.168.0.31<Enter>`（応答の無いアドレス）→ タイトルが `Filer: \\192.168.0.31`。`<Esc>` を投げて **626 ms** で `Filer: C:\…\fx\many` に戻り、トースト `Stopped waiting for \\192.168.0.31`。`j` でカーソルが 1/500 → 2/500。**50 秒後と 80 秒後**に撮り直しても新しいトーストは無く、プロセスは生きている |
| 32.10 | 合格・チェック済み | 綴り違い: `` Open failed: `Hidemruu.exe` was not found — Hidemruu.exe "C:\…\t4.txt" ``。存在するが失敗する側（`cmd /c exit 3`）: `Open failed: exit code 3 — cmd /c exit 3 "C:\…\t4.txt"`。`filer env` も `Hidemruu.exe : not found (opener [typo])` |
| 40.17 | 合格・チェック済み | ペインで nvim を開き `<C-S-Enter>` で全画面に。`SendInput` の `MOUSEEVENTF_WHEEL` を 1 ノッチ → PTY ログに `in key \e[<65;80;19M` が **1 行**（t=28078 ms）。3 秒おいて 3 ノッチ → **3 行**（t=31102 / 31276 / 31432 ms）。5 行ではない |
| 44.16 | 合格・チェック済み | `C:\Users\yuu06` で `gu`: ヘッダが `4 measured so far` → `10 measured so far` → `11 measured so far` と増え、終わると `42 items · 18 G total` |
| 44.17 | 合格（**TESTING-CHECKS.md では既に `[x]`**。ARM64 でも確認） | `gu` の中で `m t` → 数字が `2026-10-01 13:07` に替わり棒は残る。`m u` → サイズが戻り（`151 K` / `105 K` / …）、ヘッダは `15 items · 293 K total` のまま、**`Measuring…` のトーストは出ない**。`<Esc>` → ふつうの一覧（`15 items`、列なし） |
| 44.19 | 合格・チェック済み | 歩き終わって **25 秒後**でもヘッダは `5 items · 28 G total`（`C:\dev`）。`filer C:\Users\yuu06 --keys "gu<Wait:0>j"` は歩いている間ずっと `1/11`（`4 measured so far` のフレーム）で、終わってから `2/42` |
| 45.17 | 合格・チェック済み | 見出しは `proj ↔ proj`、その下に **両方のフルパス** `C:\Users\yuu06\AppData\Local\Temp\filer-scrat…-of-the-comparison\proj ↔ C:\Users\yuu06\AppData\Local\Temp\filer-scrat…-of-the-comparison\proj`（**真ん中で切れて**両端が読める）。フォルダの行は `= sub\` で、子の `~ sub\deep.txt` と同じ `\`。`/` ではない |
| 46.21 | 合格・チェック済み | `CHANGELOG.md` の spot で `<A-j>` を 17 回 → カーソルが `From branch  claude/task-09i0cs`。`<Enter>` → トースト `Opened https://github.com/uchmk/filer/tree/claude/task-09i0cs`。`Get-CimInstance Win32_Process` のコマンドラインも `chrome.exe --single-argument https://github.com/uchmk/filer/tree/claude/task-09i0cs` |
| 47.5 | 合格・チェック済み | `f` のプロンプトを開いて放置。`(Get-Process filer).CPU` が **0.25 → 0.25 → 0.25**（10 秒 × 2 回、どちらも増分 **0.00**）。キャレットは前後の画面で同じ位置に描かれていて点滅していない。**陽性対照**: 同じ窓に 100 ms おきにキーを 10 秒投げると 0.25 → 1.125（**+0.875**）。だから 0 は読み取りであって、止まった計器ではない |

### `cargo test`

**ネイティブ ARM64 で 566 passed; 1 failed**（見つけたもの 1）。
設定ディレクトリを隔離すると **567 passed; 0 failed**（3.14s）。
x64 ランナーに無い失敗は **この 1 件だけ**で、それも ARM64 固有ではない。

### Proposals

#### 提案 1: `--keys` に「待たずに次を押す」書き方が要る

**踏んだこと**: 12.14 の `[running]` を捕まえるのに、`--keys "<Space>×5 d w"` では
**必ず `[done]` になってから** `w` が届く。`--keys` はキーの間で `App::settled()` を待つが、
`settled()` は ops のジョブを見ていない（`src/app.rs:1844`）ので、`d` のあとは
「一覧の読み直し」を待ち、その頃にはゴミ箱送りが終わっている。
結局 `keymap.toml` に `run = [ "remove", "tasks_show" ]` を 1 つ縛って回避した。

**どう変えるべきか**: `--keys` に `<Now>`（次のキーを settled を待たずに同じフレームで）か、
`<Keys:dw>` のような「ひと息で送る」括りを足す。`<Wait:0>` は既にあるが
**`<Wait:0>` でも settled は待つ**ので別物。

**なぜ**: 「進んでいる最中の表示」を確かめる行は TESTING.md にいくつもある
（12.14、44.16、12.3 の `Restore` 行）。今はどれも**運か回避策**で、無人の run では
再現しない日が出る。

**大きさ**: `--keys` のパーサに 1 トークン、送り手に分岐 1 つ。

#### 提案 2: spot のパネルで `j` / `k` がファイルを送ってしまう

**踏んだこと**: 46.21 で `From branch` の行にカーソルを下ろそうと `j` を 17 回押したら、
**パネルではなく一覧**が 17 行進み、spot は 17 個先のファイル（`TODO.md`）を映した。
パネルの行を動かすのは `<A-j>` / `<A-k>` だった。

**どう変えるべきか**: 最低限、spot の見出し `Spot: <name> — <Esc> to close` に
`<A-j>/<A-k> 行` と `<Enter> 開く` を足す。help のパネルは
`Keys — <Esc> close, j/k scroll, <A-j>/<A-k> half a page, …` と全部書いてある。
踏み込むなら、`Pull request` / `Came in via` / `From branch` を持つ板では `j` / `k` を
パネル側に寄せる（ファイル送りは `<Down>` / `<Up>` に残す）。

**なぜ**: v0.52.0 の `C`、v0.59.1 の `<Enter>` と、**パネルの行が操作対象になった**のに、
その行へ行く手段が見出しに書かれていない。この run で 1 回分まるごと無駄にした。

**大きさ**: 見出しの 1 行なら数分。キーの入れ替えは keymap とドキュメントの判断。

#### 提案 3: 「Open with」の板でも、動かし方を見出しに書く

**踏んだこと**: 32.10 で 2 つ目の opener を選ぼうと `j` を押したら、`type to filter` の欄に
`j` が入って候補が消えた。`<Down>` が正しかった。

**どう変えるべきか**: 見出しを `Open with — <Down>/<Up> choose, type to filter, <Enter> open` に。

**なぜ**: 1 行で済むし、spot と同じ取り違えがもう 1 か所ある。

**大きさ**: 1 行。

#### 提案 4: `filer env` の Config 節が、同じディレクトリを 2 回並べて警告も 2 回出す

**踏んだこと**: `YAZI_CONFIG_HOME` と `FILER_CONFIG_HOME` を同じ空ディレクトリに向けて
検証を隔離したら、Config 節が同じ行を 2 回出し、
`[term] belongs in filer.toml and was ignored` の警告まで 2 回出た。

**どう変えるべきか**: 2 つの設定ホームが同じパスなら 1 行にまとめ、
`(YAZI_CONFIG_HOME and FILER_CONFIG_HOME)` と添える。警告も重複を畳む。

**なぜ**: 「設定を隔離して何かを試す」のは**検証の定番**で、この run はそれ無しでは
見つけたもの 1 を切り分けられなかった。そのとき出力が倍になるのは、読みにくいだけでなく
「2 つ壊れている」と読める。

**大きさ**: 出力を組むところ 1 か所、数行。

#### 提案 5: 12.14 の行に「大きさ」を書き足してほしい

**踏んだこと**: 同じボリュームへのゴミ箱送りは**リネーム**なので、小さいファイル 5 つでは
40 ms ほどで終わる。`PrintWindow` の 1 コマが約 57 ms なので、`[running]` は原理的に捕まらない。
40 MB × 5 でも 120 MB × 5 でも駄目で、**400 MB × 5 でようやく 1 コマ**に入った。

**どう変えるべきか**: 12.14 の「Do」に
「five files big enough to take a moment (400 MB each worked on an NVMe)」と足す。

**なぜ**: 書いてなければ、次に押す人も同じ 30 分を使う。そして「捕まらなかった」を
「出なかった（不具合）」と取り違える余地がある。

**大きさ**: 1 行。この役割はドキュメントを直さないので提案にとどめる。

### 順番表（`.claude/windows-role.md`「The ARM64 lane」）

**マージする側で次を入れてからマージしてほしい。**入れないと、次の ARM64 の run が
また同じ 24 行を取る。

- **先頭の行「v0.57.3 to v0.59.5, the quiet keys | 24」を消す。**24 行すべてに手を付け、
  22 行が合格、13.17 だけが残った。
- 残った 13.17 は**昇格が要る行**なので、`.claude/windows-role.md` の
  「46.16 は still open … 45.11 も」の段落に **13.17 の `mklink /D` の半分**を足すのが収まりがよい。
- そのうえで、次に取る節は現在 2 番目の **「1. the terminal pane | the `[ ]` rows」**になる。
