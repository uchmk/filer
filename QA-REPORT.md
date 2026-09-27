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
（`242 / 242 checked`）。再生成の必要なし。

### 番号の並び

1..46 に欠番・重複は無く、各 id は `<節>.<n>` になっている。ただし
**46 節（v0.47.0、spot パネルの Git セクション）が `## Known gaps in this checklist` の
後ろに置かれている。**「A new section goes wherever it reads best」に従えば 41 節の隣だが、
移すと 42 以降が全部繰り上がるので、末尾に足したのは本文の方針どおり。
ただ「Known gaps」は**文書のまとめ**なので、節はその手前に来るほうが読める。
番号を動かさずに直せる（46 節のブロックを Known gaps の直前へ移すだけ）ので、
そこだけ人の判断で。
