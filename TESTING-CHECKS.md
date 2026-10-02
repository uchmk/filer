# 実機チェックリスト

`TESTING.md` から `cargo run --example make-testcheck` で生成している。**正は TESTING.md**（英語）で、
このファイルはそれを日本語で並べ替えたもの。食い違ったら TESTING.md を信じること。各行の
末尾の *斜体* が TESTING.md の原文で、訳はその手前にある。

**チェック（`[x]`）だけは手で書いてよく、生成し直しても残る。**それ以外を書き換えても次の
生成で消える。

**322 / 448 済み。**（TESTING.md の全 617 件のうち、`cargo test` が見ている 169 件は
「押すもの」から外してある）

## 使い方

1. `filer.exe` と、`scripts\make-fixtures.ps1` が作るテスト用ファイルを用意する（詳しくは TESTING.md の
   「What you need」）。
2. 節ごとに「準備」を走らせてから、上から押していく。
3. 期待どおりなら `[ ]` を `[x]` にする。違ったら `<F12>` で issue を出すか、そのまま書き留める。
4. 節の見出しの `3 / 12` は、その節で人が押す分の進捗。

キーの網羅は別ファイル（[TESTING-KEYS.md](TESTING-KEYS.md)）で、こちらは「1 つのキーでは
確かめられない振る舞い」の側。

## 1. ターミナルペイン — 47 / 49

自動テスト済みなので下には出していない: 1.9i

準備:

```powershell
# `many\` と `repo` は make-fixtures.ps1 が作る
cd $HOME\Desktop\filer-fixtures
```

