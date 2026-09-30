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
