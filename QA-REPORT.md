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

## TESTING.md sections 12 / 13 / 24 — audited against 97dec91

3 節をまとめて `ui::harness::Screen` のテストにした。どれもファイル一覧まわりで、
下準備は既存の `panes::listing` を使い回している。

| 節 | テストモジュール | 自動化した項目 |
| --- | --- | --- |
| 12 | `ui::undo_frame`（`src/ui/mod.rs`、3 件） | 12.6 / 12.7 |
| 13（一覧） | `ui::link_rows`（`src/ui/mod.rs`、6 件） | 13.1 / 13.2 / 13.3 / 13.4 / 13.5 / 13.6 |
| 13（spot） | `ui::overlay::spot_link_section`（`src/ui/overlay.rs`、5 件） | 13.10 / 13.11 / 13.12 / 13.13 / 13.15 |
| 24 | `ui::awkward_names`（`src/ui/mod.rs`、2 件） | 24.1（一部）/ 24.3（一部） |

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

### TESTING-KEYS.md は同期している

`cargo +stable run --example make-keycheck -- --check` は exit 0
（`in sync with src/config/defaults/keymap.toml (242 / 242 checked)`）。再生成の必要なし。