- [x] **1.1** `<C-t>` を一覧から押す → 下部にシェルが開き、一覧が表示しているディレクトリに既にいる — *`<C-t>` from the file list → A shell opens along the bottom, already in the directory the list is showing*
- [ ] **1.2** `dir` と打って Enter → 出力が一覧と同じフォントで、桁が揃い、字が重ならない — *Type `dir` and press Enter → Output in the list's own font, columns lined up, no overlapping glyphs*
- [x] **1.3** カーソルを見る → シェルのカーソル位置が四角く塗られ、打つと動く — *Look at the cursor → A block where the shell's cursor is, and it moves as you type*
- [x] **1.3a** `<C-t>` でキーを一覧に戻す → カーソルが**中抜き**になり、ペイン上端の線は枠の色のまま（フォーカスで緑にならない） — *`<C-t>` to give the keys back (v0.20.2) → The cursor goes **hollow**, and the rule along the top of the pane stays the plain border colour — it no longer turns green with focus*
- [x] **1.4** 色の出るものを実行（`repo` の中で `git status`）→ ANSI 16 色が出て、**一覧自身の配色と一致**する（別パレットに見えない） — *Run something colorful (`git status` in the `repo` fixture) → The 16 ANSI colors, and they match the file list's own colors rather than looking like a second palette*
- [x] **1.5** **もう一度 `<C-t>`** → キーが一覧に戻り、**シェルは生きたまま**出力も残っている（v0.6.0 の修正。以前はここでシェルが終了していた） — ***`<C-t>` again** → Keys go back to the list — **and the shell is still there**, with its output intact. This is the v0.6.0 fix; before it, this ended the shell*
- [x] **1.6** `<C-t>` を何度か往復 → 同じシェルのまま。スクロールバックが消えない — *`<C-t>`, `<C-t>`, `<C-t>` a few times → The same shell throughout. The scrollback never resets*
- [x] **1.7** `<C-S-t>` → **ここで**ペインが閉じ、シェルが終了する — *`<C-S-t>` → *Now* the pane closes and the shell ends*
- [ ] **1.8** 開き直してウィンドウをリサイズ → 桁が組み直される。半端に切れた列も、引き伸ばされた字も無い — *Reopen, then resize the window → The grid reflows; no clipped half-columns, no stretched text*
- [x] **1.9** `many\` で `dir` して画面を埋め、`<S-PageUp>` → **本文が動く**（v0.20.3 まで注記だけが動き、スクロールしていない画面に「N 行前」と出ていた） — *`dir` in `many\` to fill the screen, then `<S-PageUp>` → **The text moves.** Until v0.20.3 only the note moved — it said "N lines back" over a screen that had not scrolled*
- [x] **1.9a** `<S-PageUp>` / `<S-PageDown>` → 半画面ずつ戻る / 進む（v0.20.4 まで符号が逆で、`<S-PageUp>` が下を向いて何も起きなかった） — *`<S-PageUp>` / `<S-PageDown>` (v0.20.4) → Half a screen back / forward. Until v0.20.4 the sign was inverted, so `<S-PageUp>` aimed at the bottom and did nothing*
- [x] **1.9b** `<S-Home>`、`<S-End>` → 最古の行と、プロンプト — *`<S-Home>`, `<S-End>` → The oldest line held, and the prompt. These worked before — no sign to get wrong*
- [x] **1.9d** ペインの上でホイール → 1 ノッチずつ滑らかに動く（以前は 1〜2 行動かすのに強く回す必要があった） — *The mouse wheel over the pane (v0.20.4) → Moves smoothly, a notch at a time. It used to need spinning hard for one or two lines*
- [x] **1.9e** プロンプトが画面から出るまで戻る → カーソルも一緒に消える（元の高さに四角が取り残されない） — *Scroll back far enough that the prompt leaves the screen → The cursor goes with it — no block left behind at its old height*
- [x] **1.9c** `<C-S-f>` でスクロールバックのずっと上にある語を検索、Enter → そこへ飛び、**一致箇所が強調される** — *`<C-S-f>` for a word far up the scrollback, Enter → The view jumps to the match **and the match is highlighted***
- [x] **1.9f** `<C-S-f>` で**いま画面に出ている**語を検索 → 履歴の古いほうではなく、画面のものが先に見つかる — *`<C-S-f>` for a word that is on screen right now (v0.20.4) → The one on screen is found first, not an older one up in the history*
- [x] **1.9g** そのあと `<C-S-n>` / `<C-S-b>` → `<C-S-n>` で履歴の上へ、`<C-S-b>` で下へ戻る — *`<C-S-n>` / `<C-S-b>` after that → `<C-S-n>` walks further up into the history, `<C-S-b>` comes back down*
- [x] **1.9h** `<C-S-f>` で存在しない語を検索 → 赤いトーストで「無い」と言う（無反応ではない） — *`<C-S-f>` for something that is not there → A red toast saying so — not silence*
- [x] **1.10** `<S-End>` のあと何か 1 文字打つ → 最下部に戻る。打つだけでも戻ることの確認 — *`<S-End>`, then type a character → Back at the bottom, and typing alone would have done it*
- [x] **1.11** 出力の上をドラッグ → **ドラッグ中から選択が描かれ**、離すとクリップボードに入る（v0.20.4 まではコピーは効くのに何も描かれなかった） — *Drag across some output (v0.20.4) → **It highlights as you drag**, and is on the clipboard when you let go. Before v0.20.4 the copy worked and nothing was drawn*
- [x] **1.11a** 同じ範囲を**右から左へ**ドラッグ → 1 文字も違わず同じ文字列（v0.26.4 まで逆向きは**両端で 1 文字ずつ**落ちていた） — *Drag **right to left** over the same run of text (v0.26.4) → The same text, character for character. Until v0.26.4 a backwards drag lost one at **each** end*
- [x] **1.11b** 最初の文字の**左の隙間ではなく、文字の上から**ドラッグを始める → その文字が含まれる（以前は隙間から始めないと落ちた） — *Start the drag **on** the first character, not to its left (v0.26.4) → It is included. It used to be dropped unless the drag began in the gap before it*
- [x] **1.11c** 文字の**右半分**からドラッグを始める → その文字は含まれない（これが正しく、1.11a が成り立つのと同じ規則） — *Drag from the right half of a character → That character is left out — correct, and the same rule that makes 1.11a work*
- [x] **1.12** 単語をダブルクリック → 単語が選択され、それが目に見える — *Double-click a word → The word is selected, and visibly so*
- [x] **1.13** `<C-S-f>` でスクロールバックの語を検索、Enter、そのあと `<C-S-n>` → 次々に移動でき、末尾で先頭に回り込む — *`<C-S-f>`, type a word from the scrollback, Enter, then `<C-S-n>` → Matches are found and stepped through; it wraps at the end*
- [x] **1.14** ターミナルの中で `<F1>` → キー一覧がターミナルの**上に**開く。`<Esc>` で閉じ、入力がシェルに戻る — *`<F1>` inside the terminal → The key list opens **over** the terminal. `<Esc>` closes it and typing goes back to the shell*
- [x] **1.15** ターミナルの中で `<C-S-p>` → コマンドパレットが開き、そこから実行できる — *`<C-S-p>` inside the terminal → The command palette opens, and running something from it works*
- [x] **1.16** シェルで `cd` してから `<A-Up>` → 一覧がシェルのいる場所へ追従する — *`cd` somewhere in the shell, then `<A-Up>` → The file list follows to where the shell is*
- [x] **1.17** 2 つ選んで `<A-t>` → パスが引用符付きでシェルの行に打ち込まれる。**実行はされない** — *Select two files, `<A-t>` → Their paths are typed onto the shell's line, quoted, **not run***
- [x] **1.18** OSC 7 を報告するシェル（PowerShell 7、または `PROMPT_COMMAND` を設定した bash）で、一覧側のディレクトリを変える → シェルに余計な `cd` が打ち込まれない — *With a shell that reports OSC 7 (PowerShell 7, or bash with a `PROMPT_COMMAND`), change directory in the list → No stray `cd` is typed into the shell*
- [x] **1.19** ペインで時間のかかるもの（`sleep 30`）を走らせて `<C-c>` を押す（v0.47.34） → コマンドが止まってプロンプトが戻る。**filer は開いたまま。**v0.47.34 までは `[mgr]` の `close` が走り、タブが閉じ、最後の 1 枚なら filer ごと終了してシェルも道連れになっていた — *Run something slow in the pane (`sleep 30`) and press `<C-c>` (v0.47.34) → The command stops and the prompt comes back. **filer is still open** — until v0.47.34 this ran `[mgr]` `close`, so the tab went and the last one took filer and the shell with it*
- [x] **1.20** 名前に `'` を含むファイルで `<A-t>`、`[term] shell` で指定できる各シェルについて（v0.47.34）→ シェルが 1 語として読める形で入る。PowerShell なら `''` と重ね、bash なら `'\''`、cmd なら素の `"…"`。**`>>` の継続プロンプトにならない** — *`<A-t>` on a file with a `'` in its name, in each shell `[term] shell` can name (v0.47.34) → The line is one word the shell can read: `''` doubled for PowerShell, `'\''` for bash, plain `"…"` for cmd. **No `>>` continuation prompt***
- [x] **1.21** ペインを開いたまま、名前に `'` を含むディレクトリへ一覧を移動する（v0.47.34） → `cd` が通ってプロンプトが戻る。1.20 と同じ引用が、filer 自身が打つパスにも効いていること — *Walk the list into a directory with a `'` in its name, with the pane open (v0.47.34) → The `cd` lands and the prompt returns. The same quoting as 1.20, on the path filer types for itself*
- [x] **1.22** `[term] shell` を Git Bash のフルパスにして、ペインを開いたまま **普通の**ディレクトリ（`'` も空白も無い）へ一覧を移動する → `cd` が着いて、プロンプトがそのディレクトリになる。**`bash: cd: R:Tempfiler-fixtures: No such file or directory` にならないこと** —— 引用の外の `\` は POSIX シェルではエスケープなので、v0.48.1 まで普通の Windows パスには一切入れなかった。1.20 と 1.21 はどちらも `'` のある名前を指定しているので、これを見落としていた — *With `[term] shell` set to Git Bash's full path, walk the list into an **ordinary** directory — no `'`, no space (v0.48.1) → The `cd` lands and the prompt is in that directory. **Not `bash: cd: R:Tempfiler-fixtures: No such file or directory`** — an unquoted `\` is an escape to a POSIX shell, so until v0.48.1 no ordinary Windows path could be walked into at all. 1.20 and 1.21 both name a `'`, which is why they missed it*
- [x] **1.23** 同じシェルで、**普通の**名前のファイルに `<A-t>` → パスがバックスラッシュごとそのまま入る。1.22 と `quote()` を共有していて、これまで推定されただけで一度も押されていない — *The same shell, `<A-t>` on a file with an **ordinary** name (v0.48.1) → The path arrives whole, backslashes and all. This shares `quote()` with 1.22 and was only ever inferred from it, never pressed*
- [x] **1.24** ペインの中で全画面 TUI を走らせる（`gh dash`、または `lazygit`）（v0.48.2） → ちゃんと描ける。代替画面・色・罫線・TUI 自身の分割ペイン。**一度は見えている**ので、この行はそれを見続けるためのもの — *In the pane, run a full-screen TUI — `gh dash`, or `lazygit` (v0.48.2) → It draws: alternate screen, colours, box drawing, its own split panes. Seen once already; this row is for keeping it seen*
- [x] **1.25** その TUI を操作して、抜ける（`j` / `k` で移動、`q` で終了） → キーが届き、抜けるとペインが返ってきてプロンプトが使える。**描けることと操作できることは別の主張で、1.24 が通ってもこちらは何も言えない。**抜けられない TUI はペインを道連れにする — *Drive that TUI, then quit it (`j` / `k` to move, `q` to leave) → The keys reach it, and quitting gives the pane back with a working prompt. **Drawing and driving are separate claims** — 1.24 passing says nothing about this one, and a TUI that cannot be left would strand the pane*
- [x] **1.26** ペインを開いた状態で `<C-S-Enter>`（v0.48.4） → ペインが**上端まで**窓を取る。ヘッダも一覧も無く、下にステータスバーだけ。高さの 3 分の 1 はシェルには妥当で、全画面のプログラムには足りない — *With the pane open, `<C-S-Enter>` (v0.48.4) → The pane takes the window **to the top edge** — no header, no list, only the status bar below it. A third of the height is right for a shell and too little for a full-screen program*
- [x] **1.27** TUI を走らせた状態のペインの中から、もう一度 `<C-S-Enter>` → 3 分の 1 に戻る。**ターミナルがキーを持っている間に効くこと**が要点 —— TUI はまさにその状態を作るし、この機能が要るのもそこだけ — *`<C-S-Enter>` again, from inside the pane, with a TUI running in it → Back to a third. The key has to work **while the terminal holds the keys** — that is the state a TUI puts you in, and the only one where this matters*
- [x] **1.28** **一覧側**から最大化して、何か打つ → 打鍵はペインに行く（隠れた一覧ではなく）。最大化はペインにキーを渡す —— 見えない一覧は、キーを向ける先ではないので — *Maximise from the **list** side, then type → The keystrokes go to the pane, not to the hidden list. Maximising hands the pane the keys, because a list nobody can see is not somewhere to aim them*
- [x] **1.29** 最大化してから `<C-t>` → キーが一覧に戻り、**同時にペインも 3 分の 1 に戻る**。1 回の押下で両方。ペインから出ることと窓を返すことは同じ意図 — *Maximise, then `<C-t>` → The keys go back to the list **and the pane returns to a third** in one press. Leaving the pane and giving the window back are the same intent*
- [x] **1.30** 最大化してから `<C-S-t>`（シェルを終わらせる） → ペインが消え、一覧が全高で描かれる（隙間の下に押し込まれない）。ペインの無い最大化が残らないこと — *Maximise, then `<C-S-t>` (end the shell) → The pane goes, and the list is drawn full height rather than under a gap. Nothing is left maximised with no pane in it*
- [x] **1.31** ペインで `lazygit` → `?` でキー一覧を開く → `Esc`（v0.48.6） → **一覧が閉じる。**v0.48.6 までは何度押しても閉じなかった（gh-dash など tcell のプログラムすべて同じ）。Windows Terminal が対照で、あちらでは最初から閉じる — *In the pane, `lazygit`, then `?` to open its key list, then `Esc` (v0.48.6) → **The list closes.** Until v0.48.6 it never did, however often `Esc` was pressed — the same in gh-dash, or any tcell program. Windows Terminal is the control: it has always closed there*
- [x] **1.32** ペインの pwsh のプロンプトで `abc` と打ち（Enter は押さない）、`Esc` → 行が空になる。PSReadLine は元々影響を受けていなかった。1.31 を直した変更がこちらを壊していないことを見る行 — *At the pwsh prompt in the pane, type `abc` without Enter, then `Esc` → The line empties. PSReadLine was never affected; this row is there so the change that fixed 1.31 is seen not to have broken it*
- [x] **1.33** 同梱の ConPTY を filer.exe の横に置いた状態で、ペインで `pwsh -File scripts\keyprobe.ps1 -Query`（v0.49.0） → DA1 の応答が `\e[?6c`（filer 自身の答えがそのまま通ったもの）で、文字の間に `{up:…}` が出ない。`\e[?61;6;7;22;23;24;28;32;42c` なら Windows 標準の ConPTY が答えている —— 2 つのファイルが無いか、filer.exe の横に無い — *With the bundled ConPTY beside filer.exe, `pwsh -File scripts\keyprobe.ps1 -Query` in the pane (v0.49.0) → The primary DA reply reads `\e[?6c` — filer's own answer, passed through — and no `{up:…}` appears between characters. `\e[?61;6;7;22;23;24;28;32;42c` means the ConPTY built into Windows answered instead: the two files are missing, or not beside filer.exe*
- [x] **1.34** 続けてペインで `lazygit` → いつもの画面で開き、**メニューが開いていない。**Windows 標準の ConPTY では、押していないキーでコピー画面が開いた状態で始まっていた — *Then `lazygit` in the pane → It opens on its usual view with **no menu open**. On the ConPTY built into Windows it started with its copy menu showing, a key nobody pressed*
- [x] **1.35** ペインで `lazygit`（または長く動くコマンド）を動かしたまま `<C-S-t>`（v0.52.0）→ **End the shell?** と聞かれ、動いているものの名前が出る。`n` でシェルもプログラムも残り、`y` で両方終わって **Ended the shell** とトーストが出る — *Run `lazygit` (or any long command) in the pane, then `<C-S-t>` (v0.52.0) → A dialog asks **End the shell?** and names what is running. `n` keeps the shell and the program; `y` ends both, and a toast says **Ended the shell***
- [x] **1.36** 何も動いていないプロンプトで `<C-S-t>` → **確認は出ない。**ペインはすぐ消え、トーストに **Ended the shell** と出る。`<C-t>` で隠しただけのようには見えなくなった — *At a bare prompt with nothing running, `<C-S-t>` → **No dialog**: the pane goes at once, and the toast says **Ended the shell** — so it no longer looks like `<C-t>` merely hiding it*
- [x] **1.37** ペインを**閉じた**まま、ファイルを選んで `<A-t>`（v0.57.0）→ ペインが開き、シェルのプロンプトが出たところで引用されたパスが行に入る。「The terminal is not open」とは言わず、プロファイル読み込み中のシェルに打って消えることもない。キーはペインに移る — *With the pane **closed**, select a file and `<A-t>` (v0.57.0) → The pane opens and, once the shell's prompt is up, the quoted path is on its line -- not "The terminal is not open", and not lost to a shell still loading its profile. The keys are in the pane*

## 2. ミニマップ — 0 / 10

`long.rs`（4000 行）を開く。帯の形が元のファイルに見えるかどうかが本題。

自動テスト済みなので下には出していない: 2.1, 2.8

準備:

```powershell
cd $HOME\Desktop\filer-fixtures
# long.rs は 4000 行。コメント見出し・インデント・40 行ごとの空行で形が出るように作ってある
```

- [ ] **2.2** 形を見る → コメント見出しが長い帯、インデントされた塊は右から始まる帯、40 行ごとの空行が隙間。**元のファイルの形に見えること** — *Look at the shape → Comment headers read as long bars, indented blocks as bars starting further right, the blank line every 40 as a gap. It should look like the file*
- [ ] **2.3** 色を見る → 帯に構文色が乗っている（文字列とコメントがコードと違う色）。単色の塗りつぶしではない — *Look at the colors → The bars carry syntax colors — strings and comments differ from code — not one flat color*
- [ ] **2.4** ビューポートの枠を探す → 画面に出ている範囲を覆う、明るめの枠線付きの箱がある — *Find the viewport box → A lighter box with a border, covering the part of the file on screen*
- [ ] **2.5** `<A-j>` を数回 → 箱が本文と同じだけ下がる — *`<A-j>` a few times → The box moves down in step with the text*
- [ ] **2.6** 帯の中ほどをクリック → プレビューがそこへ飛び、**クリックした行がペインの中央**に来る（上端ではない） — *Click halfway down the strip → The preview jumps there, with the clicked line in the **middle** of the pane, not at its top*
- [ ] **2.7** 帯の上を上下にドラッグ → プレビューが連続して追従する — *Drag up and down the strip → The preview follows continuously*
- [ ] **2.9** `<A-n>` → ミニマップが消え、もう一度で戻る — *`<A-n>` → The map toggles off and on*
- [ ] **2.10** `notes.md` を（レンダリング表示で）開く → **ミニマップは出ない。**これは意図的で、描画された行はファイルの行と一致しないため — *Open `notes.md` (rendered) → **No map** — this is deliberate, the rendered lines are not the file's lines*
- [ ] **2.11** `M` でソース表示にする → ミニマップが出る — *Press `M` for source → The map appears*
- [ ] **2.12** 短いファイル（`same-a.txt`）→ 出ない。2 行を地図にしても意味がない — *A short file (`same-a.txt`) → No map: two lines are not worth mapping*

## 3. 画像の拡大と移動 — 0 / 10

`zoom-me.png` は 3200×2400 で 8px のグリッド入り。ぼけたらすぐ分かる。

準備:

```powershell
cd $HOME\Desktop\filer-fixtures
# zoom-me.png（3200x2400、グリッド入り）と tiny.png（48x48）を使う
```

- [ ] **3.1** `zoom-me.png` をホバー → ペインに収まる。説明に `3200 × 2400 · fit NN%` と出る — *Hover `zoom-me.png` → It fits the pane. The caption reads `3200 × 2400 · fit NN%`*
- [ ] **3.2** `<A-1>`（等倍）→ ペインよりはるかに大きくなり、中央部分が見える。**グリッド線がくっきりしている**こと（再デコードが効いている証拠。収めた画像を引き伸ばしたぼけた絵なら、それがこの機能の避けたかったバグ） — *`<A-1>` (1:1) → It fills far more than the pane, showing the middle. **The grid lines are crisp** — this is the re-decode working; if it is a blurred enlargement of the fitted copy, that is the bug this was built to avoid*
- [ ] **3.3** くっきりする瞬間を見る → **画像が飛んだり大きさが変わったりしてはいけない。**変わるのは鮮明さだけ — *Watch the moment it sharpens → The picture must **not jump or change size** when the sharper copy arrives. Only its sharpness changes*
- [ ] **3.4** ドラッグする → 移動でき、端がペインの端に来たら止まる。画面外へ放り出せない — *Drag it → It pans, and stops when its edge reaches the pane's edge — it cannot be thrown off screen*
- [ ] **3.5** グリッドの交点にポインタを置いて `Ctrl`+ホイール → **ポインタを中心に**拡大縮小し、カーソル下の交点がその位置に留まる — *`Ctrl` and the wheel, pointer on a grid intersection → It zooms **about the pointer**: the intersection under the cursor stays under it*
- [ ] **3.6** Ctrl 無しのホイール → ペインがスクロールする。拡大はしない — *Plain wheel (no Ctrl) → Scrolls the pane, does not zoom*
- [ ] **3.7** ダブルクリック → 収まる大きさに戻り、中央に来る — *Double-click → Back to fitting, centred*
- [ ] **3.8** `<A-i>` / `<A-o>` → 段階的に拡大縮小。説明のパーセント表示も追従する — *`<A-i>` / `<A-o>` → In and out in steps. The caption's percentage follows*
- [ ] **3.9** 拡大してから `j` で次のファイルへ行き、戻る → また収まった状態になっている（拡大率はファイルごとのもの） — *Zoom in, then `j` to the next file and back → It is fitted again — a zoom belongs to the file it was set on*
- [ ] **3.10** `tiny.png`（48×48）をホバー → 元の大きさのまま表示される。**ペインを埋めるように引き伸ばされない** — *Hover `tiny.png` (48×48) → Shown at its own size, **not blown up** to fill the pane*

## 4. SVG と、その中の文字 — 0 / 8

**fixtures には SVG が入っていない。**手持ちの `.svg` で構わないが、Inkscape や Illustrator で
保存したものが良い（手書きの単純なものより、実際に使う形に近い）。文字入り・日本語入り・
太字斜体入りの 3 つが要る。**4.8 だけは v0.33.5 のビルドが要るので、今は飛ばしてよい。**

- [ ] **4.1** 文字の入っていない SVG（アイコンやロゴ）をホバー → 描画され、ペインに合わせて拡大され、どの大きさでもくっきりしている — *Hover an SVG with no text in it (an icon, a logo) → Drawn, scaled to fill the pane, sharp at any pane size*
- [ ] **4.2** **文字の入った** SVG → 文字が正しい位置に正しい大きさで描かれる。**消えていない・豆腐になっていない・重なっていない** — *An SVG containing **text** → The text is drawn, in the right place, at the right size — **not missing, not boxes, not overlapping***
- [ ] **4.3** **日本語**の入った SVG → 同じ。かなと漢字のあるフォントが選ばれる（文字が消えない） — *An SVG with **Japanese** text → Same. A font with kana and kanji is picked, rather than the text vanishing*
- [ ] **4.4** **インストールされていない**フォントを指定した SVG → 代替フォントが使われて読める形で出る。描画全体が失敗しない — *An SVG naming a font that is **not installed** → A fallback is used and something readable appears; it does not fail the whole render*
- [ ] **4.5** **太字**や*斜体*の文字が入った SVG → 太さと傾きが反映される（普通の字に潰れない） — *An SVG with **bold** or *italic* text → The weight and slant are there, not flattened to regular*
- [ ] **4.6** システムフォントではなく**隣に置いたフォントファイル**を使う SVG → そのディレクトリから読まれる（`resources_dir` の意図どおり） — *An SVG using a font **file next to it** rather than a system font → Loaded from the directory, as `resources_dir` intends*
- [ ] **4.7** **壊れた** SVG（途中で切ったもの）→ プレビューに `bad SVG: …` と出て、ウィンドウは動き続ける — *A **malformed** SVG (truncate one) → `bad SVG: …` on the preview, and the window keeps working*
- [ ] **4.8** 4.2 と 4.3 を v0.33.5 のビルドと見比べる → 字形の違いがあれば新しいシェイパーの影響。何が変わったかを書いて両方添える（**v0.33.5 のビルドが要るので、無ければ飛ばす**） — *Compare 4.2 and 4.3 against v0.33.5's build → Any difference in the glyphs is the new shaper; say what changed and attach both*

## 5. 2 ファイルの差分表示 — 0 / 2

自動テスト済みなので下には出していない: 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 5.6a, 5.6b, 5.6c, 5.7, 5.8, 5.10

- [ ] **5.9** 2 つの**ディレクトリ**を選んで実行 → 理由を添えて断られる — *Two directories → Refused with a reason*
- [ ] **5.11** 1 語だけ変わった行（`price` → `cost`）と、日本語の語が変わった行（`太郎` → `花子`）のある 2 つのファイルを比べる（v0.62.0）→ 変わった行では、その語だけが濃く塗られる（左は赤、右は緑）。塗りは語の真下にぴったり重なる（日本語の語も）。全部変わった行は行の色だけ — *Compare two files where one line changes a single word (`price` → `cost`), and another a Japanese word (`太郎` → `花子`) (v0.62.0) → On each changed row only that word is painted stronger, red on the left and green on the right, and the mark sits exactly under the word -- the Japanese one too. A line changed completely keeps only the row tint*

## 6. 2 分割ペインと、ペイン間の受け渡し — 0 / 1

自動テスト済みなので下には出していない: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7, 6.8, 6.9, 6.10, 6.11, 6.12, 6.13, 6.14

- [ ] **6.15** 大きなディレクトリを `<A-c>` して、ステータスバーを見る → 他のコピーと同じジョブ扱い。進捗と速度が出て、`w` から中止できる — *`<A-c>` a large directory, then watch the status bar → It is a job like any other copy: progress, speed, and cancellable from `w`*

## 7. ヘルプパネルの設定ファイルパス — 8 / 8

`~` か `<F1>` でヘルプを開く。上部の設定パスの並びが対象。

- [x] **7.1** どこにも `filer.toml` が無い状態で `~` → **両方の**ディレクトリが並び、空のほうに `nothing here` が付く（v0.25.0 までは存在するファイルしか出なかった） — *`~` with no `filer.toml` anywhere → **Both** directories are listed, the empty one marked `nothing here`. Before v0.25.0 only files that existed were shown*
- [x] **7.2** パスの行をホバー → 行が光り、ポインタが手の形になる — *Hover a path → The row lights up and the pointer becomes a hand*
- [x] **7.3** キーの行をホバー → 何も起きない。そこはリンクではない — *Hover a key row → Nothing happens — it is not a link*
- [x] **7.4** 設定**ファイル**をクリック → パネルが閉じ、そのファイルにカーソルを置いた状態でディレクトリが開く。`<Enter>` でそのまま開ける — *Click a config **file** → The panel closes, the list opens its directory with that file under the cursor. `<Enter>` then opens it*
- [x] **7.5** **ディレクトリ**をクリック → パネルが閉じ、空でもそこへ移動する — *Click a **directory** → The panel closes and the list goes there, empty or not*
- [x] **7.6** 空のほうをクリックし、そこに `filer.toml` を作って `<C-F5>` → 次に開いたとき `nothing here` が消えている — *Click the empty one, then create `filer.toml` there and `<C-F5>` → It appears in the panel next time, without `nothing here`*
- [x] **7.7** `YAZI_CONFIG_HOME` / `FILER_CONFIG_HOME` を設定した状態で → 並ぶディレクトリがそれに従う — *With `YAZI_CONFIG_HOME` / `FILER_CONFIG_HOME` set → The listed directories follow them*
- [x] **7.8** 設定の警告行 → 黄色のまま。クリックできない — *A config warning line → Still yellow, and not clickable*

## 8. ターミナルペインが起動するシェル — 7 / 7

**`[term]` は `filer.toml` に書く。**`yazi.toml` に書いても黙って無視される
（v0.33.13 でそれを警告するようにした）。設定を変えたら `<C-S-t>` でシェルを終わらせてから
`<C-t>` で開き直すこと。`<C-t>` の往復では同じシェルが生き続ける（1.5 / 1.6）。

- [x] **8.1** `filer.toml` に `[term]` が無い状態で `<C-t>`、`$PSVersionTable.PSVersion` → `7.x`（`pwsh`。v0.55.0 から、入っていればこれが既定。Q29）。`pwsh` の無い機械でだけ `5.1.x`（Windows PowerShell） — *`<C-t>` with no `[term]` in `filer.toml`, then `$PSVersionTable.PSVersion` → `7.x` — `pwsh`, the default since v0.55.0 wherever it is installed (Q29). `5.1.x`, Windows PowerShell, only on a machine without `pwsh`*
- [x] **8.2** ペインを開いたまま `[term]` / `shell = "powershell"` を足し、`<C-F5>`、続けて `<C-S-t>` → `<C-t>` → もう一度聞く（v0.67.17）→ `<C-F5>` のトーストの末尾が `— the pane keeps its shell until <C-S-t> closes it`。`<C-S-t>` `<C-t>` のあとは `5.1.x` — *With the pane open, add `[term]` / `shell = "powershell"`, `<C-F5>`, then `<C-S-t>`, `<C-t>`, ask again (v0.67.17) → The `<C-F5>` toast ends `— the pane keeps its shell until <C-S-t> closes it`; after `<C-S-t>` `<C-t>`, `5.1.x`*
- [x] **8.3** それぞれで `$PROFILE` → 別のパスが出る（5.1 は `WindowsPowerShell\`、7 は `PowerShell\`） — *`$PROFILE` in each → Two different paths — `WindowsPowerShell\` for 5.1, `PowerShell\` for 7*
- [x] **8.4** OSC 7 のフックを pwsh のプロファイルにだけ入れて、それぞれで `cd` して `<A-Up>` → pwsh では追従し、5.1 ではその旨が出る。**この非対称そのものが報告の中身** — *With the OSC 7 hook in the pwsh profile only, `cd` and `<A-Up>` under each → Works under `pwsh`, and says so under 5.1. That asymmetry is the whole bug report*
- [x] **8.5** `args = ["-NoLogo"]` → 起動時のバナーが消える — *`args = ["-NoLogo"]` → The banner is gone*
- [x] **8.6** インストールされていない `shell` を指定 → 起動に失敗し、その旨が出る（無言の空ペインにならない） — *A `shell` that is not installed → It fails to start and says so — no silent empty pane*
- [x] **8.7** `[term]` を消して `<C-F5>` → `<C-S-t>` → `<C-t>`（v0.67.17）→ 既定に戻る（`pwsh` が入っていれば `7.x`）。`<C-F5>` を抜くと、`<C-S-t>` `<C-t>` は前のシェルをまた起動する（誰もファイルを読み直していない） — *Remove `[term]` again, `<C-F5>`, `<C-S-t>`, `<C-t>` (v0.67.17) → Back to the default (`7.x` where `pwsh` is installed). Without the `<C-F5>`, `<C-S-t>` `<C-t>` starts the old shell again: nothing has re-read the file*

## 9. ファイル末尾のアウトライン — 全 5 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 10. ヤンクレジスタ（コピー・カット）の表示 — 0 / 1

残っているのは色そのものの見え方。緑 / 黄 / 赤に読めるか、3px の帯が気づける太さかどうか。

自動テスト済みなので下には出していない: 10.1, 10.2, 10.3, 10.4, 10.5, 10.6, 10.7, 10.8, 10.10

- [ ] **10.9** ファイルを `x` で切り取り、同名のファイルが既にあるディレクトリで `p` して、上書きに**いいえ**と答える → **それでもヘッダから件数が消える。**`paste()` はジョブを**投入した時点で**切り取りレジスタを空にしていて、成功した時点ではないため。結果、ファイルは移動もされず、レジスタにも残らない — *Cut a file, then `p` into a directory that already holds that name, and answer **no** to the overwrite → The count still leaves the header — `paste()` empties a cut register when it *submits* the job, not when the job succeeds, so the files are neither moved nor still in the register*

## 11. 一括リネーム — 全 12 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 12. undo と redo — 14 / 16

自動テスト済みなので下には出していない: 12.6, 12.7

- [x] **12.1** `many\` の中のファイルで `d` → ごみ箱に入る — *`d` on a file in `many\` → It goes to the recycle bin*
- [x] **12.2** `u` → 元の場所に戻る。トーストがそう言う — *`u` → It comes back, in its original place. A toast says so*
- [x] **12.3** （F2 の最中に）タスクパネル `w` を見る → `Restore` の行が現れて完了する — *Check the task panel (`w`) during F2 → A `Restore` row appears and completes*
- [x] **12.4** `U` → もう一度削除される — *`U` → Deleted again*
- [x] **12.5** 別々のフォルダにある同名のファイルを、時間を空けて 2 つ削除してから `u` → **直前に消したほう**が戻る（古いほうではない） — *Delete two files with the same name from different folders, an interval apart, then `u` → The one just deleted comes back — not the older one*
- [ ] **12.8** ファイルを `r` で改名 → `u` で戻す → **別の**ファイルを改名 → `U` → **redo は消えている。**新しい改名が履歴を分岐させたため。ファイルの作成は取り消しの手順を残さないので、新しいファイルを作っても redo は残る（#83） — *Rename a file, undo it, then rename **another** file, then `U` → Redo is gone: the new rename forked history. Creating a file records no undo step, so a new file leaves the redo in place (#83)*
- [x] **12.9** ファイルを `d` で消し、**同じ名前のファイルを先に作ってから** `u` → `u` が「その名前は使われている」と名前を挙げて断り、**取り消しの手順は残る。**邪魔なファイルをどけてもう一度押すと通る — *Delete a file, `u`, but create a file with that name first → `u` says the name is taken, and pressing it again after moving that file out of the way works*
- [x] **12.10** 別のプログラムで開いてロックされたファイルを、**他の数件と一緒に**選んで `d` → 他は消える。メッセージが**消えなかったファイルの名前を挙げ**、タスクパネルの件数も実際に消えた数と合う（v0.27.1 まで `Trash: trash: Error … Some operations were aborted` と名前を挙げずに言い、全件成功として数えていた） — *Open a file in another program so it is locked, select it **with several others**, `d` (v0.27.1) → The others go. The message **names the one that did not**, and the task panel's count matches what actually went. Until v0.27.1 it said `Trash: trash: Error … Some operations were aborted` naming nothing, and counted them all as done*
- [ ] **12.11** ごみ箱を無効にしたドライブで `d` → 同じ形のメッセージが、ファイル名を挙げて出る — *`d` on a drive whose Recycle Bin is turned off → Same shape of message, naming the file*
- [x] **12.12** ロックされたファイルが無い状態で `d` → 以前のまま。そしてエクスプローラー自身の取り消し履歴に**項目が 1 つだけ**残る（まとめて渡す呼び出しが通常の経路であることの確認） — *`d` with nothing locked → Unchanged, and still **one** entry in Explorer's own undo — the batch call is still the normal path*
- [x] **12.13** 1 つのファイルで `d`、続けて 2 つで `d`（v0.57.3）→ そのたびにトーストが出る: `Trashed <名前> — u to undo`、次に `Trashed 2 item(s) — u to undo`。v0.57.3 まで `d` は何も言わず、`D` と見分けがつかなかった — *`d` on one file, then on two (v0.57.3) → A toast each time: `Trashed <name> — u to undo`, then `Trashed 2 item(s) — u to undo`. Until v0.57.3 `d` said nothing, so it looked the same as `D`*
- [x] **12.14** 5 つのファイルで `d` し、走っている間に `w`（v0.58.1）→ 行は `Trash 5 item(s)  [running]`（動詞は **1 回**）、その下の行は `0/5 files` で、`0 B / 0 B` は出ない — *`d` on five files, and `w` while it runs (v0.58.1) → The row reads `Trash 5 item(s)  [running]` -- the verb **once** -- and the line under it `0/5 files`, with no `0 B / 0 B`*
- [x] **12.15** 12.10 と同じく、5 件のうち 1 件を別のプログラムで開いたまま 5 件を `d`、続けて `u`（v0.59.7）→ エラーが開いているファイルを `it is open in another program` と名指しし、`u` で入った **4 件**が戻る。v0.59.7 までは `Some operations were aborted` とだけ言い、`u` は `Nothing to undo` だった — *As 12.10 -- one file of five held open elsewhere, `d` on all five -- then `u` (v0.59.7) → The error names the held file with `it is open in another program`, and `u` brings back the **four** that went. Until v0.59.7 the error said only `Some operations were aborted` and `u` said `Nothing to undo`*
- [x] **12.16** 12.9 をもう一度: ファイルを `d`、同じ名前のファイルを新しく作り、`u`（v0.59.7）→ エラーが `RestoreCollision { … TrashItem { id: "C:\$Recycle.Bin…` ではなく `a file by that name is already there. Move it away and press u again` と言う。新しいファイルをどけてからの `u` は通る — *12.9 again: `d` a file, make a new file by that name, then `u` (v0.59.7) → The error reads `a file by that name is already there. Move it away and press u again` -- not `RestoreCollision { … TrashItem { id: "C:\$Recycle.Bin…` -- and after moving the new file away, `u` works*
- [x] **12.17** `a` で `new/deep/note.txt` を作り、`u`、続けて `U`（v0.60.0）→ `u` で `note.txt` と、そのために作った 2 つのフォルダが消え、トーストは `Removed note.txt and 2 folder(s)`（v0.67.10。それまでは `Removed note.txt` で、フォルダのことを言わなかった）。`U` で 3 つとも戻る。ファイルに何か書いてから `u` すると残り、書き込まれたというエラーが出る — *`a`, type `new/deep/note.txt`, `<Enter>`, then `u`; then `U` (v0.60.0) → `u` removes `note.txt` and both folders made for it, toast `Removed note.txt and 2 folder(s)` (v0.67.10; before, `Removed note.txt` said nothing of the folders); `U` makes all three again. Write something into the file and press `u`: it stays, and the error says it has been written to since*
- [x] **12.18** ファイルをヤンクし、別のフォルダで `-`、続けて `u`、`U`（v0.60.0）→ `-` が `Linked <名前> — u to undo` と言う（v0.67.10。それまではヤンクのトーストが残るだけだった）。`u` はリンクだけを消し、元のファイルと中身はそのまま。`U` でリンクが戻る。Windows では `=`（ハードリンク）とフォルダへの `-` でも — *Yank a file, `-` in another folder, then `u`; then `U` (v0.60.0) → `-` says `Linked <name> — u to undo` (v0.67.10; before, the yank's toast stayed up). `u` removes the link and only the link: the source file and its contents are untouched. `U` makes the link again. On Windows, also with `=` (hardlink) and with a folder (`-` on a directory)*

## 13. シンボリックリンクと `g`+`f` — 9 / 11

Windows ではリンクを作るのが面倒。**ジャンクション**は管理者権限が要らない
（`mklink /J linktest C:\dev`）。**ファイルへのシンボリックリンクは開発者モードか管理者権限**が要る
（設定 > システム > 開発者向け）。

自動テスト済みなので下には出していない: 13.1, 13.2, 13.3, 13.4, 13.5, 13.6, 13.13, 13.15

準備:

```powershell
cd $HOME\Desktop\filer-fixtures
# ハードリンク（13.13, 13.14, 13.16 用）
"x" | Out-File locked.txt
fsutil hardlink create locked-2.txt locked.txt
fsutil hardlink list locked.txt        # 期待値の答え合わせ用
```

- [x] **13.7** ジャンクション（`mklink /J`）で試す → シンボリックリンクと同じ扱い（名前の後ろに `->` が付き、`g`+`f` で追える） — *A junction (`mklink /J`), not just a symlink → Treated the same: `->`, and `g`+`f` follows it*
- [x] **13.8** `y` してから、別のディレクトリで `-` → シンボリックリンクができる。**Windows では開発者モードが要る**（設定 > システム > 開発者向け）。無いと `os error 1314` で失敗し、トーストが対処法を 2 つ示す — *`y`, then `-` in another directory → The symlink appears. **On Windows this needs Developer Mode on** (Settings > System > For developers) — without it, and without running filer elevated, it fails with `os error 1314` and the toast says which two remedies there are. The privilege is the OS's, not the app's: `std` already passes `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE`, which is what makes Developer Mode enough*
- [ ] **13.8a** 開発者モードが無く昇格もしていない状態で、**フォルダ**を `y` し、別のディレクトリで `-`（v0.67.11）→ 同じ拒否のあとに `A junction needs neither: mklink /J "<リンク>" "<フォルダ>"` が続き、パスは両方とも絶対パス。それを `cmd` に貼るとジャンクションができ、`g` `f` でたどれる。ファイルへの `-` ではジャンクションのことは言わない（フォルダ専用なので）。v0.67.19 からは続けて `Make a junction instead?` と聞く（両方のパスを挙げ、ジャンクションは相対にならずネットワークの場所を指せないと言う）。`n` なら何も残らない — *Without Developer Mode and not elevated: `y` on a **folder**, then `-` in another directory (v0.67.11) → The same refusal, and after it `A junction needs neither: mklink /J "<the link>" "<the folder>"`, both paths absolute. Pasting that into `cmd` makes a junction that `g` `f` follows. `-` on a file says nothing of junctions (they are folders only). Since v0.67.19 a question follows: `Make a junction instead?`, naming both paths and saying a junction is not relative and cannot reach a network location; `n` leaves nothing behind*
- [x] **13.8b** 13.8a に続けて `y`（v0.67.19、Q46）→ トースト `Made a junction <名前> — u to undo`。`(Get-Item <リンク>).LinkType` が `Junction` で、`g` `f` でたどれる。`u` はジャンクションだけを消し（フォルダと中身は残る）、`U` でまたジャンクションとして作られる — *As 13.8a, then `y` (v0.67.19, Q46) → Toast `Made a junction <name> — u to undo`; `(Get-Item <link>).LinkType` reads `Junction` and `g` `f` follows it. `u` removes the junction and only the junction (the folder and its files stay); `U` makes it again, still a junction*
- [x] **13.9** `y` してから、**隣の**ディレクトリで `_` → 同じリンクが相対パス（`..\other\file`）で作られる。`g`+`f` で追え、両方のディレクトリを一緒に移動しても壊れない（これが `-` に対する `_` の利点） — *`y`, then `_` in a **sibling** directory → The same link, written relative (`..\other\file`). `g`+`f` follows it, and it survives moving both directories together — which is the point of `_` over `-`*
- [x] **13.10** シンボリックリンクの上で `<Tab>` → **Link** セクションが出る。`Kind` が `Symlink`、`Target` が保存されたパス、`Resolves` が実際の着地点 — *`<Tab>` on a symlink (v0.46.0) → A **Link** section: `Kind` reads `Symlink`, `Target` the stored path, `Resolves` where it lands*
- [x] **13.11** `_` で作ったリンクの上で `<Tab>` → `Kind` が `Symlink (relative)`。`Target` は相対パス、`Resolves` は絶対パスで、**2 行が食い違うことがこの対の要点** — *`<Tab>` on a link made with `_` → `Kind` reads `Symlink (relative)`, and `Target` is the relative path while `Resolves` is absolute — the two rows differ, which is the whole point of the pair*
- [x] **13.12** **壊れた**リンクの上で `<Tab>` → `Resolves` が `no (…)` と OS の理由を出し、セクション自体は表示される — *`<Tab>` on a **broken** link → `Resolves` reads `no (…)` with the OS's reason, and the section still appears*
- [x] **13.14** 同じものを Windows で → `Also at` に別名のパスが並ぶ。`fsutil hardlink list` と突き合わせる（自分自身のパスを除いた同じ集合になる） — *The same, on Windows → `Also at` lists the other path. Check it against `fsutil hardlink list` — the same set, with the file's own path left out*
- [x] **13.16** ハードリンクを作り、別のプログラムに共有なしの書き込みロックを握らせた状態で `<Tab>`（コマンドは上の「準備」）→ `Links` は `2` のまま、`Also at` も出る。ハンドルがアクセス権を一切要求しないので、排他ロックでも数えられる — *Hardlink a file, then have another program hold it open for writing with no sharing, and `<Tab>` it (commands in the preamble above) → `Links` still reads `2` and `Also at` still lists the other name. The handle asks for **no** access rights, so an exclusive write lock does not hide the count*
- [ ] **13.17** ジャンクション（`mklink /J`）で `<Tab>`（v0.59.4）→ `Kind` が `Symlink` ではなく `Junction`。同じフォルダへのシンボリックリンク（`mklink /D`）は今までどおり `Symlink`。一覧の `->` はどちらも変わらない（13.7） — *`<Tab>` on a junction (`mklink /J`) (v0.59.4) → `Kind` reads `Junction`, not `Symlink`. A symlink to the same folder (`mklink /D`) still reads `Symlink`. The list's `->` is unchanged for both (13.7)*

## 14. 親ディレクトリの列を、マウスで — 0 / 6

左端の列（いまいるディレクトリの親）をマウスで操作する節。キーボードでは触らない。

- [ ] **14.1** そこの**ディレクトリ**をクリック → 従来どおり中へ入る — *Click a **directory** there → The list goes into it, as it always has*
- [ ] **14.2** そこの**ファイル**をクリック → そのファイルのある階層まで上がり、**カーソルがそのファイルに乗る**（v0.26.7 までは何も起きなかった） — *Click a **file** there (v0.26.7) → The list goes up to where that file lives, **with the file under the cursor**. Until v0.26.7 nothing happened at all*
- [ ] **14.3** そのあと `<Enter>` → 開く。カーソルが「近く」ではなく本当にそのファイルに乗っている確認 — *Then press `<Enter>` → It opens — the cursor really is on it, not merely near it*
- [ ] **14.4** どちらかをダブルクリック → シングルクリックと同じ。2 つ目の別の意味は無い — *Double-click either → The same as a single click; no second, different meaning*
- [ ] **14.5** いま自分がいるディレクトリの行をクリック → その場に留まり、カーソルが飛び回らない — *Click the row for the directory you are already in → You stay there, and the cursor does not jump about*
- [ ] **14.6** ドライブ直下（親の列が無い場所）で → クリックするものが無く、何もおかしくならない — *At a drive root, where there is no parent column → Nothing to click, and nothing misbehaves*

## 15. ウィンドウの拡大縮小と、取り返したキー — 6 / 9

**v0.32.0 まで `<C-->` はウィンドウ縮小とハードリンク作成を同時にやっていた** —— 1 打で 2 動作。
それを分けたのがこの節。ハードリンク作成は `=` に移った（v0.45.6 まで `<C-S-->` だったが、
**あの和音はどのキーボードでも打てなかった**）。

自動テスト済みなので下には出していない: 15.8

準備:

```powershell
# 15.5 の答え合わせ用。同じドライブの中で試すこと（NTFS のハードリンクはボリュームを跨げない）
fsutil hardlink list <新しくできたパス>
```

- [x] **15.1** 何かヤンクした状態で `<C-->` → **ウィンドウが縮むだけ。**v0.32.0 まではハードリンクも同時に作っていた（1 打で 2 動作） — *`<C-->` with something yanked → **Only** the window shrinks. Until v0.32.0 it also made a hardlink — one press, two actions*
- [x] **15.2** `<C-+>` と `<C-=>` → どちらでも大きくなる。どちらに shift が要るかは配列次第（US は `+` が shift+equals、JIS は `+` が shift+semicolon で `=` が shift+minus）。**両方の綴りが割り当ててあるのでどちらからでも届く**（v0.45.6） — *`<C-+>`, and `<C-=>` → Both make it bigger. Which of the two needs shift depends on the layout — on US `+` is shift+equals, on JIS `+` is shift+semicolon and `=` is shift+minus — and both spellings are bound so either reaches it (v0.45.6)*
- [x] **15.3** `<C-0>` → 100% に戻り、トーストがそう言う — *`<C-0>` → Back to 100%, and a toast says so*
- [ ] **15.4** `<C-->` を押しっぱなし → 20% で止まり、トーストの回数が勘定に合う（100% から 8 段、残りは床で `Scale 20% (minimum) ×N`）。`<C-+>` の押しっぱなしは 500% で止まる — *Hold `<C-->` down → It stops at 20%, and the toast's count adds up: 8 steps down from 100% and the rest at the floor (`Scale 20% (minimum) ×N`). `<C-+>` held stops at 500%*
- [ ] **15.4a** 同じことを、トーストではなく窓を見ながら → 押している間**滑らかに**縮み、段の間にちらつきや空のフレームが出ない — *The same, watching the window rather than the toast → It shrinks **smoothly** while held, with no flicker or blank frames between steps*
- [x] **15.5** 何かヤンクした状態で、**同じドライブ内の**ディレクトリで `=` → ハードリンクができる。**一覧の行には印が出ない**（ハードリンクは同じ実体を指す別のエントリなので、区別する印が無い）。v0.46.0 以降は **`<Tab>` の spot パネルに出る**（`Kind: Hardlink` / `Links: 2`。それが 13.13）。外から確かめるなら `fsutil hardlink list <新しいパス>`。**ドライブを跨ぐと失敗するのが正しい**（NTFS のハードリンクはボリュームを跨げない） — *`=` with something yanked, in a directory **on the same drive** → The hardlink, in its new place. No *row* says so — a hardlink is another entry pointing at the same data, so the listing has no marker for it. Since v0.46.0 the spot panel does: `<Tab>` on it reads `Kind: Hardlink` and `Links: 2`, which is 13.13. Confirm from outside with `fsutil hardlink list <the new path>`, which lists every path sharing the data; or write to one and read the other. Across drives it must fail: NTFS hardlinks cannot leave their volume. Was `<C-S-->` until v0.45.6, a chord no keyboard can produce*
- [ ] **15.6** 画像の上で `<A-i>` / `<A-o>` → **画像の**拡大縮小のまま影響を受けない（`zoom` と `scale` は別のコマンド） — *`<A-i>` / `<A-o>` on an image → Still the **image** zoom, unaffected — `zoom` and `scale` are different commands*
- [x] **15.7** `~` → `scale in` / `scale out` / `scale reset` が他のコマンドと同じように並んでいる — *`~` → `scale in` / `scale out` / `scale reset` are listed, like any other command*
- [x] **15.9** 別のドライブのファイルをヤンクして `=`（`R:` → `C:`）（v0.59.4）→ エラーが Windows の「別のディスク ドライブに移動できません」ではなく `hardlinks can't cross drives (R: → C:). Use p to copy instead` と言う — *`=` with a file yanked from another drive (`R:` → `C:`) (v0.59.4) → The error reads `hardlinks can't cross drives (R: → C:). Use p to copy instead`, not Windows' "cannot move the file to a different disk drive"*

## 16. Word / Excel / PowerPoint — 9 / 12

- [x] **16.1** `.docx` をホバー → 段落ごとの本文が出る。16 進ダンプでもメタデータの一覧でもない — *Hover a `.docx` → Its text, paragraph by paragraph. Not a hex dump, not a metadata card*
- [x] **16.2** 1 文の中で太字と通常が混ざった段落 → **1 行になる**（書式の切れ目ごとに改行されない） — *A paragraph with mixed bold and plain in one sentence → **One line**, not one per run*
- [ ] **16.3** 見出し 1 / 2 のスタイルを使った文書で `<S-Tab>` → 見出しがアウトラインになり、`<Enter>` でその位置へ飛ぶ — *A document with Heading 1/2 styles, then `<S-Tab>` → The headings are the outline, and `<Enter>` on one jumps to it*
- [x] **16.4** `.xlsx` をホバー → 行がタブ区切りのセルとして出て、シートごとに名前が示される — *Hover a `.xlsx` → Rows as tab-separated cells, each sheet announced*
- [x] **16.5** **最初のタブが `sheet1.xml` ではない**ブック → タブがブック上の順番どおりに、本当の名前で出る — *A workbook whose **first tab is not `sheet1.xml`** → The tabs come out in the workbook's order, with their real names*
- [x] **16.6** 日付の入ったシート → `2023-03-15` と出る（`45000` ではない） — *A sheet holding dates → `2023-03-15`, **not** `45000`*
- [x] **16.7** 日付**と時刻**の入ったシート → 日付の後ろに時刻が続く — *A sheet holding a date **and** a time → The time follows the date*
- [x] **16.8** スライドが 10 枚以上ある `.pptx` をホバー → 順番どおり（9 の次が 10。1 の次ではない） — *Hover a `.pptx` with ten or more slides → In order — slide 10 after slide 9, not after slide 1*
- [x] **16.9** 3 種類のどれかに日本語 → 正しく出る。`&amp;` `&lt;` は `&` `<` に戻っている — *Japanese text in any of the three → Correct, and `&amp;` `&lt;` come through as `&` `<`*
- [ ] **16.10** 古い `.doc` を `.docx` に改名してホバー → Office XML ではないと述べるカードが出て、考えられる原因も示す — *Rename an old `.doc` to `.docx` and hover it → A card saying it is not an Office XML file, naming the likely cause*
- [x] **16.11** 非常に大きなブック → 5000 行で打ち切り、省略したと述べる。固まらない — *A very large workbook → Stops at 5000 lines and says it is truncated; it does not hang*
- [ ] **16.12** その中で `/` と `n` → 検索が効く（ただのテキストプレビューなので） — *`/` and `n` inside one → Search works, because it is an ordinary text preview*

## 17. 自前のプレビューア — 18 / 18

- [x] **17.1** 複数ページの PDF をホバー → 1 ページ目と、その下に `page 1` — *Hover a multi-page PDF → Page one, with `page 1` under it*
- [x] **17.2** `<A-j>` → 2 ページ目。下に `page 2` — *`<A-j>` → Page two. `page 2` under it*
- [x] **17.3** `<A-k>` → 1 ページ目に戻る — *`<A-k>` → Back to page one*
- [x] **17.4** 1 ページ目でもう一度 `<A-k>` → そのまま。0 ページや負にはならない — *`<A-k>` again, on page one → Stays. It does not go to page zero or below*
- [x] **17.5** `<A-j>` を最終ページより先まで押し続ける → **最終ページが画面に残ったまま**、`No more: …` とコマンド自身の文言が出る（v0.30.1 まではページがエラーに置き換わっていた） — *Hold `<A-j>` past the last page (v0.30.1) → **The last page stays on screen**, and a line says `No more: …` with the command's own words. Until v0.30.1 the page was replaced by the error*
- [x] **17.5a** その直後に `<A-k>` → 最終ページの 1 つ前へ戻る（行き過ぎた先からではない） — *`<A-k>` straight after that → Back a page from the last one, not from somewhere past it*
- [x] **17.5b** **数秒の短い動画**で `<A-j>` を数回 → 同じく、描けた最後のフレームで止まる。`step = 10` なのですぐ末尾を越えるため、ここが一番効く — *A **short** video — a few seconds — and `<A-j>` a few times → Same: it stops at the last frame it could draw. This is where it bites, since `step = 10` runs off the end almost at once*
- [x] **17.5c** 動画の説明表示 → `50s` と出る（`s 50` ではない） — *The caption on a video (v0.30.1) → `50s`, not `s 50`*
- [x] **17.6** ページを送りながら画面を見る → **コンソールの窓が一瞬も出ない。**1 回の押下につき 1 回だけ実行される — *Watch the screen while paging → **No console window flashes.** It runs once per press*
- [x] **17.7** 5 ページまで送り、別のファイルへ行って戻る → 1 ページ目に戻っている（ページはファイルごとのもの） — *Page to 5, move to another file, come back → Back at page one: the page belongs to the file*
- [x] **17.8** 一度見たページへ戻る → 即座に出る（ページ単位でキャッシュされている） — *Page back to one you have already seen → Instant — it is cached per page*
- [x] **17.9** 動画をホバー → **フレームが出る。**v0.30.2 まで Windows では一度も出なかった（`{out}.png` が `"…page".png` と引用され、`cmd` がファイル名に引用符ごと渡していた） — *Hover a video (v0.30.2) → **A frame appears.** Until v0.30.2 none ever did on Windows: `{out}.png` was quoted as `"…page".png`, which `cmd` hands to ffmpeg with the quotes in the filename*
- [x] **17.9a** 空白を含むパスで同じこと → やはり描ける。引用が拡張子まで含めて単語全体を包むため — *The same on a path with a space → Still draws — the quoting wraps the whole word, suffix included*
- [x] **17.10** その上で `<A-j>` → `step` に従って 10 秒先へ — *`<A-j>` on it → Ten seconds in, by `step`*
- [x] **17.11** 名前に**空白**を含む PDF と、日本語フォルダの中の PDF → どちらも描ける。引用はルール側ではなく filer 側がやっている — *A PDF with a **space** in its name, and one in a Japanese folder → Both draw. The quoting is filer's, not the rule's*
- [x] **17.12** `pdftoppm` の名前を変えてから PDF をホバー → ツール名を挙げたエラーが出る（固まらない） — *Rename `pdftoppm` away, then hover a PDF → An error naming the tool, not a hang*
- [x] **17.13** `[[preview]]` のルールを消して `<C-F5>`、PDF をホバー → シェルのサムネイルに戻る（以前のまま） — *Remove the `[[preview]]` rules, `<C-F5>`, hover a PDF → Back to the shell thumbnail, unchanged*
- [x] **17.14** ルールを入れた状態で `filer env` → Tools の下に `pdftoppm` と `ffmpeg` が並び、横に `preview *.pdf` が出る — *`filer env` with the rules in place → `pdftoppm` and `ffmpeg` listed under Tools, with `preview *.pdf` beside them*

## 18. クイックルックと、ミニマップの隣、その他のペイン — 0 / 5

自動テスト済みなので下には出していない: 18.1, 18.2, 18.3, 18.4, 18.8, 18.9, 18.10

- [ ] **18.5** `<C-w>` → 2 分割になり、キーのあるほうが枠で囲まれ、もう一方のカーソルは淡くなる — *`<C-w>` → The view splits into two panes; the one with the keys is framed, the other's cursor is dimmed*
- [ ] **18.6** ファイルを選んで `<A-c>` → もう一方のペインへコピーされる — *Select files, `<A-c>` → Copied into the other pane*
- [ ] **18.7** ファイルをもう一方のペインへドラッグ → 対象が枠で示され、ポインタの横に「copy」と出る（`Shift` で「move」）。**離す前に**出ること — *Drag files onto the other pane → A frame marks the target, and a label by the pointer says "copy" — `Shift` makes it "move" — **before** you let go*
- [ ] **18.11** `B` で保存しておいた場所へ、`'` に続けて文字を押す → そこへ飛ぶ。**`b` はブックマークの*管理*の前置キー**（`bb` 一覧、`bs` 保存、`bd` 削除）なので、`b` + 文字では何にも届かない — *`'` then a letter, having saved one with `B` → Jumps there. **`b` is the prefix bookmark *management* hangs off** (`bb` lists, `bs` saves, `bd` deletes), so `b` and a letter reaches nothing*
- [ ] **18.12** `z` → ジャンプ一覧が出る。ブックマークが先、そのあと最近のディレクトリが「2h ago」付きで並ぶ — *`z` → The jump list: bookmarks first, then recent directories with "2h ago" beside them*

## 19. ホイール、ペインごとの挙動 — 0 / 7

- [ ] **19.1** 長いテキストの**プレビュー**の上でホイール → 強く回さなくても 1 ノッチずつスクロールする（v0.26.5 の修正） — *Wheel over the **preview** of a long text file → It scrolls, one notch at a time, without spinning hard. This is the v0.26.5 fix*
- [ ] **19.2** プレビューの上でできる限りゆっくり回す → それでも動く。端数も捨てずに積算される — *Turn the wheel as slowly as you can over the preview → It still moves. Every fraction counts; nothing is discarded*
- [ ] **19.3** **ファイル一覧**の上でホイール → 同じ。分割しているときは、それぞれのペインの上で — *Wheel over the **file list** → The same, and with the split open, over each pane in turn*
- [ ] **19.4** **ターミナル**ペインの上でホイール → 同じく正しい（v0.20.4 で先に直してあり、いまは同じコードを共有している） — *Wheel over the **terminal** pane → Still right — fixed earlier, in v0.20.4, and now sharing the same code*
- [ ] **19.5** 一方に回してすぐ逆に回す → 即座に反転する。取り残された端数による空走りが無い — *Turn one way then straight back → It reverses at once, with no dead travel from a stranded remainder*
- [ ] **19.6** 画像の上で `Ctrl`+ホイール → 拡大縮小し、同じ操作でペインが**スクロールしない** — *`Ctrl` and the wheel over an image → Zooms, and does **not** scroll the pane with the same turn*
- [ ] **19.7** 回している途中でポインタを別のペインへ移す → どちらも飛ばない。端数はペインごとに別々に持っている — *Move the pointer between panes mid-turn → Neither jumps: each keeps its own remainder*

## 20. 設定とテーマ — 7 / 7

- [x] **20.1** filer を開いたまま `theme.toml` を編集し（`[mgr] cwd` を目立つ色に）、`<C-F5>` → 再起動せずに色が変わる — *With filer open, edit `theme.toml` (change `[mgr] cwd` to something loud) and press `<C-F5>` → The color changes without restarting*
- [x] **20.2** `filer.toml` の `[ui] font_size` を変えて `<C-F5>` → 文字の大きさが変わる — *Change `[ui] font_size` in `filer.toml`, `<C-F5>` → The text resizes*
- [x] **20.3** `keymap.toml` に割り当てを足して `<C-F5>` → 新しいキーが効き、`<F1>` の一覧にも出る — *Add a `keymap.toml` binding, `<C-F5>` → The new key works, and `<F1>` lists it*
- [x] **20.4** `,s` で並べ替えてから `<C-F5>` → **並び順は自分で設定したまま残る。**再読み込みが手で変えたものを元に戻さないこと — *Sort with `,s`, then `<C-F5>` → The sort **stays** as you set it — a reload does not undo what you changed by hand*
- [x] **20.5** `filer.toml` に構文エラーを入れて `<C-F5>` → 問題を挙げたエラーのトーストが出て、**以前の設定がそのまま効き続ける**。**壊れたファイル自身が決めていた値も含めて**（v0.67.18、Q47。それまでは、その `[ui]` が既定値に戻っていた）。トーストの末尾は `(the last settings read from it stay in force until it parses again)` — *Put a syntax error in `filer.toml`, `<C-F5>` → An error toast naming the problem; the old config stays in force -- **including what the broken file itself set** (v0.67.18, Q47: before, its `[ui]` fell back to the defaults), and the toast ends `(the last settings read from it stay in force until it parses again)`*
- [x] **20.6** `[ui] minimap = false` にして `<C-F5>` → ミニマップが出なくなる — *`[ui] minimap = false`, `<C-F5>` → No minimap*
- [x] **20.7** `keymap.toml` に `[[mgr.prepend_keymap]]` で `on = "<F8>"`、`run = 'cd C:\Windows\System32'`（コマンドの中に引用符なし）を書く（v0.59.0）→ `<F8>` で `C:\Windows\System32` に着く。v0.59.0 までは `\` が落ち、エラーが `C:WindowsSystem32` を挙げていた — *In `keymap.toml`, `[[mgr.prepend_keymap]]` `on = "<F8>"`, `run = 'cd C:\Windows\System32'` -- no quotes inside the command (v0.59.0) → `<F8>` lands in `C:\Windows\System32`. Until v0.59.0 the backslashes were dropped and the error named `C:WindowsSystem32`*

## 21. 書庫（zip / tar / 7z） — 11 / 11

自動テスト済みなので下には出していない: 21.1, 21.4, 21.6, 21.12

- [x] **21.2** `sample.zip` で `e` → 隣に `sample` フォルダとして展開され、タスクパネルに進捗が出る — *`e` on it → Unpacked into a `sample` folder beside it; progress in the task panel*
- [x] **21.3** もう一度 `e` → 2 つ目は別の名前になる。1 つ目は上書きされない — *`e` again → The second one gets a different name; the first is not overwritten*
- [x] **21.5** `E` で名前を `.tar.gz` で終わるように変える → zip ではなく gzip 圧縮した tar になる — *`E` and change the name to end in `.tar.gz` → A gzipped tar, not a zip*
- [x] **21.7** `E` で名前を **`.7z`** で終わるように変える → 本物の 7z ができる（v0.27.0 まで読み取り専用として断られていた） — *`E` and change the name to end in **`.7z`** (v0.27.0) → A real 7z. Until v0.27.0 this was refused as read-only*
- [x] **21.8** その `.7z` で `e` → 展開でき、中身が入れたものと一致する — *`e` on that `.7z` → It unpacks, and the files match what went in*
- [x] **21.9** 同じ `.7z` を 7-Zip や エクスプローラーで開く → そちらでも開ける。他所へ持って行けることが、この形式を入れた理由 — *Open the same `.7z` in 7-Zip or Explorer → It opens there too — the point of the format is that it travels*
- [x] **21.10** サブフォルダを含むフォルダを `.7z` に固めて、タスクパネルを見る → 件数は**ファイル**の数（フォルダではない）で、途中で止まらず総数まで届く — *Pack a folder holding subfolders as `.7z`, watch the task panel → The count is of **files**, not folders, and it reaches the total rather than stopping short*
- [x] **21.11** 同じ入力の `.7z` と `.zip` を比べる → 7z のほうが小さい。それが持っている理由 — *Compare the `.7z` and the `.zip` of the same input → The 7z is smaller; that is the reason to have it*
- [x] **21.13** `to-pack\` を `.zip` に圧縮し、`7z l` で見る（v0.57.2）→ どのエントリも元のファイルの日時（偶数秒まで）を持つ。`1980-01-01 00:00:00` ではない — *Pack `to-pack\` as `.zip`, then `7z l` the archive (v0.57.2) → Every entry carries its file's own date and time (to the even second), not `1980-01-01 00:00:00`*
- [x] **21.14** `to-pack\` のファイルを古い日時にし（`(Get-Item f).LastWriteTime = "2021-06-15 12:34:56"`）、`E` で `.zip`・`.tar.gz`・`.7z` に固めて、それぞれ `e` で展開する（v0.65.7）→ 展開したどのファイルも元の `LastWriteTime` に戻っている（zip は偶数秒まで）。展開した時刻ではない。v0.65.7 より前は、どれも `e` の時刻になっていた（#156） — *Give `to-pack\` files with old dates (`(Get-Item f).LastWriteTime = "2021-06-15 12:34:56"`), pack it with `E` as `.zip`, `.tar.gz` and `.7z`, then `e` each one (v0.65.7) → Every unpacked file has its original `LastWriteTime` back (a zip to the even second), not the moment it was unpacked. Before v0.65.7 every one read the time of the `e` (#156)*
- [x] **21.15** `to-pack\` だけを選んで `E`（書庫の一番上がフォルダ 1 つ）、`to-pack.zip` で `e`。続けて一番上がファイルの `sample.zip` でも同じ（v0.66.0）→ `to-pack_1\` の中に直接ファイルが入る。`to-pack_1\to-pack\` にはならない（Q43）。`sample.zip` は今までどおり自分の `sample\`（か `sample_1\`）フォルダに展開される — *`E` on `to-pack\` alone (the archive's top level is one folder), then `e` on `to-pack.zip`; then the same with `sample.zip`, whose top level is loose files (v0.66.0) → `to-pack_1\` holds the files directly -- no `to-pack_1\to-pack\` (Q43). `sample.zip` still unpacks into its own `sample\` (or `sample_1\`) folder*

## 22. エディタを行番号付きで開く（エディタのインストールが要る） — 2 / 6

- [ ] **22.1** 秀丸エディタ → アウトラインの項目の行で開く — *秀丸エディタ → Opens at the outline entry's line*
- [ ] **22.2** サクラエディタ → 同じ — *サクラエディタ → Same*
- [ ] **22.3** EmEditor → 同じ — *EmEditor → Same*
- [ ] **22.4** Notepad++ → 同じ — *Notepad++ → Same*
- [x] **22.5** メモ帳 → 開くが先頭から。行を指定する引数が無いので、**これが正しい** — *メモ帳 → Opens, at the top — it has no line argument, and that is correct*
- [x] **22.6** VS Code / nvim（入っていれば）→ その行で開く — *VS Code / nvim, if you have them → At the line*

## 23. ネットワークパス（共有が要る） — 1 / 5

自動テスト済みなので下には出していない: 23.6

- [ ] **23.1** `g<Space>` から `\\server\share` と入力 → 開く — *`g<Space>`, type `\\server\share` → It opens*
- [ ] **23.2** そこへファイルをコピー、そこからコピー → どちらも動き、進捗が出る — *Copy a file to and from it → Works, with progress*
- [ ] **23.3** 一覧の取得中にネットワークを抜く、または応答しないホストを指定する → **ウィンドウが固まらない。**エラーのトーストが出て、タブは元の場所に戻る — *Unplug the network mid-listing, or point at a dead host → **The window keeps responding.** An error toast, and the tab goes back where it was*
- [ ] **23.4** 共有上のパスを Tab 補完する → プロンプトが固まらず、待っている間 `…` が出る — *Tab-complete a path on the share → The prompt stays responsive; a `…` shows while it waits*
- [x] **23.5** `g<Space>` で、途中のフォルダ名に `|` を含むパス（`C:\Temp\a|b\c\d`）を打って `<Enter>`（v0.57.3）→ エラーのトーストは **1 つ**で、パス全体を挙げる。v0.57.3 までは親の列がそれぞれ自分のトーストを足し、断片（`b: …`、`c: …`）だけを挙げていた — *`g<Space>`, type a path with a `\|` in a folder name partway down (`C:\Temp\a\|b\c\d`), `<Enter>` (v0.57.3) → **One** error toast, naming a whole path. Until v0.57.3 the parent columns each added their own, naming only a fragment (`b: …`, `c: …`)*

## 24. 扱いにくい名前 — 3 / 3

自動テスト済みなので下には出していない: 24.1, 24.2, 24.3

- [x] **24.4** 引用符を含む名前をコピーして、`<A-t>` でターミナルへ → シェルが 1 語として受け取る形に引用される — *Copy the name with a quote in it, `<A-t>` into the terminal → Quoted so the shell sees one word*
- [x] **24.5** 日本語名のファイルで `d` してから `u` → 同じ名前で戻る — *`d` then `u` on the CJK-named file → Comes back under the same name*
- [x] **24.6** 新しいフォルダで `scripts\make-fixtures.ps1` を走らせる（v0.59.1）→ 警告は出ない。ただし普通の（大文字小文字を区別しない）NTFS のフォルダでは `awkward names: 5 entries on disk, expected 6` が出て、`fsutil file setCaseSensitiveInfo` を挙げる（24.3 がそこで押せない理由） — *Run `scripts\make-fixtures.ps1` in a fresh folder (v0.59.1) → No warning, except on an ordinary (case-insensitive) NTFS folder: `awkward names: 5 entries on disk, expected 6`, naming `fsutil file setCaseSensitiveInfo` -- the reason 24.3 cannot be pressed there*

## 25. `filer env` — 29 / 31

- [x] **25.1** PowerShell から `filer env` → 5 つの節（Filer、Config、Last run、Tools、Variables）が表示される。release ビルドは GUI バイナリなので、`--version` と同じ `CONOUT$` の経路を通る —— **実際に文字が出ること** — *`filer env` from PowerShell → The five sections print (Filer, Config, Last run, Tools, Variables). A release build is a GUI binary, so this is the same `CONOUT$` path `--version` uses — **text actually appears***
- [x] **25.2** Config の節 → 両方のディレクトリが出て、それぞれ中身か `nothing here` を述べ、`not here:` に残りが並ぶ — *The Config section → Both directories, each saying what is in it or `nothing here`, and `not here:` listing the rest*
- [x] **25.3** `keymap.toml` にわざと打ち間違いを入れる → `Warnings` の下に警告が出て、複数行がそのキーの下に字下げされる — *With a deliberate typo in `keymap.toml` → The warning appears under `Warnings`, its several lines indented under the one key*
- [x] **25.4** Tools の節 → `git` とその版、ペインが起動するシェル、`[[preview]]` の規則とオープナーが名指すプログラムが、それぞれパス（無ければ `not found`）と用途付きで並ぶ。filer が走らせないもの（`pdftoppm`、`ffmpeg`）は出ない — *The Tools section → `git` with its version, the shell the terminal pane starts, and every program a `[[preview]]` rule or an opener names -- each with its path (or `not found`) and what it is for. Nothing filer does not run (no `pdftoppm`, `ffmpeg`)*
- [x] **25.4a** `[term] shell = "pwsh"` を設定した状態で → シェルとして `pwsh` が並ぶ。設定していなければ `powershell`。**実際に起動するほう**であって推測ではない — *With `[term] shell = "pwsh"` set (v0.29.1) → `pwsh` is the shell listed. Without it, `powershell` — the one that will actually launch, not a guess*
- [x] **25.4b** オープナーを設定した状態で → 指定された各プログラムが、属するオープナーの種類とともに並ぶ（見つかったかどうかも） — *With openers configured → Each named program is listed with the opener kind it belongs to, found or not*
- [x] **25.4c** **引用符付きのフルパス**を指定したオープナー（秀丸、サクラ）→ 最初の空白までではなく、パス全体が解決される — *An opener naming a **quoted full path** (秀丸, サクラ) → The whole path is resolved, not just up to the first space*
- [x] **25.4d** `filer env` の実行中に画面を見る → **エディタやビューアが 1 つも起動しない。**`PATH` を調べるだけで、実行はしない — *Watch the screen while `filer env` runs → **No editor or viewer opens.** The programs are looked up on `PATH`, never executed*
- [x] **25.5** ARM 版 Windows で x64 ビルドを使う → `OS arch` と `Process arch` が**食い違う。**その食い違いを見せることが、両方を出している理由 — *On Windows on ARM with the x64 build → `OS arch` and `Process arch` **disagree** — that disagreement is the whole reason both are printed*
- [x] **25.6** `filer --help` → COMMANDS の下に `env` が並ぶ — *`filer --help` → `env` is listed under COMMANDS*
- [x] **25.7** `filer.exe` をダブルクリック（コンソール無し）→ 以前のまま。ウィンドウが開き、どこにも何も出力されない — *Double-click `filer.exe` (no console) → Unchanged: the window opens, nothing is printed anywhere*
- [x] **25.8** filer を一度開いて終了し、そのあと `filer env` → **Last run** の節が出る。アダプタとそのバックエンド・デバイス種別、読み込まれた全フォントファイル — *Open filer once, quit, then `filer env` (v0.29.0) → A **Last run** section: the adapter with its backend and device type, and every font file that was loaded*
- [x] **25.9** 新しい機械で、filer を一度も開かずに `filer env` → `not recorded — filer has not opened a window on this machine yet` と出る（空の節ではない） — *On a fresh machine, `filer env` **before** ever opening filer → `not recorded — filer has not opened a window on this machine yet`, not an empty section*
- [x] **25.10** `filer.toml` で別のフォントを指定して `<C-F5>`、もう一度 `filer env` → 新しいファイルが並ぶ。再読み込みが記録を更新している — *Name a different font in `filer.toml`, `<C-F5>`, then `filer env` again → The new file is listed; the reload updates the record*
- [x] **25.8a** filer を開いて終了し、`filer env` の **Window** の行を画面と突き合わせる（v0.47.33）→ ピクセルの数が目の前の窓と一致し、`pt × 倍率` がその数になる。**DPI の議論に決着を付けるのはこの行。**外から `GetClientRect` で測った値も `PrintWindow` のキャプチャも、**測った側のプロセスの DPI 認識に依存する**ので、窓と食い違ったまま両方もっともらしく見えることがある — *Open filer, quit, `filer env`, and check the **Window** row against the screen (v0.47.33) → The pixels are the window you can see, and `pt x scale` multiplies out to them. **This is the row that settles a DPI argument** — what a script measures with `GetClientRect`, or a `PrintWindow` capture, depends on the DPI awareness of whatever did the measuring, and can disagree with the window while looking right*
- [x] **25.8b** 150% 表示の画面で filer を開いて終了し、`filer env` → Window の行が例えば `2040 x 1290 px (1360 x 860 pt @ 1.5)` と出る。ピクセルがポイントの 1.5 倍で、**窓の右端も下端も何も切れていない** — *On a display at 150%, open filer, quit, then `filer env` → The Window row reads e.g. `2040 x 1290 px (1360 x 860 pt @ 1.5)` — the pixels are half again the points, and **nothing is cut off the right or bottom edge of the window***
- [ ] **25.11** ボールド体がどこにも無い状態で → `none found; bold is faked by overstriking` と出る。ボールドの一覧を通常のものと分けてあるのは、このため — *With no bold face anywhere → `none found; bold is faked by overstriking` — the bold list is separate from the regular one on purpose*
- [x] **25.12** `start` で始まるオープナー（既定のアプリで開くもの）→ **`built into cmd`** と出る（`not found` ではない）。`cmd` 自身のコマンドで `PATH` 上のファイルではないため、他の行と同じ探索では見つけられない（v0.33.12） — *An opener starting with `start` (the default-app one) → **`built into cmd`**, not `not found`. It is one of `cmd`'s own commands and is never a file on the `PATH`, so the lookup every other row uses cannot see it (v0.33.12)*
- [x] **25.13** そのオープナーを使うルールのファイルで `<Enter>` → 実際に開く。表示と挙動が一致していること — *`<Enter>` on a file whose rule uses that opener → It really does open — the row and the behaviour agree*
- [x] **25.14** 本当に入っていないプログラムを指定したオープナー → やはり **`not found`**。例外扱いはシェル自身の名前だけ — *An opener naming a program that genuinely is not installed → Still **`not found`**. The exemption is for the shell's own names only*
- [x] **25.15** `yazi.toml` を壊して Warnings の行を読む → パスが **`…\filer\yazi.toml`** と、すべて円記号で書かれている。以前は `…\filer/yazi.toml` と出ていた —— **編集すべきファイルを指し示すのが仕事のメッセージなのに、そこが壊れていた**（v0.33.12） — *Break `yazi.toml` and read the Warnings row → The path is written **`…\filer\yazi.toml`**, all backslashes. It used to come out `…\filer/yazi.toml`, in the one message whose job is to name the file to edit (v0.33.12)*
- [x] **25.16** ファイルのあるフォルダで `filer <フォルダ> --keys "<Tab>C"`、続けて `Get-Clipboard`（v0.54.0）→ 窓が開き、spot が勝手に先頭の行で開き、クリップボードにパネル全体が `ラベル<TAB>値` の行で入っている。`Name` と `Path` がその先頭の行を指している — *`filer <a folder with files> --keys "<Tab>C"`, then `Get-Clipboard` (v0.54.0) → The window opens, spot opens on the first row by itself, and the clipboard holds the whole panel as `Label<TAB>value` lines — `Name` and `Path` naming that first row*
- [x] **25.17** `filer --keys "<Tab"` と `filer --keys "<Bogus>"` → **窓は開かない。**問題を言う 1 行（`has no closing >` / `is not a key`）と、終了コード 2 — *`filer --keys "<Tab"` and `filer --keys "<Bogus>"` → **No window**: one line naming the problem (`has no closing >` / `is not a key`), exit code 2*
- [x] **25.18** release ビルドで `filer env \| Out-File out.txt` と `Get-Content out.txt`、それと `cmd /c "filer env > out2.txt"`（v0.54.4）→ レポート全体が**両方のファイルに入り**、画面には何も出ない。v0.54.4 より前はどちらも空だった。（PowerShell 自身の `filer env > out.txt` は今も空になる。PowerShell が窓のアプリの出力をファイルにつながないため。README にそう書いた） — *Release build: `filer env \| Out-File out.txt`, then `Get-Content out.txt`; and `cmd /c "filer env > out2.txt"` (v0.54.4) → The whole report is **in both files**, and nothing is printed on screen. Before v0.54.4 both were empty. (PowerShell's own `filer env > out.txt` still gives an empty file: PowerShell does not connect a windowed program's output to a file. README says so)*
- [ ] **25.19** `filer env \| Select-String arch` → **arch の 2 行だけ**が出る（Windows。それ以外は `Process arch` の 1 行）（レポート全体ではない）。`$v = & filer env; $v.Count` はレポートの行数で、0 ではない — *`filer env \| Select-String arch` → **Only the two arch lines** (Windows; elsewhere there is one, `Process arch`), not the whole report. `$v = & filer env; $v.Count` is the report's line count, not 0*
- [x] **25.20** 何もリダイレクトせずに `filer env` と `filer --version` → 25.1 のとおり、今までどおり画面に出る。コンソールの経路は変わっていない — *`filer env` with nothing redirected, and `filer --version` → Still printed on screen, as 25.1 has it — the console path is unchanged*
- [x] **25.21** `filer env`（v0.58.1）→ 答えた `.exe` のフルパスを `Executable` の行に出す。ARM64 の機械では、**x64** ビルドの `Process arch` が `x86_64 (emulated on aarch64)`、ARM64 ビルドは `aarch64` だけ — *`filer env` (v0.58.1) → An `Executable` row with the full path of the `.exe` that answered. On the ARM64 machine, the **x64** build's `Process arch` reads `x86_64 (emulated on aarch64)`; the ARM64 build's reads `aarch64` alone*
- [x] **25.22** `FILER_PTY_LOG` を設定して `filer --keys "<C-t><Wait:2000>echo<Space>hi<Enter><Wait:1000><C-S-Enter>"`（v0.59.0）→ `echo` が届く前にシェルのプロンプトが出ている（ログの `out` の行が `in key` の行より前）、`hi` が表示され、1 秒後にペインが窓を取る。`filer --keys "<Wait:1.5s>"` はコマンド行で断られ、`<Wait:500>` と書き方を示す — *`filer --keys "<C-t><Wait:2000>echo<Space>hi<Enter><Wait:1000><C-S-Enter>"` with `FILER_PTY_LOG` set (v0.59.0) → The shell's prompt is up before `echo` arrives (the log's `out` lines show it ahead of the `in key` lines), `hi` is printed, and the pane takes the window a second later. `filer --keys "<Wait:1.5s>"` is refused on the command line, naming `<Wait:500>`*
- [x] **25.23** ペインを開いて `<C-S-Enter>`、filer を閉じてから `filer env`（v0.59.4）→ `Last run` の下に `Terminal pane` の行があり、最後の大きさを `N x M (lines x columns)` で出す。ペインを一度も開かなかった run の後は `not opened in that run` — *Open the pane, `<C-S-Enter>`, close filer, then `filer env` (v0.59.4) → A `Terminal pane` row under `Last run` gives the grid as `N x M (lines x columns)`, the size it last had. After a run that never opened the pane: `not opened in that run`*
- [x] **25.24** どこかのフォルダのシェルから `filer .`、次に `filer ..`、次に引用符なしで `filer two words`（v0.59.5）→ `.` はそのフォルダを開き、タイトルが**絶対**パスで親の列があり、`h` で上がれる。`..` は 1 つ上を開く。引用符なしの 2 語は窓を開く前に `filer: more than one path: "two" and "words" (a path with a space in it needs quotes)` と断られる — *From a shell in some folder, `filer .`, then `filer ..`, then `filer two words` unquoted (v0.59.5) → `.` opens that folder with its **absolute** path in the title and a parent column, and `h` goes up; `..` opens the one above. The unquoted pair is refused before any window: `filer: more than one path: "two" and "words" (a path with a space in it needs quotes)`*
- [x] **25.25** オープナーでファイルを開き（`<Enter>` か `<S-Enter>`）、`;` のシェルを 1 つ走らせ、filer を閉じてから `filer env`（v0.59.9）→ `Last run` の下の `Launched` の行に、filer が組み立てたとおりのコマンド行が 2 つ、新しいものが後で、最大 5 つ並ぶ。何も起動しなかった run の後は `nothing in that run` — *Open a file with an opener (`<Enter>` or `<S-Enter>`), run one `;` shell command, close filer, then `filer env` (v0.59.9) → A `Launched` row under `Last run` lists both command lines exactly as filer built them, newest last, at most five. After a run that launched nothing: `nothing in that run`*

## 26. アプリの中からのバグ報告 — 8 / 11

- [x] **26.1** `<F12>` → 既定のブラウザで GitHub の新規 issue フォームが開き、トーストがそう言う — *`<F12>` → The default browser opens GitHub's new-issue form, and a toast says so*
- [x] **26.2** フォームを見る → **Version** と **OS とアーキテクチャ** が既に埋まっている。残りは空 — *Look at the form → **Version** and **OS とアーキテクチャ** are already filled in; the rest is empty*
- [x] **26.3** 埋まっている版を、ターミナルの `filer --version` と比べる → アーキテクチャも含めて同じ文字列 — *Compare the filled version against `filer --version` in a terminal → The same string, architecture included*
- [x] **26.4** 埋まっている OS の行を `winver` と比べる → エディション・機能更新・ビルドがすべて一致し、UBR も入っている（`Windows 11 Pro 25H2 (build 26200.9457)`） — *Compare the filled OS line against `winver` → Edition, feature update and build all match, UBR included (`Windows 11 Pro 25H2 (build 26200.9457)`)*
- [x] **26.4b** フォーム自身が載せている PowerShell の断片と比べる → 同じ事実。上から貼り直す価値のあるものが残っていない — *Compare it against the form's own PowerShell snippet → The same facts. Nothing left worth pasting over the top*
- [x] **26.5** ARM64 機で **ARM64** ビルドを使う → OS arch と Process arch がどちらも `aarch64` — *On the ARM64 machine, with the **ARM64** build → OS arch and Process arch both read `aarch64`*
- [x] **26.6** ARM64 機で **x64** ビルドを使う（エミュレーション）→ OS arch は `aarch64`、Process arch は `x86_64` —— **食い違うことが、まさに報告したい事実** — *On the ARM64 machine, with the **x64** build (under emulation) → OS arch `aarch64`, Process arch `x86_64` — **the two disagree, and that is the finding***
- [ ] **26.7** 報告を投稿する → 投稿でき、あらかじめ埋まっていた欄も残る — *Submit the report → It posts, and the pre-filled fields survive*
- [ ] **26.8** 既定のブラウザが未設定（または関連付けが壊れている）状態で `<F12>` → 失敗を述べるエラーのトースト。**ウィンドウは動き続ける** — *`<F12>` with no browser set as default (or a broken association) → An error toast naming the failure. **The window keeps working***
- [x] **26.9** ターミナルペインの中から `<F12>`（先に `<C-t>`）→ 何も起きない。`[term]` がシェルに渡しているためで、これが正しい — *`<F12>` from the terminal pane (`<C-t>` first) → Nothing: `[term]` passes it to the shell, which is correct*
- [ ] **26.10** 26.8 と同じく関連付けを壊した状態で `<F12>`（v0.52.0）→ エラーのトーストが、報告のリンクが**クリップボードにある**とも言う。ブラウザに貼ると同じ、埋まった状態のフォームが開く。ブラウザが開けたときはクリップボードに触らない — *`<F12>` with the browser association broken, as in 26.8 (v0.52.0) → The error toast also says the report's link is **on the clipboard**; pasting it into a browser opens the same pre-filled form. When the browser *does* open, the clipboard is left alone*

## 27. 届かなかったプレビュー — 全 5 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 28. 外から加えられた変更 — 7 / 8

- [x] **28.1** カーソルを**最終行**に置いて、そのファイルをエクスプローラーから削除 → 行が消え、カーソルは新しい最終行に乗る。**落ちない** — *Put the cursor on the **last** row, delete that file from Explorer → The row goes, the cursor lands on the new last row, **no crash***
- [x] **28.2** カーソルを最終行に置いて、末尾の複数ファイルを一度に削除 → 同じ — *Cursor on the last row; delete several files at the end at once → Same*
- [x] **28.3** フォルダ内の全ファイルを外から削除 → 空の一覧になり、操作も効く — *Delete every file in the folder from outside → An empty listing, still responsive*
- [x] **28.4** **絞り込んだ**一覧（`f`）の最終行にカーソルを置き、そのファイルを削除 → 同じ。絞り込みも保たれる — *Cursor on the last row of a **filtered** listing (`f`), delete the file it is on → Same, and the filter still holds*
- [x] **28.5** **もう一方のペイン**（`<C-w>`）と、ディレクトリの**プレビュー**で同じこと → どちらも落ちない — *Same in the **other pane** (`<C-w>`) and in the **preview** of a directory → Neither crashes*
- [x] **28.6** カーソルを最終行に置いて、そのファイルを `d` で削除 → 同じ。これが Issue #5 の報告内容 — *Cursor on the last row, delete that file with `d` → Same — this is what Issue #5 reported*
- [x] **28.7** カーソルが乗っているファイルを外から改名 → カーソルが名前に追従するか、その場に留まる。落ちない — *Rename a file from outside while the cursor is on it → The cursor follows the name or stays put; no crash*
- [ ] **28.8** 窓に触らず（キーもマウスも使わず）、一覧にあるファイルをエクスプローラーから削除する（v0.57.2）→ キーを押さなくても 0.5 秒以内に行が消える。v0.57.2 までは次にキーを押すまで残っていた（#108） — *Leave the window alone -- no key, no mouse -- and delete a listed file from Explorer (v0.57.2) → The row goes within half a second, without a key being pressed. Until v0.57.2 the list kept it until the next key (#108)*

## 29. ターミナルのカレントディレクトリを持ち帰る — 8 / 8

自動テスト済みなので下には出していない: 29.4

- [x] **29.1** `$PROFILE` にフックが**無い**状態でターミナルを開き（`<C-t>`）、`cd` してから `<A-Up>` → OSC 7 と `LocationChangedAction` を挙げ、README を指すトーストが出る。**無反応でも待ちでもない** — *With **no** hook in `$PROFILE`, open the terminal (`<C-t>`), `cd` somewhere, press `<A-Up>` → A toast naming OSC 7 and `LocationChangedAction`, pointing at the README — **not** silence, and not a wait*
- [x] **29.2** README のフックを `$PROFILE` に貼り、新しいターミナルを開いて `cd C:\dev`、`<A-Up>` → ファイル一覧が `C:\dev` へ移動する — *Paste the README hook into `$PROFILE`, open a new terminal, `cd C:\dev`, press `<A-Up>` → The file list moves to `C:\dev`*
- [x] **29.3** 名前に**空白**を含むディレクトリと、**日本語**を含むディレクトリで同じこと → どちらも壊れずに届く — *Same with a directory whose name has a **space** and one with **Japanese** in it → Both arrive intact*
- [x] **29.5** Starship を既に入れているシェルで、フックの行を手で実行 → プロンプトはこれまでどおり描かれる（フックは `prompt` ではなく `LocationChangedAction` を使うため） — *Run the hook line by hand in a shell that already has Starship → The prompt still draws normally (the hook uses `LocationChangedAction`, not `prompt`)*
- [x] **29.6** `[term] shell` を書かず、PowerShell 7 が入っている機械で（v0.55.0）`<C-t>` と `$PSVersionTable.PSVersion` → 7.x。ペインは `pwsh` で始まり、`filer env` もペインのシェルとして `pwsh` を挙げる。`[term]` に `shell = "powershell"` と書けば 5.1 に戻る — *With no `[term] shell` and PowerShell 7 installed (v0.55.0), `<C-t>` and `$PSVersionTable.PSVersion` → 7.x — the pane started `pwsh`, and `filer env` names `pwsh` as the pane's shell. With `shell = "powershell"` in `[term]`, 5.1 again*
- [x] **29.7** `[term] shell` なしで `<C-t>`、次に `shell = "powershell"` にしてもう一度（v0.57.4）→ 最初のトーストがシェルの名前を言う: `Started pwsh — <C-t> back to the list`、次に `Started powershell (Windows PowerShell 5.1) — …`（v0.65.3。それ以前は設定で書いたときは `powershell` だけだった）。`$PSVersionTable.PSVersion` の答えと合っていること — *`<C-t>` with no `[term] shell`, then again with `shell = "powershell"` (v0.57.4) → The first toast names the shell: `Started pwsh — <C-t> back to the list`, then `Started powershell (Windows PowerShell 5.1) — …` (v0.65.3; before that the configured one said only `powershell`). It has to match what `$PSVersionTable.PSVersion` says*
- [x] **29.8** フックの無い `powershell`（5.1）で開いたペインで `<A-Up>`（v0.59.4）→ 赤いトーストがシェルの名前を挙げる（`` `powershell (Windows PowerShell 5.1)` has not said where it is … that shell's $PROFILE ``）。フックを 7 ではなく 5.1 のプロファイルに入れればよいと分かる — *In a pane started as `powershell` (5.1) with no hook, `<A-Up>` (v0.59.4) → The red toast names the shell -- `` `powershell (Windows PowerShell 5.1)` has not said where it is … that shell's $PROFILE `` -- so the hook goes into 5.1's profile, not 7's*
- [x] **29.9** `$PROFILE` に別のツールのハンドラを先に置き（`mise activate pwsh`、または代わりに `$ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = { param($s, $e) [Console]::Title = "other: $($e.NewPath)" }`）、その後に README のフックを置いて、新しいペインで `cd C:\dev`、`<A-Up>`（v0.64.2）→ 両方動く: 一覧が `C:\dev` に移り、**かつ**別のツールのハンドラも働いている（代わりのものならタイトルが `other: C:\dev`）。v0.64.2 より前は README のフックが別のものを置き換えていた — *Put a handler of another tool's in `$PROFILE` first (`mise activate pwsh`, or a stand-in: `$ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = { param($s, $e) [Console]::Title = "other: $($e.NewPath)" }`), the README hook after it, open a new pane, `cd C:\dev`, `<A-Up>` (v0.64.2) → Both run: the list moves to `C:\dev` **and** the other tool's handler still does its job (the stand-in's title reads `other: C:\dev`). Before v0.64.2 the README hook replaced the other one*

## 30. プロンプトでの右クリック貼り付け — 10 / 14

自動テスト済みなので下には出していない: 30.3

- [ ] **30.1** エクスプローラーのアドレスバーでパスをコピーし、`c`+`d` で `cd` のプロンプトを出し、入力欄を右クリック → パスが入る。`<Enter>` でそこへ移動する — *Copy a path in Explorer's address bar, press `c`+`d` (or whatever opens the `cd` prompt), right-click the field → The path appears; `<Enter>` goes there*
- [x] **30.2** `abc` と打ち、`a` と `b` の間を**右**ボタンでクリック → そこに貼られる（末尾ではない） — *Type `abc`, click between `a` and `b` with the **right** button → The paste lands there, not at the end*
- [ ] **30.4** 2 行のテキストをコピーして `s` に右クリック → 1 行になり、改行は空白として表示される（`<C-v>` と同じ） — *Copy two lines of text, right-click into `s` → One line, the break shown as a space — the same as `<C-v>`*
- [x] **30.5** 日本語のパスをコピーして `cd` に右クリック → 壊れずに入り、カーソルはその後ろに来る — *Copy a Japanese path, right-click into `cd` → Intact, and the caret sits after it*
- [x] **30.6** クリップボードに（文字ではなく）画像がある状態でプロンプトを右クリック → 何も起きない。**トーストも出ない** — *With an image (not text) on the clipboard, right-click a prompt → Nothing happens, **no toast***
- [x] **30.7** コマンドパレット、`f`、`S-r`（一括リネーム）で同じこと → どれも貼れる。一括リネームはプレビューが再描画される — *Same in the command palette, in `f`, and in `S-r` (bulk rename) → Each pastes; the bulk preview re-renders*
- [x] **30.8** **ファイル一覧**を右クリック → これまでどおりコンテキストメニュー。一覧側は変わっていない — *Right-click in the **file list** → Still the context menu — the list is unchanged*
- [x] **30.9** **ターミナル**ペイン（`<C-t>`）を右クリック → クリップボードの内容が打ち込まれ、キーを持っていなければペインがキーを受け取る — *Right-click in the **terminal** pane (`<C-t>`) → The clipboard is typed in, and the pane takes the keys if it did not have them*
- [x] **30.10** ターミナルでドラッグして選択し、そのあと右クリック → 離した時点でコピーされていて、右クリックで貼り戻される（選択でコピー、右クリックで貼り付け） — *Select text in the terminal with a drag, then right-click → The selection was copied on release; the right-click pastes it back — select to copy, right-click to paste*
- [ ] **30.11** **3 行**をコピーして、PowerShell のプロンプトのターミナルに右クリック → 3 行ともバッファに入り、`<Enter>` を押すまで**何も実行されない**（PSReadLine が bracketed paste を要求するため） — *Copy **three lines** and right-click into the terminal at a PowerShell prompt → All three sit in the buffer, **nothing runs** until `<Enter>` (PSReadLine asks for bracketed paste)*
- [x] **30.12** bracketed paste を**要求しない**シェル（`cmd.exe`）で同じこと → これまでどおり行が実行される。そして `[200~` のような余計な文字が出ない — *The same in a shell that does **not** ask for bracketed paste (`cmd.exe`) → The lines run, as they always have — and no stray `[200~` appears*
- [x] **30.13** `vim` を開いた状態のターミナルで右クリック → 文字が挿入される。画面に `[200~` が出ない — *Right-click in the terminal while `vim` is open → The text is inserted; no `[200~` on screen*
- [x] **30.14** ターミナルで `<C-v>` → 右クリックと同じ（30.11 の挙動も含めて） — *`<C-v>` in the terminal → Same as the right-click, including 23.11*
- [ ] **30.15** `report.txt` で `r`、2 つのファイルで `R`、`gSpace`（`cd`）、`a.txt` で `E`（v0.55.0）→ どれも入っている文字が選択された状態で開き、打つと置き換わる。`r` は `report`、`R` は `{name}{ext}` 全体、`cd` はパス全体、`E` は `.zip` の前の `a` を選ぶ。続けてクリップボードにパスを入れ、`cd` の選択範囲の**上**を右クリック → パスが選択範囲を置き換え、`<Enter>` でそこへ行く — *`r` on `report.txt`, `R` on two files, `gSpace` (`cd`), `E` on `a.txt` (v0.55.0); then, with a path on the clipboard, right-click **on** the `cd` prompt's selection → Each opens with its text selected, so typing replaces it: `r` selects `report`, `R` all of `{name}{ext}`, `cd` the whole path, `E` the `a` before `.zip`. The right-click's path replaces the selection, and `<Enter>` goes there*

## 31. ホストの共有一覧 — 11 / 16

- [x] **31.1** `g`+`<Space>` で `\\<サーバのアドレス>` と入力して `<Enter>` → 共有が並ぶ。エクスプローラーが見せるものと同じ — *`g`+`<Space>`, type `\\<your server's address>`, `<Enter>` → The shares are listed, the same ones Explorer shows*
- [ ] **31.2** アドレスではなくホスト**名**で、また `//` の綴りでも同じこと → どちらも届く。表示は `\\host` の綴りに戻される — *Same with a host **name** rather than an address, and with the `//` spelling → Both arrive; the path is shown back in the `\\host` spelling*
- [x] **31.3** 共有の中へ入り、`h` で出る → 共有へ入り、ホストの一覧へ戻る — *Walk into a share and back out with `h` → Into the share, then back to the host list*
- [x] **31.4** ホストの位置でもう一度 `h` → 何も動かない（ホストが最上位）。落ちない — *`h` again, at the host → Nothing moves (the host is the top), no crash*
- [x] **31.5** 電源が入っていないホスト、または存在しないホスト（自分のサブネットの未使用アドレス）→ タブは元の場所に戻り、理由がトーストで出る。ウィンドウは固まらない — *A host that is off, or does not exist (an unused address on your own subnet) → The tab returns to where it was and a toast says why — it does not hang the window*
- [ ] **31.5a** 24.1 と 24.5 をもう一度、**トーストが出るか**を見ながら → v0.16.0 は黙って親に戻っていたので、失敗が「何も起きない」に見えていた。いまは一覧かメッセージのどちらかが必ず出る。メッセージなら、その os error 番号が報告すべきもの — *24.1 and 24.5 again, watching for a **toast** → v0.16.0 fell back to the parent in silence, so a failure looked like nothing happening. Whatever the outcome, there is now either a listing or a message; if it is still a message, its os error number is the thing to report*
- [x] **31.6** この機械に資格情報を与えていないホスト → 同じく、拒否がトーストで名前付きで出る — *A host that needs a login the machine has not been given → Same: a refusal as a toast, naming it*
- [ ] **31.7** 共有が**たくさん**あるホスト（1 画面に収まらない数）→ 全部出て、普通にスクロールできる — *A host with **many** shares (more than a screenful) → All of them, scrolling normally*
- [ ] **31.8** 名前に空白や非 ASCII を含む共有 → 壊れずに出る — *A share name with a space or non-ASCII in it → Intact*
- [x] **31.9** 共有をホバーしてサイズの列を見る → 空。読むものが無く、**そこで数え続けてはいけない** — *Hover a share and look at the size column → Empty — there is nothing to read, and it must not sit there counting*
- [x] **31.10** ホストの一覧で `<C-r>` / 再読み込み → サーバに問い直す。落ちない — *`<C-r>` / refresh on the host listing → Re-asks the server; no crash*
- [x] **31.11** **もう一方のペイン**（`<C-w>`）と 2 つ目のタブでホストを開く → どちらも問題なし — *Open the host in the **other pane** (`<C-w>`) and in a second tab → Both fine*
- [x] **31.12** ホストへ行ってから、別のディレクトリへ移動する → 監視できなかったホストについて、ウォッチャが文句を言わない — *Go to a host, then change directory away → The watcher does not complain about the host it could not watch*
- [x] **31.13** 応答はするが何も共有していないホスト（v0.57.3）→ 一覧は `(empty)` ではなく `(no shares)` と言う — *A host that answers but shares nothing (v0.57.3) → The list says `(no shares)`, not `(empty)`*
- [x] **31.14** `g<Space>` で自分のサブネットの何も応答しないアドレスを打って `<Enter>`、諦める前に `<Esc>`（v0.58.1）→ すぐ元の場所に戻り、`Stopped waiting for \\<アドレス>` のトーストが出て、`j` / `k` が効く。取りやめた試みが後で時間切れになっても何も言わない — *`g<Space>` an address on your subnet that nothing answers on, `<Enter>`, then `<Esc>` before it gives up (v0.58.1) → Back where you were at once, a toast `Stopped waiting for \\<address>`, `j` / `k` work again -- and nothing more is said when the abandoned attempt times out later*
- [ ] **31.15** `g<Space>` で応答しないアドレスを打って `<Enter>`、諦める前に見出しを読む（v0.59.8）→ 見出しの件数が `listing…`、位置が `…`。空のホストに着いたように読める `0 items` と `0/0` ではない。応答が来るか `<Esc>` で戻る（31.14）と、件数が戻る — *`g<Space>` an address that does not answer, `<Enter>`, and read the header before it gives up (v0.59.8) → The header's count reads `listing…` and the position `…` -- not `0 items` and `0/0`, which read as having arrived at an empty host. Once it answers, or `<Esc>` takes the tab back (31.14), the counts return*

## 32. オープナー — 8 / 13

- [x] **32.1** README の `[opener]` / `[open]` の例を `yazi.toml` に貼って再起動し、`.txt` で `<S-Enter>` → 秀丸 / サクラ / VS Code / Neovim / 既定 が、コマンド行ではなく**説明文**で並ぶ — *Paste the README's `[opener]` / `[open]` example into `yazi.toml`, restart, `<S-Enter>` on a `.txt` → 秀丸 / サクラ / VS Code / Neovim / default — with the descriptions, not the command lines*
- [ ] **32.2** 同じファイルで `<Enter>` → 先頭の項目（秀丸）で開く。コンソールが一瞬も出ない — *`<Enter>` on the same file → Opens in the first entry (秀丸), no console flash*
- [x] **32.3** `.pdf` で `<S-Enter>` → Edge と Chrome が先に、そのあと既定アプリの項目 — *`<S-Enter>` on a `.pdf` → Edge and Chrome first, then the default-app entry*
- [x] **32.4** `.xlsx` で `<S-Enter>` して Excel を選ぶ → Excel が開く。これが `start ""` の場合で、それが無いと失敗する — *`<S-Enter>` on a `.xlsx`, pick Excel → Excel opens it — this is the `start ""` case that fails without it*
- [ ] **32.5** 名前に**空白**を含むファイルで、上のそれぞれを試す → 1 つの引数として渡り、正しく開く — *A file whose name has a **space**, through each of the above → One argument, opens correctly*
- [x] **32.6** 複数選択してから `<Enter>` → 全部がひとつの起動に渡される — *Several files selected, then `<Enter>` → All of them go to one invocation*
- [x] **32.7** `*.{xlsx,xls,csv}` と書いたルール → 3 つとも一致する（v0.17.0 以前はこれが効かなかった） — *A rule written `*.{xlsx,xls,csv}` → Matches all three (this is what did not work before v0.17.0)*
- [x] **32.8** 入っていないプログラムを指定したオープナー → 数秒以内にエラーのトースト。固まらない — *An opener naming a program that is not installed → An error toast within a few seconds, no hang*
- [ ] **32.8a** プログラムを**引用符付きのフルパス**で指定したオープナー（秀丸、サクラ）→ 開く。これが v0.17.0 のバグで、`cmd` が行を壊し、しかも失敗が無言だった — *An opener whose program is a **quoted full path** (秀丸, サクラ) → It opens. This is the v0.17.0 bug: `cmd` mangled the line and the failure was silent*
- [ ] **32.8b** 秀丸とサクラを、`<S-Enter>` から**と**先頭項目としての `<Enter>` から、両方試す → どちらも開く。同じ起動処理へ別の経路で入るため — *秀丸 and サクラ from `<S-Enter>` **and** from `<Enter>` as the first entry → Both, since they take different code paths to the same launcher*
- [x] **32.8c** パスにわざと打ち間違いを入れたオープナー → 失敗を述べるトースト。日本語版 Windows では `cmd` の文言ではなく終了コードが出るはずで、**それは仕様であって報告すべきバグではない** — *An opener with a deliberate typo in the path → A toast naming the failure. On a Japanese Windows expect the exit code rather than `cmd`'s own words — that is intended, not a bug to report*
- [ ] **32.9** アウトラインから（行の上で `<C-o>`）秀丸とサクラで開く → その行に着地する — *Open from the outline (`<C-o>` at a line) into 秀丸 and サクラ → Lands on the line*
- [x] **32.10** プログラム名を打ち間違えたオープナー（`run = 'Hidemruu.exe %s'`）を `<S-Enter>` で選ぶ（v0.59.1）→ エラーが `exit code 1` ではなく ``Open failed: `Hidemruu.exe` was not found — …`` と言う。プログラムはあるが失敗したときは今までどおり終了コードを言う — *An opener whose program is misspelled (`run = 'Hidemruu.exe %s'`), `<S-Enter>` and pick it (v0.59.1) → The error reads ``Open failed: `Hidemruu.exe` was not found — …``, not `exit code 1`. An opener whose program exists but fails still gives its exit code*

## 33. 設定の警告と、その色 — 0 / 10

自動テスト済みなので下には出していない: 33.1, 33.2, 33.3, 33.4, 33.5, 33.7, 33.8, 33.10

- [ ] **33.6** 背景の明るいテーマ → 黄色がそれでも読める。読めなければそう報告すること（いまは固定の既定値で、テーマから変えられない） — *A theme with a light background → The yellow is still readable; say so if it is not — it is a fixed default, not yet themeable*
- [ ] **33.9** 設定ファイルを**3 つ同時に**壊す → 箱が最大 5 つまで下へ積まれ、それぞれ自分の文章の大きさになり、重ならない — *Break **three** config files at once → Up to five boxes stack downward, each sized to its own text, none overlapping the next*
- [ ] **33.11** `[[preview]]` を `yazi.toml` に入れて（本来は `filer.toml`）起動 → **1 行で** `…\yazi.toml: [[preview]] belongs in filer.toml, and nothing in this file was read`。以前の `invalid type: map, expected a string` ではない（v0.33.13） — *Put `[[preview]]` into `yazi.toml` (it belongs in `filer.toml`) and start → **One line**: `…\yazi.toml: [[preview]] belongs in filer.toml, and nothing in this file was read`. Not the old `invalid type: map, expected a string` (v0.33.13)*
- [ ] **33.12** `[term]` も `yazi.toml` に入れる → それについても同じ形の行が 1 つ増える。どちらも「このファイルは読まれなかった」と述べる。実際そうだから — *Put `[term]` into `yazi.toml` as well → A second line for it, same shape. Both say the file went unread, because it did*
- [ ] **33.13** `[[preview]]` の無い、それ以外は正常な `yazi.toml` に `[term]` を入れる → `… belongs in filer.toml and was ignored`。*unread* ではなく *ignored* —— ファイルの残りは読み込まれている — *Put `[term]` into a `yazi.toml` that is otherwise fine (no `[[preview]]`) → `… belongs in filer.toml and was ignored` — *ignored*, not *unread*: the rest of the file did load*
- [ ] **33.14** `[opener]` を `filer.toml` に入れる → 逆向きの同じ警告（`belongs in yazi.toml`） — *Put `[opener]` into `filer.toml` → The same warning the other way round: `belongs in yazi.toml`*
- [ ] **33.15** 両方を正しいファイルへ移して `<C-F5>` → 警告が消える。`filer env` の表示も一致し、ターミナルペインが `[term] shell` の指すものを起動するようになる — *Move both into the right files, `<C-F5>` → No warnings. `filer env` agrees, and the terminal pane now starts what `[term] shell` names*
- [ ] **33.16** filer を**起動したまま** `%APPDATA%\filer\filer.toml` を作り、`~` を押す → そのファイルが 1 行として、警告色で `on disk, not read yet — <C-F5> re-reads config` と出る。ディレクトリは `nothing here` に**ならない**（v0.34.0） — *With filer **already running**, create `%APPDATA%\filer\filer.toml`, then press `~` → The file is a row of its own, in the warning colour, reading `on disk, not read yet — <C-F5> re-reads config`. The directory is **not** `nothing here` (v0.34.0)*
- [ ] **33.17** `<C-F5>` してから、もう一度 `~` → 普通に読み込まれたファイルの行になり、印が消える — *`<C-F5>`, then `~` again → The row is now an ordinary loaded file, no marker*
- [ ] **33.18** `config_reload` を `<F9>` に割り当て直して 33.16 を繰り返す → 行が `<C-F5>` ではなく `<F9>` を示す。メッセージに埋め込まれているのではなく、keymap から読んでいるため — *Rebind `config_reload` to `<F9>` and repeat 33.16 → The row names `<F9>`, not `<C-F5>` — it is read from the keymap, not written into the message*

## 34. ヘルプパネル自身のスクロール — 0 / 1

自動テスト済みなので下には出していない: 34.1, 34.2, 34.3, 34.4, 34.5, 34.6, 34.7, 34.8, 34.9, 34.10, 34.11, 34.12, 34.13, 34.14

- [ ] **34.15** `C`（v0.67.16）→ トースト `Copied the help panel: N keys`。クリップボードにパネルが文字で入る: `config` とパス、続いて 1 行だけの `keys`、そのあと 1 キー 1 行の `キー<TAB>説明<TAB>コマンド`。`Get-Clipboard | Select-String "^j\t"` で `j<TAB>Move cursor down<TAB>arrow 1` が見つかる。パネルは開いたまま — *`C` (v0.67.16) → Toast `Copied the help panel: N keys`. The clipboard holds the panel as text: `config` and the paths, then `keys` on a line of its own, then one `keys<TAB>description<TAB>command` line per key -- `Get-Clipboard \| Select-String "^j\t"` finds `j<TAB>Move cursor down<TAB>arrow 1`. The panel stays open*

## 35. 設定ファイルの探索場所（OS ごと） — 3 / 10

- [x] **35.1** [Windows] 両方の環境変数を未設定にして `filer env` → `%APPDATA%\yazi\config` と `%APPDATA%\filer`。**v0.34.0 から変わっていないこと。**動いていてはいけない行 — *Windows → `filer env` with both variables unset → `%APPDATA%\yazi\config` and `%APPDATA%\filer` — **unchanged from v0.34.0.** This is the row that must not have moved*
- [ ] **35.2** [Windows] `%APPDATA%\yazi\config\yazi.toml` に `[mgr] sort_by = "mtime"` を置く → 読まれる。yazi 自身のディレクトリを filer と共有したまま — *Windows → Put `[mgr] sort_by = "mtime"` in `%APPDATA%\yazi\config\yazi.toml` → Read. yazi's own directory still shares with filer*
- [ ] **35.3** [macOS] `filer env` → `~/.config/yazi` と `~/.config/filer`。`~/Library/Application Support/…` では**ない** — *macOS → `filer env` → `~/.config/yazi` and `~/.config/filer`, **not** `~/Library/Application Support/…`*
- [ ] **35.4** [macOS] yazi を入れて一度 `yazi` を起動し、yazi が読む場所に `yazi.toml` を置く → filer が同じファイルを読む。**これがこの変更の目的。**v0.35.0 以前は `~/Library/Application Support/yazi/config/` を見ていて、yazi はそこに書かない — *macOS → Install yazi, run `yazi` once, put a `yazi.toml` where yazi reads it → filer reads the same file. This is the whole point of the change: before v0.35.0 filer looked under `~/Library/Application Support/yazi/config/`, which yazi never writes*
- [ ] **35.5** [macOS] `~/Library/Application Support/filer/` に設定を置いたまま更新した場合 → **もう読まれない。**`filer env` には無いものとして出る。`~/.config/filer/` へ移すこと（CHANGELOG に**変更**として明記した） — *macOS → Anyone upgrading with config in `~/Library/Application Support/filer/` → It is **no longer read** — `filer env` lists it as missing. Move it to `~/.config/filer/`. Called out as a 変更 in CHANGELOG*
- [ ] **35.6** [Linux] `filer env` → `~/.config/yazi`。`~/.config/yazi/config` では**ない** — *Linux → `filer env` → `~/.config/yazi` — **not** `~/.config/yazi/config`*
- [ ] **35.7** [Linux / macOS] `XDG_CONFIG_HOME=/tmp/x filer env` → `/tmp/x/yazi` と `/tmp/x/filer` — *Linux / macOS → `XDG_CONFIG_HOME=/tmp/x filer env` → `/tmp/x/yazi` and `/tmp/x/filer`*
- [ ] **35.8** [Linux / macOS] `XDG_CONFIG_HOME=relative filer env` と、空にした場合 → どちらも `~/.config/…` に戻る。XDG の仕様では相対値は無視する — *Linux / macOS → `XDG_CONFIG_HOME=relative filer env`, and again with it empty → Falls back to `~/.config/…`. XDG says a relative value is ignored*
- [x] **35.9** [共通] `last-run.toml` → 状態ディレクトリ（`data_dir()`）のまま。この変更では**触っていない**。Windows では同じ `%APPDATA%\filer`、Linux では `~/.local/share/filer` — *Any → `last-run.toml` → Still in the state directory (`data_dir()`), which this change did **not** touch. On Windows that is the same `%APPDATA%\filer`; on Linux `~/.local/share/filer`*
- [x] **35.10** [共通] 別の場所から設定ディレクトリへ `filer.toml` をシンボリックリンクして `<C-F5>` → リンク越しに読まれる。**リンクのパス経由で、保存時に改名するエディタで編集したあと**もう一度確かめること（それをするとシンボリックリンクが普通のファイルに置き換わる） — *Any → Symlink `filer.toml` into the config directory from elsewhere, then `<C-F5>` → Read through the link. Re-check after editing via the **link path** with an editor that saves by rename — that replaces the symlink with a regular file*

## 36. `T` と、`<F3>` との違い — 4 / 5

自動テスト済みなので下には出していない: 36.1, 36.2, 36.3, 36.4, 36.5, 36.5a, 36.5b, 36.5c, 36.6, 36.7, 36.8, 36.9, 36.11, 36.12, 36.16, 36.17

- [ ] **36.10** `prepend_keymap` で `T` ではなく `<S-t>` を割り当てて `<C-F5>` → **どのキーを押しても何も起きない。**テストが固定している教訓そのもの。記法としては正しいので、警告も出ない — *Bind `<S-t>` instead of `T` in `prepend_keymap`, `<C-F5>` → **Nothing happens on any key** — the lesson the tests pin. No warning is printed either, because the notation is valid*
- [x] **36.13** 最大化プレビュー（`T`）の状態で `q` → 列が戻り、アプリは動き続ける — *maximized preview (`T`) → `q` → Columns back, app still running*
- [x] **36.14** `help`（`~`）、タスク一覧、spot（`Tab`）、比較（`<A-d>`）のそれぞれで `q` → 閉じるだけでアプリは動き続ける（以前から同じ。これらは元々自分のレイヤーを持っている） — *`help` (`~`), task list, spotter (`Tab`), comparison (`<A-d>`) → `q` in each → Closes, app still running (unchanged — these already had their own layer)*
- [x] **36.15** 何も開いていない状態で `q` → 1 回目で終了する — *Nothing up → `q` → Quits on the first press*
- [x] **36.18** `[[mgr.keymap]]` で `on = "Q"` / `run = "quit"` を割り当て、`<F3>` を開いた状態で `Q` → `q` と同じくまずパネルが閉じる。**挙動は文字ではなく動作に紐づいている** — *Rebind: `[[mgr.keymap]]` with `on = "Q"`, `run = "quit"`, then `Q` with `<F3>` up → Closes the panel first, like `q` — the behaviour is on the action, not the letter*

## 37. `start ""` 形式のオープナーが実際に起動するか — 6 / 8

- [x] **37.1** `browser = [{ run = 'start "" msedge %*' }]` を先頭にして `.pdf` で `<Enter>` → **Edge が PDF を開く。**コマンドプロンプトが出ない — *`<Enter>` on a `.pdf` with `browser = [{ run = 'start "" msedge %*' }]` first → **Edge opens the PDF.** No command prompt appears*
- [x] **37.2** `start "" excel %*` などを設定して `.xlsx` / `.docx` / `.pptx` で `<Enter>` → Office のアプリがファイルを開く — *`<Enter>` on `.xlsx` / `.docx` / `.pptx` with `start "" excel %*` and friends → The Office app opens the file*
- [x] **37.3** `open = [{ run = 'start "" %*' }]` に流れるものを `<Enter>` → 関連付けられたアプリが開く — *`<Enter>` on anything routed to `open = [{ run = 'start "" %*' }]` → The file's associated app opens it*
- [x] **37.4** **名前に空白を含む**ファイルで、上のそれぞれを試す → 2 つではなく 1 つのファイルとして開く。パスの引用符が保たれている — *A file whose **name contains a space**, through any of the above → Opens as one file, not two. The path keeps its quotes*
- [x] **37.5** `start "" msedge "%*"` と書いたオープナー（プレースホルダを手で引用したもの）→ 37.1 と同じ結果。プレースホルダを囲む対は変わらず吸収される — *An opener written `start "" msedge "%*"` (placeholder quoted by hand) → Same result as 37.1 — the pair around the placeholder is still absorbed*
- [x] **37.6** PDF を 2 つ選んで `<Enter>` → 引用符でひと塊にならず、別々の引数として両方開く — *Select two PDFs, `<Enter>` → Both open as separate arguments, not one quoted blob*
- [ ] **37.7** フルパスで指定したオープナー（IrfanView、サクラ、秀丸）→ 以前のまま。これらは `start` を通っていない — *Openers given as a full path (IrfanView, sakura, Hidemaru) → Unchanged — these never went through `start`*
- [ ] **37.8** PDF で `O` → Edge、Chrome、既定アプリ、そのあとエディタ群が並ぶ。それぞれ表示どおりのものが起動する — *`O` on a PDF → The picker lists Edge, Chrome, the default app, then the editors; each entry launches what it says*

## 38. フォーカスの規則、それを描く 2 つのペインで — 全 9 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 39. ターミナルペインでの `<A-j>` / `<A-k>` — 8 / 8

自動テスト済みなので下には出していない: 39.9

- [x] **39.1** `<C-t>` で長いもの（`dir /s` や `ls -R`）を実行してから `<A-k>` → スクロールバックが 1 回につき 5 行**上**へ — *`<C-t>`, run something long (`dir /s` or `ls -R`), then `<A-k>` → The scrollback goes **up** five lines per press*
- [x] **39.2** `<A-j>` → 5 行**下**へ戻る。ファイル一覧でプレビューをスクロールするのと同じ向き — *`<A-j>` → Back **down** five lines. Same direction as in the file list, where these scroll the preview*
- [x] **39.3** `<A-k>` を押し続けて最上部まで行き、`<A-j>` で戻る → 行き過ぎずに両端で止まる。戻るときの空押しが無い — *Hold `<A-k>` to the top, then `<A-j>` back → Stops at each end without overshooting — no dead presses coming back*
- [x] **39.4** `<S-PageUp>` / `<S-PageDown>` / `<S-Home>` / `<S-End>` とホイール → 以前のまま — *`<S-PageUp>` / `<S-PageDown>` / `<S-Home>` / `<S-End>`, and the wheel → Unchanged*
- [x] **39.5** ターミナルから**フォーカスを外して**（`<C-t>` で一覧へ戻って）`<A-j>` → ターミナルではなく**プレビュー**がスクロールする。キーではなくレイヤーが決めている — *With the terminal **unfocused** (`<C-t>` back to the list), `<A-j>` → Scrolls the **preview**, not the terminal. The layer decides, not the key*
- [x] **39.6** ペインの中で Alt+j を読むプログラムを動かす（`nnoremap <A-j> :m+1<CR>` を設定した `nvim`）→ v0.38.0 以降は**キーが届く**（40 節を参照）。それ以前は届かなかった — *In the pane, run a program that reads Alt+j — `nvim` with `nnoremap <A-j> :m+1<CR>` → **It does see the key** from v0.38.0 — see section 40. Before that it did not*
- [x] **39.7** `[[term.prepend_keymap]]` で `<A-j>` を `noop` に割り当てて `<C-F5>` → キーは何もせず、**シェルにも届かない。**ここで割り当てたものは消費される。返すには `[term] keymap = [...]` を丸ごと置き換える必要がある — *`[[term.prepend_keymap]]` binding `<A-j>` to `noop`, then `<C-F5>` → The key does nothing **and still does not reach the shell** — anything bound here is consumed. Handing it back needs a full `[term] keymap = [...]` replacement*
- [x] **39.8** シェルのプロンプトで Alt+b / Alt+f / Alt+d → これまでどおり readline に届く。取られたのは j と k だけ — *Alt+b / Alt+f / Alt+d at the shell prompt → Still reach readline. Only j and k were taken*

## 40. 全画面プログラムにスクロールのジェスチャーを渡す — 14 / 16

自動テスト済みなので下には出していない: 40.8

- [x] **40.1** `<C-t>` から長いファイルを `nvim` で開き、`nnoremap <A-j> :m+1<CR>` を割り当てた状態で `<A-j>` / `<A-k>` → **nvim にキーが届く。**v0.37.0 の衝突が解消されている — *`<C-t>`, `nvim` a long file, `<A-j>` / `<A-k>` with `nnoremap <A-j> :m+1<CR>` bound → **nvim sees the key.** The v0.37.0 collision is gone*
- [x] **40.2** 同じ nvim の中で `<S-PageUp>` / `<S-PageDown>` / `<S-Home>` / `<S-End>` → すべて nvim に届く。2 つだけでなく `term_scroll` のキー全部が渡される — *In the same nvim, `<S-PageUp>` / `<S-PageDown>` / `<S-Home>` / `<S-End>` → All reach nvim. Every `term_scroll` key is handed over, not just the two*
- [x] **40.3** 同じ nvim の中で `<C-t>` → **これは filer のまま。**nvim を動かしたままペインから出る。スクロール以外のキーは決して渡さない — *In the same nvim, `<C-t>` → **Still filer's** — it leaves the pane, with nvim left running. Non-scrolling keys are never handed over*
- [x] **40.4** nvim を終了してから、シェルのプロンプトで `<A-j>` / `<A-k>` → filer のスクロールバック操作に戻る。受け渡しは設定ではなくプログラムに追従している — *Quit nvim, then `<A-j>` / `<A-k>` at the shell prompt → Back to scrolling filer's scrollback. The handover follows the program, not a setting*
- [x] **40.5** nvim の中と `less` の中でホイール → 文書がスクロールする。v0.38.0 以前は存在しないスクロールバックを歩こうとして何も動かなかった — *The wheel inside nvim, and inside `less` → Scrolls the document. Before v0.38.0 it tried to walk a scrollback that does not exist, so nothing moved*
- [x] **40.6** シェルのプロンプトでホイール → これまでどおりスクロールバックを歩く — *The wheel at the shell prompt → Still walks the scrollback, unchanged*
- [ ] **40.7** 長いファイルを `less` で開いて `<S-PageUp>`、`q` で終了してからもう一度 `<S-PageUp>` → `less` の中では文書がページ送りされ、終了後はペインのスクロールバックが動く — *`less` a long file, `<S-PageUp>`, then `q` to quit, then `<S-PageUp>` again → Inside `less` it pages the document; after quitting it scrolls the pane's scrollback*
- [x] **40.9** 代替画面**かつ**アプリケーションカーソルモードを使うプログラム → ホイールの矢印が CSI ではなく SS3（`ESC O A`）で届く。挿入モードの nvim が確かめやすい — *A program using the alternate screen **and** application-cursor mode → The wheel's arrows arrive as SS3 (`ESC O A`), not CSI. nvim in insert mode is the easy check*
- [x] **40.10** ペインの `bash` / `zsh` プロンプトで数語打ってから `Alt-b` / `Alt-f` → カーソルが**単語単位**で動く。v0.38.0 以前は何も起きなかった（キーが、送るバイトを持たないまま捨てられていた） — *At a `bash`/`zsh` prompt in the pane, type a few words, then `Alt-b` / `Alt-f` → The cursor moves **by word**. Before v0.38.0 nothing happened — the key was dropped with no bytes behind it*
- [x] **40.11** 同じプロンプトで `Alt-d` → 前方の単語を削除する — *`Alt-d` at the same prompt → Deletes the word ahead*
- [ ] **40.12** ペインの PowerShell（PSReadLine）で `Alt-b` / `Alt-f` → 同じ単語移動 — *PowerShell (PSReadLine) in the pane, `Alt-b` / `Alt-f` → Same word motions*
- [x] **40.13** 普通のプロンプトで `Alt-j` / `Alt-k` → **これは filer のスクロールのまま。**この 2 つは `[term]` レイヤーに割り当てられていて、プロンプトは代替画面ではないため — *`Alt-j` / `Alt-k` at an ordinary prompt → **Still filer's scroll** — these two are bound in the `[term]` layer, and the prompt is not the alternate screen*
- [x] **40.14** `FILER_PTY_LOG` を設定して nvim の中でホイール（v0.55.0）→ nvim の表示がスクロールし、**カーソルは同じ行のまま**（前後で `:echo line('.')`）。ログには `\e[A` ではなく `[<64;…M` / `\e[<65;…M` が出る — *The wheel inside nvim, with `FILER_PTY_LOG` set (v0.55.0) → nvim's view scrolls and **its cursor stays on the same line** (`:echo line('.')` before and after). The log shows `\e[<64;…M` / `\e[<65;…M`, not `\e[A`*
- [x] **40.15** ペインで lazygit を動かし（v0.55.0）、`<S-End>` を毎秒 30 回で約 300 回、その途中に `?` と `<Esc>` を入れる（#99 が #93 を組み直した形）→ `<Esc>` から 1 秒以内にキー一覧が閉じる。v0.55.0 までは数分開いたままだった。レコードで送った `<Esc>` の直後に `\e[1;2F` の `<S-End>` が来ると、tcell には 1 つの列に読めていた — *lazygit in the pane (v0.55.0): about 300 `<S-End>` at 30 a second, with `?` then `<Esc>` in the middle of them, as #99 rebuilt #93 → The key list closes within a second of the `<Esc>`. Until v0.55.0 it stayed open for minutes: `<Esc>` sent as a record and `<S-End>` as `\e[1;2F` right behind it read to tcell as one sequence*
- [x] **40.16** `FILER_PTY_LOG` を設定して（v0.55.0）ペインを開き、`ping -t localhost` と打って `<C-c>`、続けて打った語の上で `<C-Left>` と `<Tab>` の補完 → ログの `out` の最初のほうに `\e[?9001h` があり、`in key` の行は `\e[1;5D` ではなくレコード（`\e[…;…;…;1;…;1_`）。`<C-c>` で ping が止まり、`<C-Left>` は 1 語戻り、`<Tab>` は補完する。シェルはレコードを本物のキーボードと同じに読む — *With `FILER_PTY_LOG` set (v0.55.0): open the pane, type `ping -t localhost`, `<C-c>`, then `<C-Left>` over a typed word and `<Tab>` completion → The log's `out` lines hold `\e[?9001h` near the start, and the `in key` lines are records (`\e[…;…;…;1;…;1_`) rather than `\e[1;5D`; `<C-c>` stops the ping, `<C-Left>` moves by a word, `<Tab>` completes — the shell reads records as it reads a real keyboard*
- [x] **40.17** `FILER_PTY_LOG` を設定して、nvim の中でホイールを 1 ノッチ、次に 3 ノッチ（v0.58.1）→ ノッチ 1 つにつき `\\e[<64;…M`（または `65`）が 1 本: 1 本、次に 3 本。v0.58.1 までは平滑化した量を数えていたので、3 ノッチで 5 本出ていた — *With `FILER_PTY_LOG` set, inside nvim, one notch of the wheel; then three (v0.58.1) → One `\e[<64;…M` (or `65`) per notch: 1, then 3. Until v0.58.1 three notches sent five, because the smoothed delta was counted*

## 41. spot パネルの 4 つの provider — 10 / 14

- [x] **41.1** fixtures の `.zip` で `<Tab>` → **Archive** セクション（形式、エントリ数とフォルダ数、展開後サイズ、圧縮率、`Encrypted: no`） — *`<Tab>` on a `.zip` from the fixtures → An **Archive** section: format, entry and folder counts, unpacked size, ratio, `Encrypted: no`*
- [x] **41.2** **7-Zip で暗号化した** zip で `<Tab>` → `Encrypted: yes (entries need a password)` と出て、件数も出ている。**単体テストにできない**（この `zip` のビルドに AES 書き込みが無く、暗号化した fixture をツリー内で作れない） — *`<Tab>` on a zip made **encrypted by 7-Zip** → `Encrypted: yes (entries need a password)`, and the counts are still there. **Cannot be unit-tested — this build of `zip` has no AES writer, so no encrypted fixture can be made in-tree***
- [x] **41.3** 「ファイル名も暗号化」で作った 7z で `<Tab>` → `Encrypted: yes (the listing itself)` と出て、**件数は一切出ない**（その先が分からないため） — *`<Tab>` on a 7z made with "encrypt file names" → `Encrypted: yes (the listing itself)` and **no counts at all** (nothing below is known)*
- [x] **41.4** エントリが 2 万を超える書庫で `<Tab>` → ウィンドウが止まらずにパネルが出て、`Scanned: first 20,000 entries` と述べる — *`<Tab>` on an archive with more than 20,000 entries → The panel arrives without the window stalling, and says `Scanned: first 20,000 entries`*
- [ ] **41.5** メモ帳で CRLF 保存したファイルと LF のファイルで `<Tab>` → `Line endings` の行が件数付きで区別する — *`<Tab>` on a CRLF file saved by Notepad, then on an LF one → The `Line endings` row tells them apart, with counts*
- [ ] **41.6** メモ帳の「UTF-16 LE」保存で `<Tab>` → `Encoding: UTF-16 LE`、`BOM: UTF-16 LE (FF FE)`。バイナリ扱いに**ならない** — *`<Tab>` on a Notepad "UTF-16 LE" save → `Encoding: UTF-16 LE`, `BOM: UTF-16 LE (FF FE)` — **not** treated as binary*
- [x] **41.7** 2GB のログで `<Tab>` → 行がすぐ出て `Scanned: first 1.0 M of …` となり、**`Final newline` の行は出ない**（末尾を読んでいないため） — *`<Tab>` on a 2 GB log → Rows arrive promptly, `Scanned: first 1.0 M of …`, and **no `Final newline` row** (the end was never read)*
- [x] **41.8** リリースの 6 つのバイナリそれぞれで `<Tab>` → `Architecture` が成果物の名前のトリプルと一致する（`x86_64` / `aarch64`） — *`<Tab>` on each of the six release binaries → `Architecture` matches the triple the artifact is named for — `x86_64` / `aarch64`*
- [x] **41.9** `C:\Windows\explorer.exe` と、`.dll` で `<Tab>` → `Windows GUI` / `DLL` — *`<Tab>` on `C:\Windows\explorer.exe`, then on a `.dll` → `Windows GUI` / `DLL`*
- [x] **41.10** Office で保存した本物の `.docx` / `.xlsx` / `.pptx` で `<Tab>` → 作成者、リビジョン、**UTC** と明記された時刻、語数 / ページ数 / スライド数 — *`<Tab>` on a real `.docx` / `.xlsx` / `.pptx` saved by Office → Author, revision, times marked **UTC**, word / page / slide counts*
- [x] **41.11** 古い `.doc` で `<Tab>` → **Document セクションが出ず、エラーも出ない** — *`<Tab>` on an old `.doc` → **No Document section, and no error***
- [ ] **41.12** 新しい各セクションのキーの列を見る → キーが値の列にはみ出していない（`overlay.rs` が `key_w = 130.0` で固定している） — *Look at the key column on every new section → No key runs into the value column (`overlay.rs` hard-codes `key_w = 130.0`)*
- [x] **41.13** 新しいセクションの行まで `<A-j>` で下り、`y` → 正しい値がコピーされる。**`Act::Copy` は全セクションを通して行を数えるので、セクションが増えると添字がずれる** — *`<A-j>` down into a new section's rows, then `y` → The right value is copied. **`Act::Copy` counts rows across every section, so the new sections shift the indices***
- [ ] **41.14** 遅いネットワークドライブ上のフォルダで `<Tab>` → パネルはカーソルに追従し続ける（spot ワーカーは newest-wins） — *`<Tab>` on a folder on a slow network drive → The panel still follows the cursor; the spot worker is newest-wins*

## 42. ミニマップのホバーカード — 全 13 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 43. CSV / TSV を表として見る — 0 / 1

自動テスト済みなので下には出していない: 43.1, 43.2, 43.3, 43.4, 43.5, 43.6, 43.7, 43.8, 43.10, 43.11, 43.12, 43.13

- [ ] **43.9** 50MB の CSV → すぐ開き、`max_text_bytes` で切られ、フッタが省略したと述べる — *A 50 MB CSV → Opens promptly, cut at `max_text_bytes`, footer says truncated*

## 44. ディスク使用量 — 16 / 16

自動テスト済みなので下には出していない: 44.7, 44.15, 44.18

- [x] **44.1** `node_modules` のあるプロジェクトで `gu` → 子が大きい順に棒付きで並ぶ。`node_modules` が上位に来て、合計が自身のエントリよりはるかに大きい — *`gu` in a project with a `node_modules` → Children largest first, with bars; `node_modules` near the top with a total far bigger than its own entry*
- [x] **44.2** `gu` してから `<Esc>` → 元のディレクトリに戻り、カーソルも元の位置。走査は止まる（抜けたあと CPU を使わない） — *`gu`, then `<Esc>` → Back in the directory, cursor where it was. The walk stops (no CPU after leaving)*
- [x] **44.3** 30 万ファイル超のツリーで `gu` → 完了し、走査を打ち切ったことと合計が下限値であることを述べる — *`gu` on a tree with 300k+ files → Finishes, and says the walk was cut short and the totals are floors*
- [x] **44.4** `.gitignore` された `target/` や `build/` を含むフォルダで `gu` → **数に入る**（除外されない） — *`gu` in a folder holding a `.gitignore`d `target/` or `build/` → It is **counted**, not skipped*
- [x] **44.5** 容量の大半を隠しフォルダが占める場所で `gu` → 数に入り、表示もされる — *`gu` where a hidden folder holds most of the space → It is counted, and visible*
- [x] **44.6** 大きなツリーへのシンボリックリンク / ジャンクションを含むフォルダで `gu` → リンクは 1 エントリ扱いでツリーの二重計上にならず、固まらない — *`gu` on a folder with a symlink/junction to a big tree → The link is one entry, not a second copy of the tree, and no hang*
- [x] **44.8** ネットワーク共有（UNC）で `gu` → 答えるか、穏当に失敗する。走査の途中でも `<Esc>` で抜けられる — *`gu` on a network share (UNC) → Answers or fails gracefully; `<Esc>` still gets out mid-walk*
- [x] **44.9** `gu` のあと `j`/`k`、`y`、`d`、スペースで選択 → 通常の一覧キーが全部効く。**パネルではなく一覧そのもの**だから — *`gu`, then `j`/`k`, `y`, `d`, space to select → All the ordinary list keys work — this is the list, not a panel*
- [x] **44.10** `gu` のあとフォルダで `Enter`、続けて `h`、もう一度 `h`（v0.63.0。それ以前は `Enter` でビューを抜けた）→ `Enter`: ビューのまま、そのフォルダを測る（ヘッダのパスがそのフォルダ）。1 回目の `h`: 1 つ上に戻り、ビューのまま、出てきたフォルダにカーソル。2 回目の `h`（`gu` を押したフォルダ）: ビューが閉じて普通の一覧に戻る — *`gu`, then `Enter` on a folder; then `h`, and `h` again (v0.63.0; before that `Enter` left the view) → `Enter`: the view stays and measures that folder (the header path is the folder). First `h`: back up, still in the view, the cursor on the folder it left. Second `h`, in the folder `gu` was pressed in: the view closes and the ordinary listing is back*
- [x] **44.11** 使用量ビューを開いたまま `gu` → メッセージを添えて断られる（戻れないビューにならない） — *`gu` while a usage view is already up → Refused with a message, not a view with no way back*
- [x] **44.12** `gu` のあと `,` で並べ替え直す → 指示どおり順序が変わる。`<Esc>` してからもう一度 `gu` すれば大きい順に戻る（ビューの中の `gu` は断られる。44.11） — *`gu`, then `,` to re-sort → The order changes (as asked); `<Esc>` and `gu` again restores largest-first (`gu` inside the view is refused, 44.11)*
- [x] **44.13** フォルダの合計をエクスプローラーのプロパティと比べる → 丸め誤差の範囲で一致。**ハードリンクは多めに出るが、それは仕様として文書化してある** — *Compare a folder's total against Explorer's own properties → Within rounding. **Hard links read high — that is documented, not a bug***
- [x] **44.14** タブを `linemode mtime`（`m t`）にして `gu`、すぐ `<Esc>`（v0.56.0）→ 行には日付ではなくサイズ（`1.5 M`、`6.0 K`）が出る。`<Esc>` の後は日付に戻り、`Measuring…` のトーストもすぐ消える — *With the tab on `linemode mtime` (`m t`), `gu`, then `<Esc>` straight away (v0.56.0) → The rows show sizes (`1.5 M`, `6.0 K`), not dates; after `<Esc>` the list shows dates again, and the `Measuring…` toast is gone at once*
- [x] **44.16** 数秒かかる大きさの木で `gu` して、見出しを見る（v0.57.3）→ 歩いている間は `N measured so far` で増えていき、合計のトーストが出たら `N items` になる — *`gu` on a tree big enough to take seconds, and watch the header (v0.57.3) → `N measured so far`, growing, while it walks; `N items` once the total's toast is up*
- [x] **44.17** `gu` のあと、ビューの中で `m t`、続けて `m u`（v0.58.0）→ `m t` で数字が日付に替わり棒だけが残る。`m u` で**歩き直さずに**サイズが戻る（`Measuring…` が出ない）。`<Esc>` でタブ自身の表示に戻るのは今までどおり — *`gu`, then `m t` inside the view, then `m u` (v0.58.0) → `m t` swaps the numbers for dates with the bars left; `m u` brings the sizes back **without** walking again (no `Measuring…`). `<Esc>` still gives the tab its own mode back*
- [x] **44.19** 数秒かかる木で `gu` し、合計のトーストが消えるまで待つ（v0.59.2）→ ビューを開いている間ずっと、見出しが `N items · <大きさ> total` と言う（ここが使用量ビューだと分かる唯一の印）。その木で `filer --keys "gu<Wait:0>j"` とすると、カーソルは歩きが終わってから動く — *`gu` on a tree that takes seconds, then wait past the total's toast (v0.59.2) → The header reads `N items · <size> total` for as long as the view is up -- the one sign left that this is the usage view. And `filer --keys "gu<Wait:0>j"` on that tree moves the cursor only after the walk is done*

## 45. 2 つのフォルダを比べる — 13 / 14

自動テスト済みなので下には出していない: 45.1, 45.2, 45.5, 45.7

- [x] **45.3** `n` / `N` → `=` でない行の間を歩き、一致した行は飛ばす。末尾ではその旨を述べる — *`n` / `N` → Walks between the rows that are not `=`, skipping matches. At the end it says so*
- [x] **45.4** `gg` / `G` → 最初の行と最後の行 — *`gg` / `G` → First and last row*
- [x] **45.6** 最後の 1 バイトだけが違うファイルを含むツリー → その行が `=` ではなく `~` になる — *A tree where one file differs in its last byte only → That row is `~`, not `=`*
- [x] **45.8** 片方ではフォルダ、もう片方では同名のファイル → `~` — *A folder on one side where the other has a file of that name → `~`*
- [x] **45.9** ファイル 1 つとフォルダ 1 つを選んで `<A-d>` → `compare two files, or two folders — not one of each` と断られる — *Select one file and one folder, `<A-d>` → Refused with "compare two files, or two folders — not one of each"*
- [x] **45.10** `node_modules` 同士（10 万パス超）→ 答えが出るか、打ち切ったと述べる。ウィンドウは固まらない — *Two `node_modules` (100k+ paths) → Answers, or says it was cut short; the window does not freeze*
- [ ] **45.11** シンボリックリンクの指す先だけが違う 2 つのツリー → そのリンクの行が「異なる」と読める — *Two trees differing only in where a symlink points → The link row reads as differing*
- [x] **45.12** 2 分割して各ペインでフォルダの上に立ち、`<A-d>` → その 2 つが比較される — *Split the view, stand on a folder in each pane, `<A-d>` → Compares those two*
- [x] **45.13** `q` / `<Esc>` → 閉じる。そして 2 つの**ファイル**はこれまでどおり行単位で比較できる — *`q` / `<Esc>` → Closes, and two **files** still compare line by line as before*
- [x] **45.14** 数百のパスのうち奥の 1 ファイルだけが違う 2 つのツリーを比べる（v0.53.0）→ カーソルが**そのファイルの上**で開く（先頭の行ではない）。差分の無い組は先頭で開く — *Compare two trees of hundreds of paths that differ in one file far down (v0.53.0) → The view opens with the cursor **on that file**, not on the first row. A pair with no differences opens at the top*
- [x] **45.15** `z`、続けて `j` / `n`、もう一度 `z` → `=` の行が一覧から消える。フッタは数え続け、`matches hidden (z)` が付く。`j` と `n` は見えている行だけを歩く。2 回目の `z` で全部の行が戻り、カーソルは同じパスの上にある — *`z`, then `j` / `n`, then `z` again → The `=` rows leave the list; the footer still counts them and adds `matches hidden (z)`; `j` and `n` step only over what is shown; the second `z` brings every row back with the cursor on the same path*
- [x] **45.16** 中の `t1` を指す**ジャンクション**（`mklink /J ln t1`）を持つフォルダを写し、元と写しを比べる（v0.55.0）→ `= ln`。行き先の文字列は違っても、どちらも自分のツリーの `t1` に着くので同じと読む — *Copy a folder holding a **junction** to a folder inside it (`mklink /J ln t1`), then compare the original with the copy (v0.55.0) → `= ln`: both links land on `t1` in their own tree, so the copies read as the same even though the two targets differ as text*
- [x] **45.17** 別の場所にある同じ名前の 2 つのフォルダ（片方にサブフォルダあり）を比べる（v0.59.4）→ 見出しの下に両方の**フルパス**（`…\left\proj  ↔  …\right\proj`）。長ければそれぞれ真ん中が切られ、両端は読める。フォルダの行は子のパスと同じく `\` で終わり、`/` ではない — *Compare two folders of the same name in different places, one holding a subfolder (v0.59.4) → Under the title, both **full paths** (`…\left\proj  ↔  …\right\proj`), each cut in its middle if long so both ends stay readable. A folder row ends in `\` like its children's paths, not `/`*
- [x] **45.18** 2 つのフォルダを比べ、`≠` のファイルの行にカーソルを置いて `<Enter>`、続けて `q`（v0.61.0）→ 2 つのファイルが行単位で横に並び、題に両方のフルパスが出る。`q` でフォルダの比較の**同じ行**に戻る（閉じない、先頭に戻らない）。片側にしか無い行で `<Enter>` を押すと `Compare: it is on one side only` と出て動かない — *Compare two folders, put the cursor on a `≠` file row, `<Enter>`; then `q` (v0.61.0) → The two files open side by side, line by line, titled with both full paths. `q` goes back to the folder comparison **on the same row**, not closed and not at the top. `<Enter>` on a row that exists on one side only says `Compare: it is on one side only` and stays*

## 46. spot パネルの Git セクション — 15 / 21

準備:

```powershell
# git リポジトリの中で実行すること。fixtures の `repo` がそれ
cd $HOME\Desktop\filer-fixtures\repo
git log -1 --format="%h %an %ad %s"    # 期待値の答え合わせ用
```

- [x] **46.1** コミット済みのファイルで `<Tab>` → **Git** セクションが出る。`Last change` が短いハッシュと `YYYY-MM-DD HH:MM`、続いて `Subject` と `Author` — *`<Tab>` on a committed file → A **Git** section: `Last change` is a short hash and `YYYY-MM-DD HH:MM`, then `Subject` and `Author`*
- [x] **46.2** `git log -1 -- <そのファイル>` と突き合わせる → 同じコミット。リポジトリの最新ではなく、**そのパスに触れた最新のもの** — *Check it against `git log -1 -- <that file>` → The same commit. Not the repository's newest — **the newest that touched this path***
- [x] **46.3** 複数のコミットが触れたファイルで `<Tab>` → `Commits` が件数付きで出る — *`<Tab>` on a file changed by more than one commit → `Commits` appears with the count*
- [x] **46.4** ちょうど 1 つのコミットで追加されたファイルで `<Tab>` → **`Commits` の行が出ない。**1 件では日付以上のことを言っていないため — *`<Tab>` on a file added by exactly one commit → **No `Commits` row** — one says nothing the date has not*
- [x] **46.5** そのパスに触れたコミットが 50 件以上ある履歴のファイルで `<Tab>` → `Commits` が `50+` と出る（間違った合計ではない）。ルート近くのディレクトリで履歴全部ではなく 1 ページ分だけ読むための上限 — *`<Tab>` on a file in a history of 50+ commits touching it → `Commits` reads `50+`, not a wrong total. The cap is there so a directory near the root reads a page, not the whole history*
- [ ] **46.6** **ディレクトリ**で `<Tab>` → その中の何かに触れた最後のコミット — *`<Tab>` on a **directory** → The last commit that touched anything inside it*
- [x] **46.7** 新規で未コミットのファイル（`git status` で `?`）で `<Tab>` → **Git セクションが出ない。**履歴の中に触れたものが無いため — *`<Tab>` on a file that is new and never committed (`git status` shows `?`) → **No Git section at all** — nothing in the history touches it*
- [x] **46.8** リポジトリでない場所で `<Tab>` → Git セクションが出ず、パネルの描画前に待ちも発生しない — *`<Tab>` somewhere that is not a repository → No Git section, and no pause before the panel draws*
- [x] **46.9** `git` が `PATH` に無い機械で同じこと → Git セクションが出ず、エラーも出ず、パネルの他の部分にも影響しない — *The same on a machine with no `git` on `PATH` → No Git section, no error, and the rest of the panel is unaffected*
- [x] **46.10** 最後のコミットの件名に日本語や絵文字が入っているファイルで `<Tab>` → 文字化けせずそのまま描かれる（書式が NUL 区切りなので引用が要らない） — *`<Tab>` on a file whose last subject has Japanese in it, or an emoji → Drawn intact, not mojibake — the format is NUL-separated so nothing needs quoting*
- [x] **46.11** コンソールの窓が出ないか見張る → **一瞬も出ない。**`git` は status ワーカーと同じく `CREATE_NO_WINDOW` で起動される — *Watch for a console window → **None flashes.** `git` is spawned with `CREATE_NO_WINDOW`, the same as the status worker*
- [x] **46.12** マージ済みの pull request 経由で入ったコミットを持つファイルで `<Tab>` → `Came in via` が `#<n>` と merge の短いハッシュ、`From branch` がブランチ名 — *`<Tab>` on a file whose commit arrived through a merged pull request → `Came in via` reads `#<n>` then the merge's short hash, and `From branch` names the branch*
- [x] **46.13** `#<n>` を GitHub の pull request と突き合わせる → **同じ番号**で、そのファイルがその PR の差分に入っている。番号は merge コミットの件名から読んでいて何も取得していないので、**件名が出所であることを示すのがこの行** — *Check `#<n>` against the pull request on GitHub → **The same number**, and the file is in that pull request's diff. The number is read out of the merge commit's subject — nothing is fetched, so this is the row that proves the subject is the source*
- [x] **46.14** 最後のコミットが **`main` に直接** push されたファイルで `<Tab>` → **`Came in via` も `From branch` も出ない。**履歴の行だけ。あとから来ただけの merge に帰属させてはいけない — *`<Tab>` on a file whose last commit was pushed **straight to `main`** → **No `Came in via` and no `From branch`** — the history rows only. A merge that merely came later must not be credited*
- [x] **46.15** 現在のブランチでコミットし **まだマージしていない**ファイルで `<Tab>` → 同じく履歴の行だけで `Came in via` は出ない。まだどこにも到着していないので訊く相手がいない — *`<Tab>` on a file committed on the current branch and **not merged yet** → The same: history rows, no `Came in via`. It has not arrived anywhere to be asked about*
- [ ] **46.16** 回線を抜く、Wi-Fi を切る、または Windows ファイアウォールで `filer.exe` と `git.exe` の外向き通信を塞ぐ —— そのうえで 46.12 をやり直す → **同じ出力・同じ速さ。**ここは何もマシンの外に出ない。ファイアウォールの方法は、実機で動くセッション向け（前の 2 つだとセッション自体が切れる） — *Pull the network cable, turn off Wi-Fi, or block `filer.exe` and `git.exe` outbound in Windows Firewall — then repeat 46.12 → **Identical output, at the same speed.** Nothing here leaves the machine. The firewall form is for a session on the machine, which the other two would cut off*
- [ ] **46.17** spot パネルで `C`（v0.52.0）→ 全部の行が `ラベル<TAB>値` の形でクリップボードに入る。節ごとに見出しの下、節と節の間は空行。トーストが行数を言う — *In the spot panel, `C` (v0.52.0) → Every row is on the clipboard as `Label<TAB>value`, under each section's title, sections a blank line apart. The toast counts the rows*
- [ ] **46.18** GitHub のリポジトリの clone で、46.12 のファイルの `Pull request` の行 → 上の `#<n>` の `https://github.com/<owner>/<repo>/pull/<n>` と出る。その行か `Came in via` の行で `<Enter>` を押すと、そのページがブラウザで開く — *On a 46.12 file in a clone of a GitHub repository, the `Pull request` row → It reads `https://github.com/<owner>/<repo>/pull/<n>` for the `#<n>` above it. `<Enter>` on it — or on `Came in via` — opens that page in the browser*
- [ ] **46.19** `origin/HEAD` のある clone で、46.15 のファイル（コミット済み・未マージ）→ **`Not merged`** の行に `not in origin/main yet`（その clone の既定のブランチ）。46.14 のファイル（main に直接）にはこの行が**無い**ので、2 つが同じ見た目ではなくなった — *On a 46.15 file (committed, not merged) in a clone that has `origin/HEAD` → A **`Not merged`** row: `not in origin/main yet` (the clone's own default branch). A 46.14 file (straight to main) has **no** such row, so the two no longer look alike*
- [ ] **46.20** `origin/HEAD` の無いリポジトリ（`git remote set-head origin -d`）で同じこと → `Not merged` の行は出ない。既定のブランチを推測しない — *The same in a repository with no `origin/HEAD` (`git remote set-head origin -d`) → No `Not merged` row at all — filer does not guess the default branch*
- [x] **46.21** プルリクエストで入ったファイルで `<Tab>`、カーソルを `From branch` に置いて `<Enter>`（v0.59.1）→ ブラウザが枝のページ（`…/tree/<枝>`）を開き、トーストが `Opened …` と言う。マージ後に消された枝なら GitHub 自身の 404 が開くが、アドレスとしては正しい — *On a file that came in through a pull request, `<Tab>`, the cursor on `From branch`, `<Enter>` (v0.59.1) → The browser opens the branch's page (`…/tree/<branch>`), and the toast says `Opened …`. A branch deleted after the merge opens GitHub's own 404, which is still the right address*

## 47. 放置した窓は CPU を使わない — 4 / 5

- [x] **47.1** ファイルとサブフォルダのあるフォルダで filer を開き、10 秒何も触らない。`(Get-Process filer).CPU` を 10 秒あけて 2 回読む → 2 つの差は **1 秒よりずっと小さい**（100 分の数秒なら普通） — *Open filer on a folder of files and subfolders, touch nothing for 10 s, then read `(Get-Process filer).CPU` twice, 10 s apart → The two readings differ by **well under 1 s** (a few hundredths is normal)*
- [x] **47.2** `j` でファイルに乗り、すぐ（40 ms のデバウンスの内に）`j` でサブフォルダに乗って手を離す。CPU を 10 秒あけて 2 回読む → 同じく**増えない**。v0.54.2 より前は、これが描き続けたまま止まらなくなる手順だった — *`j` onto a file and at once `j` onto a subfolder (inside the 40 ms debounce), then hands off; read the CPU twice, 10 s apart → The same: **no rise**. Before v0.54.2 this was the sequence that left it drawing for ever*
- [x] **47.3** 47.2 のあと窓を最小化する → 最小化していても増えない — *The same as 47.2, then minimise the window → Still no rise while minimised*
- [ ] **47.4** 47.1〜47.3 でまだ増えるとき: `Get-Process filer \| % Threads \| sort TotalProcessorTime -desc \| select -first 3 Id, TotalProcessorTime` を 10 秒あけて 2 回 → どのスレッドの時間が増えるかと、分かれば開始アドレスを報告する。次に調べるのはそのスレッド — *If 47.1-47.3 still rise: `Get-Process filer \| % Threads \| sort TotalProcessorTime -desc \| select -first 3 Id, TotalProcessorTime`, twice, 10 s apart → Report which thread's time grows, and its start address if a tool can name it. That thread is the next thing to look at*
- [x] **47.5** `f` のプロンプトを開いて 10 秒何も触らず、前後で CPU を読む（v0.59.3）→ プロンプトを開いていないとき（47.1）と同じく増えない。キャレットは点滅せず常に出ている。v0.59.3 までは点滅のために毎秒 2 回描いていて、10 秒で 0.14〜0.30 CPU 秒（#103、#110） — *Open the `f` prompt, touch nothing for 10 s, and read the CPU before and after (v0.59.3) → No rise, as with no prompt open (47.1). The caret is steady rather than blinking. Until v0.59.3 the blink drew twice a second: 0.14-0.30 CPU-s per 10 s (#103, #110)*

## 48. The release zips (v0.64.0) — 6 / 6

- [x] **48.1** 展開した各フォルダで `Get-ChildItem -Recurse` → フォルダが 1 つ（`filer-<タグ>-windows-x64` か `-arm64`）で、中はちょうど 4 ファイル: `filer.exe`、`conpty.dll`、`OpenConsole.exe`、`ConPTY-LICENSE.txt`。他には何も無く、フォルダの横にも何も無い — *`Get-ChildItem -Recurse` in each extracted folder → One folder, `filer-<tag>-windows-x64` (or `-arm64`), holding exactly four files: `filer.exe`, `conpty.dll`, `OpenConsole.exe` and `ConPTY-LICENSE.txt`. Nothing else, and nothing at the top level beside the folder*
- [x] **48.2** 各フォルダで `.\filer.exe --version` → x64 の zip は `filer <版> (x86_64)`、ARM64 の zip は `filer <版> (aarch64)`。版はタグから `v` を除いたもの — *`.\filer.exe --version` from each folder → `filer <version> (x86_64)` from the x64 zip and `filer <version> (aarch64)` from the ARM64 one, the version being the tag without its `v`*
- [x] **48.3** 各 zip の 3 つのバイナリの PE machine（節の冒頭の式）→ x64 の zip は 3 つとも `8664`、ARM64 の zip は 3 つとも `AA64`。混ざっていたらこの行が探している不具合（ARM64 版に x64 の ConPTY が入ると、起動はしてペインでおかしくなる） — *The PE machine (above) of all three binaries in each zip → `8664` for all three in the x64 zip, `AA64` for all three in the ARM64 one. A mixed zip is the bug this row exists for: the ARM64 build with an x64 ConPTY would start and then misbehave in the pane*
- [x] **48.4** `ConPTY-LICENSE.txt` を読む → リリースのコミットの `scripts/fetch-conpty.ps1` が固定している版（`$version`）が書いてあり、`{VERSION}` が残っていない — *Read `ConPTY-LICENSE.txt` → Names the version `scripts/fetch-conpty.ps1` pins (`$version`) on the release's commit, and no `{VERSION}` is left in it*
- [x] **48.5** 各 zip と展開した各ファイルに `Get-FileHash -Algorithm SHA256` → リリースページ末尾の **SHA-256** の表の、そのファイルの行と全部一致する。表が無ければ `sums` ジョブが動かなかったということなので、そう書く — *`Get-FileHash -Algorithm SHA256` on each zip and on each extracted file → Every hash equals the row for that file in the **SHA-256** table at the end of the release page. A missing table means the `sums` job did not run: say so*
- [x] **48.6** x64 のフォルダの `filer.exe` を起動してペインを開き（`<C-t>`）、プロセスのモジュールを見る: `(Get-Process filer).Modules \| ? ModuleName -eq conpty.dll \| % FileName` → **そのフォルダの** `conpty.dll` で、`C:\Windows` の下のものではない。zip はそのためにある — *Start `filer.exe` from the x64 folder, open the pane (`<C-t>`), and list the process's modules: `(Get-Process filer).Modules \| ? ModuleName -eq conpty.dll \| % FileName` → The `conpty.dll` **in that folder**, not one under `C:\Windows`. That is what the zip is for*
