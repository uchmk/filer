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
