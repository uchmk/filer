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
