# 実機チェックリスト

`TESTING.md` から `cargo run --example make-testcheck` で生成している。**正は TESTING.md**（英語）で、
このファイルはそれを日本語で並べ替えたもの。食い違ったら TESTING.md を信じること。各行の
末尾の *斜体* が TESTING.md の原文で、訳はその手前にある。

**チェック（`[x]`）だけは手で書いてよく、生成し直しても残る。**それ以外を書き換えても次の
生成で消える。

**0 / 359 済み。**（TESTING.md の全 516 件のうち、`cargo test` が見ている 157 件は
「押すもの」から外してある）

未訳 351 件は原文のまま `〔未訳〕` を付けて出している。

## 使い方

1. `filer.exe` と、`scripts\make-fixtures.ps1` が作るテスト用ファイルを用意する（詳しくは TESTING.md の
   「What you need」）。
2. 節ごとに「準備」を走らせてから、上から押していく。
3. 期待どおりなら `[ ]` を `[x]` にする。違ったら `<F12>` で issue を出すか、そのまま書き留める。
4. 節の見出しの `3 / 12` は、その節で人が押す分の進捗。

キーの網羅は別ファイル（[TESTING-KEYS.md](TESTING-KEYS.md)）で、こちらは「1 つのキーでは
確かめられない振る舞い」の側。

## 1. ターミナルペイン — 0 / 30

準備:

```powershell
# `many\` と `repo` は make-fixtures.ps1 が作る
cd $HOME\Desktop\filer-fixtures
```

- [ ] **1.1** `<C-t>` from the file list → A shell opens along the bottom, already in the directory the list is showing 〔未訳〕
- [ ] **1.2** Type `dir` and press Enter → Output in the list's own font, columns lined up, no overlapping glyphs 〔未訳〕
- [ ] **1.3** Look at the cursor → A block where the shell's cursor is, and it moves as you type 〔未訳〕
- [ ] **1.3a** `<C-t>` to give the keys back (v0.20.2) → The cursor goes **hollow**, and the rule along the top of the pane stays the plain border colour — it no longer turns green with focus 〔未訳〕
- [ ] **1.4** Run something colorful (`git status` in the `repo` fixture) → The 16 ANSI colors, and they match the file list's own colors rather than looking like a second palette 〔未訳〕
- [ ] **1.5** **`<C-t>` again** → Keys go back to the list — **and the shell is still there**, with its output intact. This is the v0.6.0 fix; before it, this ended the shell 〔未訳〕
- [ ] **1.6** `<C-t>`, `<C-t>`, `<C-t>` a few times → The same shell throughout. The scrollback never resets 〔未訳〕
- [ ] **1.7** `<C-S-t>` → *Now* the pane closes and the shell ends 〔未訳〕
- [ ] **1.8** Reopen, then resize the window → The grid reflows; no clipped half-columns, no stretched text 〔未訳〕
- [ ] **1.9** `dir` in `many\` to fill the screen, then `<S-PageUp>` → **The text moves.** Until v0.20.3 only the note moved — it said "N lines back" over a screen that had not scrolled 〔未訳〕
- [ ] **1.9a** `<S-PageUp>` / `<S-PageDown>` (v0.20.4) → Half a screen back / forward. Until v0.20.4 the sign was inverted, so `<S-PageUp>` aimed at the bottom and did nothing 〔未訳〕
- [ ] **1.9b** `<S-Home>`, `<S-End>` → The oldest line held, and the prompt. These worked before — no sign to get wrong 〔未訳〕
- [ ] **1.9d** The mouse wheel over the pane (v0.20.4) → Moves smoothly, a notch at a time. It used to need spinning hard for one or two lines 〔未訳〕
- [ ] **1.9e** Scroll back far enough that the prompt leaves the screen → The cursor goes with it — no block left behind at its old height 〔未訳〕
- [ ] **1.9c** `<C-S-f>` for a word far up the scrollback, Enter → The view jumps to the match **and the match is highlighted** 〔未訳〕
- [ ] **1.9f** `<C-S-f>` for a word that is on screen right now (v0.20.4) → The one on screen is found first, not an older one up in the history 〔未訳〕
- [ ] **1.9g** `<C-S-n>` / `<C-S-b>` after that → `<C-S-n>` walks further up into the history, `<C-S-b>` comes back down 〔未訳〕
- [ ] **1.9h** `<C-S-f>` for something that is not there → A red toast saying so — not silence 〔未訳〕
- [ ] **1.10** `<S-End>`, then type a character → Back at the bottom, and typing alone would have done it 〔未訳〕
- [ ] **1.11** Drag across some output (v0.20.4) → **It highlights as you drag**, and is on the clipboard when you let go. Before v0.20.4 the copy worked and nothing was drawn 〔未訳〕
- [ ] **1.11a** Drag **right to left** over the same run of text (v0.26.4) → The same text, character for character. Until v0.26.4 a backwards drag lost one at **each** end 〔未訳〕
- [ ] **1.11b** Start the drag **on** the first character, not to its left (v0.26.4) → It is included. It used to be dropped unless the drag began in the gap before it 〔未訳〕
- [ ] **1.11c** Drag from the right half of a character → That character is left out — correct, and the same rule that makes 1.11a work 〔未訳〕
- [ ] **1.12** Double-click a word → The word is selected, and visibly so 〔未訳〕
- [ ] **1.13** `<C-S-f>`, type a word from the scrollback, Enter, then `<C-S-n>` → Matches are found and stepped through; it wraps at the end 〔未訳〕
- [ ] **1.14** `<F1>` inside the terminal → The key list opens **over** the terminal. `<Esc>` closes it and typing goes back to the shell 〔未訳〕
- [ ] **1.15** `<C-S-p>` inside the terminal → The command palette opens, and running something from it works 〔未訳〕
- [ ] **1.16** `cd` somewhere in the shell, then `<A-Up>` → The file list follows to where the shell is 〔未訳〕
- [ ] **1.17** Select two files, `<A-t>` → Their paths are typed onto the shell's line, quoted, **not run** 〔未訳〕
- [ ] **1.18** With a shell that reports OSC 7 (PowerShell 7, or bash with a `PROMPT_COMMAND`), change directory in the list → No stray `cd` is typed into the shell 〔未訳〕

## 2. ミニマップ — 0 / 10

`long.rs`（4000 行）を開く。帯の形が元のファイルに見えるかどうかが本題。

自動テスト済みなので下には出していない: 2.1, 2.8

- [ ] **2.2** Look at the shape → Comment headers read as long bars, indented blocks as bars starting further right, the blank line every 40 as a gap. It should look like the file 〔未訳〕
- [ ] **2.3** Look at the colors → The bars carry syntax colors — strings and comments differ from code — not one flat color 〔未訳〕
- [ ] **2.4** Find the viewport box → A lighter box with a border, covering the part of the file on screen 〔未訳〕
- [ ] **2.5** `<A-j>` a few times → The box moves down in step with the text 〔未訳〕
- [ ] **2.6** Click halfway down the strip → The preview jumps there, with the clicked line in the **middle** of the pane, not at its top 〔未訳〕
- [ ] **2.7** Drag up and down the strip → The preview follows continuously 〔未訳〕
- [ ] **2.9** `<A-n>` → The map toggles off and on 〔未訳〕
- [ ] **2.10** Open `notes.md` (rendered) → **No map** — this is deliberate, the rendered lines are not the file's lines 〔未訳〕
- [ ] **2.11** Press `M` for source → The map appears 〔未訳〕
- [ ] **2.12** A short file (`same-a.txt`) → No map: two lines are not worth mapping 〔未訳〕

## 3. 画像の拡大と移動 — 0 / 10

`zoom-me.png` は 3200×2400 で 8px のグリッド入り。ぼけたらすぐ分かる。

- [ ] **3.1** Hover `zoom-me.png` → It fits the pane. The caption reads `3200 × 2400 · fit NN%` 〔未訳〕
- [ ] **3.2** `<A-1>` (1:1) → It fills far more than the pane, showing the middle. **The grid lines are crisp** — this is the re-decode working; if it is a blurred enlargement of the fitted copy, that is the bug this was built to avoid 〔未訳〕
- [ ] **3.3** Watch the moment it sharpens → The picture must **not jump or change size** when the sharper copy arrives. Only its sharpness changes 〔未訳〕
- [ ] **3.4** Drag it → It pans, and stops when its edge reaches the pane's edge — it cannot be thrown off screen 〔未訳〕
- [ ] **3.5** `Ctrl` and the wheel, pointer on a grid intersection → It zooms **about the pointer**: the intersection under the cursor stays under it 〔未訳〕
- [ ] **3.6** Plain wheel (no Ctrl) → Scrolls the pane, does not zoom 〔未訳〕
- [ ] **3.7** Double-click → Back to fitting, centred 〔未訳〕
- [ ] **3.8** `<A-i>` / `<A-o>` → In and out in steps. The caption's percentage follows 〔未訳〕
- [ ] **3.9** Zoom in, then `j` to the next file and back → It is fitted again — a zoom belongs to the file it was set on 〔未訳〕
- [ ] **3.10** Hover `tiny.png` (48×48) → Shown at its own size, **not blown up** to fill the pane 〔未訳〕

## 4. SVG と、その中の文字 — 0 / 8

- [ ] **4.1** Hover an SVG with no text in it (an icon, a logo) → Drawn, scaled to fill the pane, sharp at any pane size 〔未訳〕
- [ ] **4.2** An SVG containing **text** → The text is drawn, in the right place, at the right size — **not missing, not boxes, not overlapping** 〔未訳〕
- [ ] **4.3** An SVG with **Japanese** text → Same. A font with kana and kanji is picked, rather than the text vanishing 〔未訳〕
- [ ] **4.4** An SVG naming a font that is **not installed** → A fallback is used and something readable appears; it does not fail the whole render 〔未訳〕
- [ ] **4.5** An SVG with **bold** or *italic* text → The weight and slant are there, not flattened to regular 〔未訳〕
- [ ] **4.6** An SVG using a font **file next to it** rather than a system font → Loaded from the directory, as `resources_dir` intends 〔未訳〕
- [ ] **4.7** A **malformed** SVG (truncate one) → `bad SVG: …` on the preview, and the window keeps working 〔未訳〕
- [ ] **4.8** Compare 4.2 and 4.3 against v0.33.5's build → Any difference in the glyphs is the new shaper; say what changed and attach both 〔未訳〕

## 5. 2 ファイルの差分表示 — 0 / 1

自動テスト済みなので下には出していない: 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 5.6a, 5.6b, 5.6c, 5.7, 5.8, 5.10

- [ ] **5.9** Two directories → Refused with a reason 〔未訳〕

## 6. 2 分割ペインと、ペイン間の受け渡し — 0 / 1

自動テスト済みなので下には出していない: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7, 6.8, 6.9, 6.10, 6.11, 6.12, 6.13, 6.14

- [ ] **6.15** `<A-c>` a large directory, then watch the status bar → It is a job like any other copy: progress, speed, and cancellable from `w` 〔未訳〕

## 7. ヘルプパネルの設定ファイルパス — 0 / 8

- [ ] **7.1** `~` with no `filer.toml` anywhere → **Both** directories are listed, the empty one marked `nothing here`. Before v0.25.0 only files that existed were shown 〔未訳〕
- [ ] **7.2** Hover a path → The row lights up and the pointer becomes a hand 〔未訳〕
- [ ] **7.3** Hover a key row → Nothing happens — it is not a link 〔未訳〕
- [ ] **7.4** Click a config **file** → The panel closes, the list opens its directory with that file under the cursor. `<Enter>` then opens it 〔未訳〕
- [ ] **7.5** Click a **directory** → The panel closes and the list goes there, empty or not 〔未訳〕
- [ ] **7.6** Click the empty one, then create `filer.toml` there and `<C-F5>` → It appears in the panel next time, without `nothing here` 〔未訳〕
- [ ] **7.7** With `YAZI_CONFIG_HOME` / `FILER_CONFIG_HOME` set → The listed directories follow them 〔未訳〕
- [ ] **7.8** A config warning line → Still yellow, and not clickable 〔未訳〕

## 8. ターミナルペインが起動するシェル — 0 / 7

- [ ] **8.1** `<C-t>` with no `[term]` in `filer.toml`, then `$PSVersionTable.PSVersion` → `5.1.x` — Windows PowerShell, unchanged from every earlier version 〔未訳〕
- [ ] **8.2** Add `[term]` / `shell = "pwsh"`, `<C-S-t>`, `<C-t>`, ask again → `7.x` 〔未訳〕
- [ ] **8.3** `$PROFILE` in each → Two different paths — `WindowsPowerShell\` for 5.1, `PowerShell\` for 7 〔未訳〕
- [ ] **8.4** With the OSC 7 hook in the pwsh profile only, `cd` and `<A-Up>` under each → Works under `pwsh`, and says so under 5.1. That asymmetry is the whole bug report 〔未訳〕
- [ ] **8.5** `args = ["-NoLogo"]` → The banner is gone 〔未訳〕
- [ ] **8.6** A `shell` that is not installed → It fails to start and says so — no silent empty pane 〔未訳〕
- [ ] **8.7** Remove `[term]` again, `<C-S-t>`, `<C-t>` → Back to the default 〔未訳〕

## 9. ファイル末尾のアウトライン — 全 5 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 10. ヤンクレジスタ（コピー・カット）の表示 — 0 / 1

残っているのは色そのものの見え方。緑 / 黄 / 赤に読めるか、3px の帯が気づける太さかどうか。

自動テスト済みなので下には出していない: 10.1, 10.2, 10.3, 10.4, 10.5, 10.6, 10.7, 10.8

- [ ] **10.9** Cut a file, then `p` into a directory that already holds that name, and answer **no** to the overwrite → The count still leaves the header — `paste()` empties a cut register when it *submits* the job, not when the job succeeds, so the files are neither moved nor still in the register 〔未訳〕

## 11. 一括リネーム — 全 12 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 12. undo と redo — 0 / 8

自動テスト済みなので下には出していない: 12.6, 12.7, 12.9, 12.12

- [ ] **12.1** `d` on a file in `many\` → It goes to the recycle bin 〔未訳〕
- [ ] **12.2** `u` → It comes back, in its original place. A toast says so 〔未訳〕
- [ ] **12.3** Check the task panel (`w`) during F2 → A `Restore` row appears and completes 〔未訳〕
- [ ] **12.4** `U` → Deleted again 〔未訳〕
- [ ] **12.5** Delete two files with the same name from different folders, an interval apart, then `u` → The one just deleted comes back — not the older one 〔未訳〕
- [ ] **12.8** Rename a file, undo it, then create a new file, then `U` → Redo is gone: the new action forked history 〔未訳〕
- [ ] **12.10** Open a file in another program so it is locked, select it **with several others**, `d` (v0.27.1) → The others go. The message **names the one that did not**, and the task panel's count matches what actually went. Until v0.27.1 it said `Trash: trash: Error … Some operations were aborted` naming nothing, and counted them all as done 〔未訳〕
- [ ] **12.11** `d` on a drive whose Recycle Bin is turned off → Same shape of message, naming the file 〔未訳〕

## 13. シンボリックリンクと `g`+`f` — 0 / 8

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

- [ ] **13.7** ジャンクション（`mklink /J`）を、ただのシンボリックリンクではなく — *A junction (`mklink /J`), not just a symlink → Treated the same: `->`, and `g`+`f` follows it*
- [ ] **13.8** `y` してから、別のディレクトリで `-` — *`y`, then `-` in another directory → The symlink appears. **On Windows this needs Developer Mode on** (Settings > System > For developers) — without it, and without running filer elevated, it fails with `os error 1314` and the toast says which two remedies there are. The privilege is the OS's, not the app's: `std` already passes `SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE`, which is what makes Developer Mode enough*
- [ ] **13.9** `y` してから、**隣の**ディレクトリで `_` — *`y`, then `_` in a **sibling** directory → The same link, written relative (`..\other\file`). `g`+`f` follows it, and it survives moving both directories together — which is the point of `_` over `-`*
- [ ] **13.10** シンボリックリンクの上で `<Tab>` — *`<Tab>` on a symlink (v0.46.0) → A **Link** section: `Kind` reads `Symlink`, `Target` the stored path, `Resolves` where it lands*
- [ ] **13.11** `_` で作ったリンクの上で `<Tab>` — *`<Tab>` on a link made with `_` → `Kind` reads `Symlink (relative)`, and `Target` is the relative path while `Resolves` is absolute — the two rows differ, which is the whole point of the pair*
- [ ] **13.12** **壊れた**リンクの上で `<Tab>` — *`<Tab>` on a **broken** link → `Resolves` reads `no (…)` with the OS's reason, and the section still appears*
- [ ] **13.14** 同じものを Windows で — *The same, on Windows → `Also at` lists the other path. Check it against `fsutil hardlink list` — the same set, with the file's own path left out*
- [ ] **13.16** ハードリンクを作り、別のプログラムに共有なしの書き込みロックを握らせた状態で `<Tab>`（コマンドは上の「準備」） — *Hardlink a file, then have another program hold it open for writing with no sharing, and `<Tab>` it (commands in the preamble above) → `Links` still reads `2` and `Also at` still lists the other name. The handle asks for **no** access rights, so an exclusive write lock does not hide the count*

## 14. 親ディレクトリの列を、マウスで — 0 / 6

- [ ] **14.1** Click a **directory** there → The list goes into it, as it always has 〔未訳〕
- [ ] **14.2** Click a **file** there (v0.26.7) → The list goes up to where that file lives, **with the file under the cursor**. Until v0.26.7 nothing happened at all 〔未訳〕
- [ ] **14.3** Then press `<Enter>` → It opens — the cursor really is on it, not merely near it 〔未訳〕
- [ ] **14.4** Double-click either → The same as a single click; no second, different meaning 〔未訳〕
- [ ] **14.5** Click the row for the directory you are already in → You stay there, and the cursor does not jump about 〔未訳〕
- [ ] **14.6** At a drive root, where there is no parent column → Nothing to click, and nothing misbehaves 〔未訳〕

## 15. ウィンドウの拡大縮小と、取り返したキー — 0 / 7

JIS 配列では `=` は shift+`-` にある。`<C-=>`／`<C-+>` はどちらの配列でも同じ文字が届く。

- [ ] **15.1** `<C-->` with something yanked → **Only** the window shrinks. Until v0.32.0 it also made a hardlink — one press, two actions 〔未訳〕
- [ ] **15.2** `<C-+>`, and `<C-=>` → Both make it bigger. Which of the two needs shift depends on the layout — on US `+` is shift+equals, on JIS `+` is shift+semicolon and `=` is shift+minus — and both spellings are bound so either reaches it (v0.45.6) 〔未訳〕
- [ ] **15.3** `<C-0>` → Back to 100%, and a toast says so 〔未訳〕
- [ ] **15.4** Hold `<C-->` down → It shrinks smoothly and stops at 20%; `<C-+>` held stops at 500% 〔未訳〕
- [ ] **15.5** `=` with something yanked, in a directory **on the same drive** → The hardlink, in its new place. Nothing in the app says so — a hardlink is another entry pointing at the same data, so it has no marker and the spot panel's Link section is for symlinks only. Confirm with `fsutil hardlink list <the new path>`, which lists every path sharing the data; or write to one and read the other. Across drives it must fail: NTFS hardlinks cannot leave their volume. Was `<C-S-->` until v0.45.6, a chord no keyboard can produce 〔未訳〕
- [ ] **15.6** `<A-i>` / `<A-o>` on an image → Still the **image** zoom, unaffected — `zoom` and `scale` are different commands 〔未訳〕
- [ ] **15.7** `~` → `scale in` / `scale out` / `scale reset` are listed, like any other command 〔未訳〕

## 16. Word / Excel / PowerPoint — 0 / 12

- [ ] **16.1** Hover a `.docx` → Its text, paragraph by paragraph. Not a hex dump, not a metadata card 〔未訳〕
- [ ] **16.2** A paragraph with mixed bold and plain in one sentence → **One line**, not one per run 〔未訳〕
- [ ] **16.3** A document with Heading 1/2 styles, then `<S-Tab>` → The headings are the outline, and `<Enter>` on one jumps to it 〔未訳〕
- [ ] **16.4** Hover a `.xlsx` → Rows as tab-separated cells, each sheet announced 〔未訳〕
- [ ] **16.5** A workbook whose **first tab is not `sheet1.xml`** → The tabs come out in the workbook's order, with their real names 〔未訳〕
- [ ] **16.6** A sheet holding dates → `2023-03-15`, **not** `45000` 〔未訳〕
- [ ] **16.7** A sheet holding a date **and** a time → The time follows the date 〔未訳〕
- [ ] **16.8** Hover a `.pptx` with ten or more slides → In order — slide 10 after slide 9, not after slide 1 〔未訳〕
- [ ] **16.9** Japanese text in any of the three → Correct, and `&amp;` `&lt;` come through as `&` `<` 〔未訳〕
- [ ] **16.10** Rename an old `.doc` to `.docx` and hover it → A card saying it is not an Office XML file, naming the likely cause 〔未訳〕
- [ ] **16.11** A very large workbook → Stops at 5000 lines and says it is truncated; it does not hang 〔未訳〕
- [ ] **16.12** `/` and `n` inside one → Search works, because it is an ordinary text preview 〔未訳〕

## 17. 自前のプレビューア — 0 / 18

- [ ] **17.1** Hover a multi-page PDF → Page one, with `page 1` under it 〔未訳〕
- [ ] **17.2** `<A-j>` → Page two. `page 2` under it 〔未訳〕
- [ ] **17.3** `<A-k>` → Back to page one 〔未訳〕
- [ ] **17.4** `<A-k>` again, on page one → Stays. It does not go to page zero or below 〔未訳〕
- [ ] **17.5** Hold `<A-j>` past the last page (v0.30.1) → **The last page stays on screen**, and a line says `No more: …` with the command's own words. Until v0.30.1 the page was replaced by the error 〔未訳〕
- [ ] **17.5a** `<A-k>` straight after that → Back a page from the last one, not from somewhere past it 〔未訳〕
- [ ] **17.5b** A **short** video — a few seconds — and `<A-j>` a few times → Same: it stops at the last frame it could draw. This is where it bites, since `step = 10` runs off the end almost at once 〔未訳〕
- [ ] **17.5c** The caption on a video (v0.30.1) → `50s`, not `s 50` 〔未訳〕
- [ ] **17.6** Watch the screen while paging → **No console window flashes.** It runs once per press 〔未訳〕
- [ ] **17.7** Page to 5, move to another file, come back → Back at page one: the page belongs to the file 〔未訳〕
- [ ] **17.8** Page back to one you have already seen → Instant — it is cached per page 〔未訳〕
- [ ] **17.9** Hover a video (v0.30.2) → **A frame appears.** Until v0.30.2 none ever did on Windows: `{out}.png` was quoted as `"…page".png`, which `cmd` hands to ffmpeg with the quotes in the filename 〔未訳〕
- [ ] **17.9a** The same on a path with a space → Still draws — the quoting wraps the whole word, suffix included 〔未訳〕
- [ ] **17.10** `<A-j>` on it → Ten seconds in, by `step` 〔未訳〕
- [ ] **17.11** A PDF with a **space** in its name, and one in a Japanese folder → Both draw. The quoting is filer's, not the rule's 〔未訳〕
- [ ] **17.12** Rename `pdftoppm` away, then hover a PDF → An error naming the tool, not a hang 〔未訳〕
- [ ] **17.13** Remove the `[[preview]]` rules, `<C-F5>`, hover a PDF → Back to the shell thumbnail, unchanged 〔未訳〕
- [ ] **17.14** `filer env` with the rules in place → `pdftoppm` and `ffmpeg` listed under Tools, with `preview *.pdf` beside them 〔未訳〕

## 18. クイックルックと、ミニマップの隣、その他のペイン — 0 / 5

自動テスト済みなので下には出していない: 18.1, 18.2, 18.3, 18.4, 18.8, 18.9, 18.10

- [ ] **18.5** `<C-w>` → The view splits into two panes; the one with the keys is framed, the other's cursor is dimmed 〔未訳〕
- [ ] **18.6** Select files, `<A-c>` → Copied into the other pane 〔未訳〕
- [ ] **18.7** Drag files onto the other pane → A frame marks the target, and a label by the pointer says "copy" — `Shift` makes it "move" — **before** you let go 〔未訳〕
- [ ] **18.11** `'` then a letter, having saved one with `B` → Jumps there. **`b` is the prefix bookmark *management* hangs off** (`bb` lists, `bs` saves, `bd` deletes), so `b` and a letter reaches nothing 〔未訳〕
- [ ] **18.12** `z` → The jump list: bookmarks first, then recent directories with "2h ago" beside them 〔未訳〕

## 19. ホイール、ペインごとの挙動 — 0 / 7

- [ ] **19.1** Wheel over the **preview** of a long text file → It scrolls, one notch at a time, without spinning hard. This is the v0.26.5 fix 〔未訳〕
- [ ] **19.2** Turn the wheel as slowly as you can over the preview → It still moves. Every fraction counts; nothing is discarded 〔未訳〕
- [ ] **19.3** Wheel over the **file list** → The same, and with the split open, over each pane in turn 〔未訳〕
- [ ] **19.4** Wheel over the **terminal** pane → Still right — fixed earlier, in v0.20.4, and now sharing the same code 〔未訳〕
- [ ] **19.5** Turn one way then straight back → It reverses at once, with no dead travel from a stranded remainder 〔未訳〕
- [ ] **19.6** `Ctrl` and the wheel over an image → Zooms, and does **not** scroll the pane with the same turn 〔未訳〕
- [ ] **19.7** Move the pointer between panes mid-turn → Neither jumps: each keeps its own remainder 〔未訳〕

## 20. 設定とテーマ — 0 / 6

- [ ] **20.1** With filer open, edit `theme.toml` (change `[mgr] cwd` to something loud) and press `<C-F5>` → The color changes without restarting 〔未訳〕
- [ ] **20.2** Change `[ui] font_size` in `filer.toml`, `<C-F5>` → The text resizes 〔未訳〕
- [ ] **20.3** Add a `keymap.toml` binding, `<C-F5>` → The new key works, and `<F1>` lists it 〔未訳〕
- [ ] **20.4** Sort with `,s`, then `<C-F5>` → The sort **stays** as you set it — a reload does not undo what you changed by hand 〔未訳〕
- [ ] **20.5** Put a syntax error in `filer.toml`, `<C-F5>` → An error toast naming the problem; the old config stays in force 〔未訳〕
- [ ] **20.6** `[ui] minimap = false`, `<C-F5>` → No minimap 〔未訳〕

## 21. 書庫（zip / tar / 7z） — 0 / 9

自動テスト済みなので下には出していない: 21.1, 21.6, 21.12

- [ ] **21.2** `e` on it → Unpacked into a `sample` folder beside it; progress in the task panel 〔未訳〕
- [ ] **21.3** `e` again → The second one gets a different name; the first is not overwritten 〔未訳〕
- [ ] **21.4** Select `to-pack\`, press `E`, accept `to-pack.zip` → Packed, and the result opens 〔未訳〕
- [ ] **21.5** `E` and change the name to end in `.tar.gz` → A gzipped tar, not a zip 〔未訳〕
- [ ] **21.7** `E` and change the name to end in **`.7z`** (v0.27.0) → A real 7z. Until v0.27.0 this was refused as read-only 〔未訳〕
- [ ] **21.8** `e` on that `.7z` → It unpacks, and the files match what went in 〔未訳〕
- [ ] **21.9** Open the same `.7z` in 7-Zip or Explorer → It opens there too — the point of the format is that it travels 〔未訳〕
- [ ] **21.10** Pack a folder holding subfolders as `.7z`, watch the task panel → The count is of **files**, not folders, and it reaches the total rather than stopping short 〔未訳〕
- [ ] **21.11** Compare the `.7z` and the `.zip` of the same input → The 7z is smaller; that is the reason to have it 〔未訳〕

## 22. エディタを行番号付きで開く（エディタのインストールが要る） — 0 / 6

- [ ] **22.1** 秀丸エディタ → Opens at the outline entry's line 〔未訳〕
- [ ] **22.2** サクラエディタ → Same 〔未訳〕
- [ ] **22.3** EmEditor → Same 〔未訳〕
- [ ] **22.4** Notepad++ → Same 〔未訳〕
- [ ] **22.5** メモ帳 → Opens, at the top — it has no line argument, and that is correct 〔未訳〕
- [ ] **22.6** VS Code / nvim, if you have them → At the line 〔未訳〕

## 23. ネットワークパス（共有が要る） — 0 / 4

- [ ] **23.1** `g<Space>`, type `\\server\share` → It opens 〔未訳〕
- [ ] **23.2** Copy a file to and from it → Works, with progress 〔未訳〕
- [ ] **23.3** Unplug the network mid-listing, or point at a dead host → **The window keeps responding.** An error toast, and the tab goes back where it was 〔未訳〕
- [ ] **23.4** Tab-complete a path on the share → The prompt stays responsive; a `…` shows while it waits 〔未訳〕

## 24. 扱いにくい名前 — 0 / 3

自動テスト済みなので下には出していない: 24.1, 24.3

- [ ] **24.2** The very long name → Elided in the middle, with the extension still readable 〔未訳〕
- [ ] **24.4** Copy the name with a quote in it, `<A-t>` into the terminal → Quoted so the shell sees one word 〔未訳〕
- [ ] **24.5** `d` then `u` on the CJK-named file → Comes back under the same name 〔未訳〕

## 25. `filer env` — 0 / 19

- [ ] **25.1** `filer env` from PowerShell → The four sections print. A release build is a GUI binary, so this is the same `CONOUT$` path `--version` uses — **text actually appears** 〔未訳〕
- [ ] **25.2** The Config section → Both directories, each saying what is in it or `nothing here`, and `not here:` listing the rest 〔未訳〕
- [ ] **25.3** With a deliberate typo in `keymap.toml` → The warning appears under `Warnings`, its several lines indented under the one key 〔未訳〕
- [ ] **25.4** The Tools section → `pdftoppm`, `ffmpeg`, `ffprobe`, `pwsh`, `git` with versions where installed and `not found` where not, each naming what it is for 〔未訳〕
- [ ] **25.4a** With `[term] shell = "pwsh"` set (v0.29.1) → `pwsh` is the shell listed. Without it, `powershell` — the one that will actually launch, not a guess 〔未訳〕
- [ ] **25.4b** With openers configured → Each named program is listed with the opener kind it belongs to, found or not 〔未訳〕
- [ ] **25.4c** An opener naming a **quoted full path** (秀丸, サクラ) → The whole path is resolved, not just up to the first space 〔未訳〕
- [ ] **25.4d** Watch the screen while `filer env` runs → **No editor or viewer opens.** The programs are looked up on `PATH`, never executed 〔未訳〕
- [ ] **25.5** On Windows on ARM with the x64 build → `OS arch` and `Process arch` **disagree** — that disagreement is the whole reason both are printed 〔未訳〕
- [ ] **25.6** `filer --help` → `env` is listed under COMMANDS 〔未訳〕
- [ ] **25.7** Double-click `filer.exe` (no console) → Unchanged: the window opens, nothing is printed anywhere 〔未訳〕
- [ ] **25.8** Open filer once, quit, then `filer env` (v0.29.0) → A **Last run** section: the adapter with its backend and device type, and every font file that was loaded 〔未訳〕
- [ ] **25.9** On a fresh machine, `filer env` **before** ever opening filer → `not recorded — filer has not opened a window on this machine yet`, not an empty section 〔未訳〕
- [ ] **25.10** Name a different font in `filer.toml`, `<C-F5>`, then `filer env` again → The new file is listed; the reload updates the record 〔未訳〕
- [ ] **25.11** With no bold face anywhere → `none found; bold is faked by overstriking` — the bold list is separate from the regular one on purpose 〔未訳〕
- [ ] **25.12** An opener starting with `start` (the default-app one) → **`built into cmd`**, not `not found`. It is one of `cmd`'s own commands and is never a file on the `PATH`, so the lookup every other row uses cannot see it (v0.33.12) 〔未訳〕
- [ ] **25.13** `<Enter>` on a file whose rule uses that opener → It really does open — the row and the behaviour agree 〔未訳〕
- [ ] **25.14** An opener naming a program that genuinely is not installed → Still **`not found`**. The exemption is for the shell's own names only 〔未訳〕
- [ ] **25.15** Break `yazi.toml` and read the Warnings row → The path is written **`…\filer\yazi.toml`**, all backslashes. It used to come out `…\filer/yazi.toml`, in the one message whose job is to name the file to edit (v0.33.12) 〔未訳〕

## 26. アプリの中からのバグ報告 — 0 / 10

- [ ] **26.1** `<F12>` → The default browser opens GitHub's new-issue form, and a toast says so 〔未訳〕
- [ ] **26.2** Look at the form → **Version** and **OS とアーキテクチャ** are already filled in; the rest is empty 〔未訳〕
- [ ] **26.3** Compare the filled version against `filer --version` in a terminal → The same string, architecture included 〔未訳〕
- [ ] **26.4** Compare the filled OS line against `winver` → Edition, feature update and build all match, UBR included (`Windows 11 Pro 25H2 (build 26200.9457)`) 〔未訳〕
- [ ] **26.4b** Compare it against the form's own PowerShell snippet → The same facts. Nothing left worth pasting over the top 〔未訳〕
- [ ] **26.5** On the ARM64 machine, with the **ARM64** build → OS arch and Process arch both read `aarch64` 〔未訳〕
- [ ] **26.6** On the ARM64 machine, with the **x64** build (under emulation) → OS arch `aarch64`, Process arch `x86_64` — **the two disagree, and that is the finding** 〔未訳〕
- [ ] **26.7** Submit the report → It posts, and the pre-filled fields survive 〔未訳〕
- [ ] **26.8** `<F12>` with no browser set as default (or a broken association) → An error toast naming the failure. **The window keeps working** 〔未訳〕
- [ ] **26.9** `<F12>` from the terminal pane (`<C-t>` first) → Nothing: `[term]` passes it to the shell, which is correct 〔未訳〕

## 27. 届かなかったプレビュー — 全 5 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 28. 外から加えられた変更 — 0 / 7

- [ ] **28.1** Put the cursor on the **last** row, delete that file from Explorer → The row goes, the cursor lands on the new last row, **no crash** 〔未訳〕
- [ ] **28.2** Cursor on the last row; delete several files at the end at once → Same 〔未訳〕
- [ ] **28.3** Delete every file in the folder from outside → An empty listing, still responsive 〔未訳〕
- [ ] **28.4** Cursor on the last row of a **filtered** listing (`f`), delete the file it is on → Same, and the filter still holds 〔未訳〕
- [ ] **28.5** Same in the **other pane** (`<C-w>`) and in the **preview** of a directory → Neither crashes 〔未訳〕
- [ ] **28.6** Cursor on the last row, delete that file with `d` → Same — this is what Issue #5 reported 〔未訳〕
- [ ] **28.7** Rename a file from outside while the cursor is on it → The cursor follows the name or stays put; no crash 〔未訳〕

## 29. ターミナルのカレントディレクトリを持ち帰る — 0 / 5

- [ ] **29.1** With **no** hook in `$PROFILE`, open the terminal (`<C-t>`), `cd` somewhere, press `<A-Up>` → A toast naming OSC 7 and `LocationChangedAction`, pointing at the README — **not** silence, and not a wait 〔未訳〕
- [ ] **29.2** Paste the README hook into `$PROFILE`, open a new terminal, `cd C:\dev`, press `<A-Up>` → The file list moves to `C:\dev` 〔未訳〕
- [ ] **29.3** Same with a directory whose name has a **space** and one with **Japanese** in it → Both arrive intact 〔未訳〕
- [ ] **29.4** `cd` to a UNC path (`\\server\share`) and press `<A-Up>` → Either it follows or it says why; no crash 〔未訳〕
- [ ] **29.5** Run the hook line by hand in a shell that already has Starship → The prompt still draws normally (the hook uses `LocationChangedAction`, not `prompt`) 〔未訳〕

## 30. プロンプトでの右クリック貼り付け — 0 / 14

- [ ] **30.1** Copy a path in Explorer's address bar, press `c`+`d` (or whatever opens the `cd` prompt), right-click the field → The path appears; `<Enter>` goes there 〔未訳〕
- [ ] **30.2** Type `abc`, click between `a` and `b` with the **right** button → The paste lands there, not at the end 〔未訳〕
- [ ] **30.3** Select part of the text with a drag, then right-click **on the selection** → The selection is replaced 〔未訳〕
- [ ] **30.4** Copy two lines of text, right-click into `s` → One line, the break shown as a space — the same as `<C-v>` 〔未訳〕
- [ ] **30.5** Copy a Japanese path, right-click into `cd` → Intact, and the caret sits after it 〔未訳〕
- [ ] **30.6** With an image (not text) on the clipboard, right-click a prompt → Nothing happens, **no toast** 〔未訳〕
- [ ] **30.7** Same in the command palette, in `f`, and in `S-r` (bulk rename) → Each pastes; the bulk preview re-renders 〔未訳〕
- [ ] **30.8** Right-click in the **file list** → Still the context menu — the list is unchanged 〔未訳〕
- [ ] **30.9** Right-click in the **terminal** pane (`<C-t>`) → The clipboard is typed in, and the pane takes the keys if it did not have them 〔未訳〕
- [ ] **30.10** Select text in the terminal with a drag, then right-click → The selection was copied on release; the right-click pastes it back — select to copy, right-click to paste 〔未訳〕
- [ ] **30.11** Copy **three lines** and right-click into the terminal at a PowerShell prompt → All three sit in the buffer, **nothing runs** until `<Enter>` (PSReadLine asks for bracketed paste) 〔未訳〕
- [ ] **30.12** The same in a shell that does **not** ask for bracketed paste (`cmd.exe`) → The lines run, as they always have — and no stray `[200~` appears 〔未訳〕
- [ ] **30.13** Right-click in the terminal while `vim` is open → The text is inserted; no `[200~` on screen 〔未訳〕
- [ ] **30.14** `<C-v>` in the terminal → Same as the right-click, including 23.11 〔未訳〕

## 31. ホストの共有一覧 — 0 / 13

- [ ] **31.1** `g`+`<Space>`, type `\\<your server's address>`, `<Enter>` → The shares are listed, the same ones Explorer shows 〔未訳〕
- [ ] **31.2** Same with a host **name** rather than an address, and with the `//` spelling → Both arrive; the path is shown back in the `\\host` spelling 〔未訳〕
- [ ] **31.3** Walk into a share and back out with `h` → Into the share, then back to the host list 〔未訳〕
- [ ] **31.4** `h` again, at the host → Nothing moves (the host is the top), no crash 〔未訳〕
- [ ] **31.5** A host that is off, or does not exist (an unused address on your own subnet) → The tab returns to where it was and a toast says why — it does not hang the window 〔未訳〕
- [ ] **31.5a** 24.1 and 24.5 again, watching for a **toast** → v0.16.0 fell back to the parent in silence, so a failure looked like nothing happening. Whatever the outcome, there is now either a listing or a message; if it is still a message, its os error number is the thing to report 〔未訳〕
- [ ] **31.6** A host that needs a login the machine has not been given → Same: a refusal as a toast, naming it 〔未訳〕
- [ ] **31.7** A host with **many** shares (more than a screenful) → All of them, scrolling normally 〔未訳〕
- [ ] **31.8** A share name with a space or non-ASCII in it → Intact 〔未訳〕
- [ ] **31.9** Hover a share and look at the size column → Empty — there is nothing to read, and it must not sit there counting 〔未訳〕
- [ ] **31.10** `<C-r>` / refresh on the host listing → Re-asks the server; no crash 〔未訳〕
- [ ] **31.11** Open the host in the **other pane** (`<C-w>`) and in a second tab → Both fine 〔未訳〕
- [ ] **31.12** Go to a host, then change directory away → The watcher does not complain about the host it could not watch 〔未訳〕

## 32. オープナー — 0 / 12

- [ ] **32.1** Paste the README's `[opener]` / `[open]` example into `yazi.toml`, restart, `<S-Enter>` on a `.txt` → 秀丸 / サクラ / VS Code / Neovim / default — with the descriptions, not the command lines 〔未訳〕
- [ ] **32.2** `<Enter>` on the same file → Opens in the first entry (秀丸), no console flash 〔未訳〕
- [ ] **32.3** `<S-Enter>` on a `.pdf` → Edge and Chrome first, then the default-app entry 〔未訳〕
- [ ] **32.4** `<S-Enter>` on a `.xlsx`, pick Excel → Excel opens it — this is the `start ""` case that fails without it 〔未訳〕
- [ ] **32.5** A file whose name has a **space**, through each of the above → One argument, opens correctly 〔未訳〕
- [ ] **32.6** Several files selected, then `<Enter>` → All of them go to one invocation 〔未訳〕
- [ ] **32.7** A rule written `*.{xlsx,xls,csv}` → Matches all three (this is what did not work before v0.17.0) 〔未訳〕
- [ ] **32.8** An opener naming a program that is not installed → An error toast within a few seconds, no hang 〔未訳〕
- [ ] **32.8a** An opener whose program is a **quoted full path** (秀丸, サクラ) → It opens. This is the v0.17.0 bug: `cmd` mangled the line and the failure was silent 〔未訳〕
- [ ] **32.8b** 秀丸 and サクラ from `<S-Enter>` **and** from `<Enter>` as the first entry → Both, since they take different code paths to the same launcher 〔未訳〕
- [ ] **32.8c** An opener with a deliberate typo in the path → A toast naming the failure. On a Japanese Windows expect the exit code rather than `cmd`'s own words — that is intended, not a bug to report 〔未訳〕
- [ ] **32.9** Open from the outline (`<C-o>` at a line) into 秀丸 and サクラ → Lands on the line 〔未訳〕

## 33. 設定の警告と、その色 — 0 / 11

自動テスト済みなので下には出していない: 33.1, 33.2, 33.3, 33.5, 33.7, 33.8, 33.10

- [ ] **33.4** Make something actually fail (an opener naming a program that is not installed, 25.8) → Still **red**, so the two are told apart at a glance 〔未訳〕
- [ ] **33.6** A theme with a light background → The yellow is still readable; say so if it is not — it is a fixed default, not yet themeable 〔未訳〕
- [ ] **33.9** Break **three** config files at once → Up to five boxes stack downward, each sized to its own text, none overlapping the next 〔未訳〕
- [ ] **33.11** Put `[[preview]]` into `yazi.toml` (it belongs in `filer.toml`) and start → **One line**: `…\yazi.toml: [[preview]] belongs in filer.toml, and nothing in this file was read`. Not the old `invalid type: map, expected a string` (v0.33.13) 〔未訳〕
- [ ] **33.12** Put `[term]` into `yazi.toml` as well → A second line for it, same shape. Both say the file went unread, because it did 〔未訳〕
- [ ] **33.13** Put `[term]` into a `yazi.toml` that is otherwise fine (no `[[preview]]`) → `… belongs in filer.toml and was ignored` — *ignored*, not *unread*: the rest of the file did load 〔未訳〕
- [ ] **33.14** Put `[opener]` into `filer.toml` → The same warning the other way round: `belongs in yazi.toml` 〔未訳〕
- [ ] **33.15** Move both into the right files, `<C-F5>` → No warnings. `filer env` agrees, and the terminal pane now starts what `[term] shell` names 〔未訳〕
- [ ] **33.16** With filer **already running**, create `%APPDATA%\filer\filer.toml`, then press `~` → The file is a row of its own, in the warning colour, reading `on disk, not read yet — <C-F5> re-reads config`. The directory is **not** `nothing here` (v0.34.0) 〔未訳〕
- [ ] **33.17** `<C-F5>`, then `~` again → The row is now an ordinary loaded file, no marker 〔未訳〕
- [ ] **33.18** Rebind `config_reload` to `<F9>` and repeat 33.16 → The row names `<F9>`, not `<C-F5>` — it is read from the keymap, not written into the message 〔未訳〕

## 34. ヘルプパネル自身のスクロール — 全 14 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 35. 設定ファイルの探索場所（OS ごと） — 0 / 10

- [ ] **35.1** Windows → `filer env` with both variables unset → `%APPDATA%\yazi\config` and `%APPDATA%\filer` — **unchanged from v0.34.0.** This is the row that must not have moved 〔未訳〕
- [ ] **35.2** Windows → Put `[mgr] sort_by = "mtime"` in `%APPDATA%\yazi\config\yazi.toml` → Read. yazi's own directory still shares with filer 〔未訳〕
- [ ] **35.3** macOS → `filer env` → `~/.config/yazi` and `~/.config/filer`, **not** `~/Library/Application Support/…` 〔未訳〕
- [ ] **35.4** macOS → Install yazi, run `yazi` once, put a `yazi.toml` where yazi reads it → filer reads the same file. This is the whole point of the change: before v0.35.0 filer looked under `~/Library/Application Support/yazi/config/`, which yazi never writes 〔未訳〕
- [ ] **35.5** macOS → Anyone upgrading with config in `~/Library/Application Support/filer/` → It is **no longer read** — `filer env` lists it as missing. Move it to `~/.config/filer/`. Called out as a 変更 in CHANGELOG 〔未訳〕
- [ ] **35.6** Linux → `filer env` → `~/.config/yazi` — **not** `~/.config/yazi/config` 〔未訳〕
- [ ] **35.7** Linux / macOS → `XDG_CONFIG_HOME=/tmp/x filer env` → `/tmp/x/yazi` and `/tmp/x/filer` 〔未訳〕
- [ ] **35.8** Linux / macOS → `XDG_CONFIG_HOME=relative filer env`, and again with it empty → Falls back to `~/.config/…`. XDG says a relative value is ignored 〔未訳〕
- [ ] **35.9** Any → `last-run.toml` → Still in the state directory (`data_dir()`), which this change did **not** touch. On Windows that is the same `%APPDATA%\filer`; on Linux `~/.local/share/filer` 〔未訳〕
- [ ] **35.10** Any → Symlink `filer.toml` into the config directory from elsewhere, then `<C-F5>` → Read through the link. Re-check after editing via the **link path** with an editor that saves by rename — that replaces the symlink with a regular file 〔未訳〕

## 36. `T` と、`<F3>` との違い — 0 / 5

自動テスト済みなので下には出していない: 36.1, 36.2, 36.3, 36.4, 36.5, 36.5a, 36.5b, 36.5c, 36.6, 36.7, 36.8, 36.9, 36.11, 36.12, 36.16, 36.17

- [ ] **36.10** Bind `<S-t>` instead of `T` in `prepend_keymap`, `<C-F5>` → **Nothing happens on any key** — the lesson the tests pin. No warning is printed either, because the notation is valid 〔未訳〕
- [ ] **36.13** maximized preview (`T`) → `q` → Columns back, app still running 〔未訳〕
- [ ] **36.14** `help` (`~`), task list, spotter (`Tab`), comparison (`<A-d>`) → `q` in each → Closes, app still running (unchanged — these already had their own layer) 〔未訳〕
- [ ] **36.15** Nothing up → `q` → Quits on the first press 〔未訳〕
- [ ] **36.18** Rebind: `[[mgr.keymap]]` with `on = "Q"`, `run = "quit"`, then `Q` with `<F3>` up → Closes the panel first, like `q` — the behaviour is on the action, not the letter 〔未訳〕

## 37. `start ""` 形式のオープナーが実際に起動するか — 0 / 8

- [ ] **37.1** `<Enter>` on a `.pdf` with `browser = [{ run = 'start "" msedge %*' }]` first → **Edge opens the PDF.** No command prompt appears 〔未訳〕
- [ ] **37.2** `<Enter>` on `.xlsx` / `.docx` / `.pptx` with `start "" excel %*` and friends → The Office app opens the file 〔未訳〕
- [ ] **37.3** `<Enter>` on anything routed to `open = [{ run = 'start "" %*' }]` → The file's associated app opens it 〔未訳〕
- [ ] **37.4** A file whose **name contains a space**, through any of the above → Opens as one file, not two. The path keeps its quotes 〔未訳〕
- [ ] **37.5** An opener written `start "" msedge "%*"` (placeholder quoted by hand) → Same result as 37.1 — the pair around the placeholder is still absorbed 〔未訳〕
- [ ] **37.6** Select two PDFs, `<Enter>` → Both open as separate arguments, not one quoted blob 〔未訳〕
- [ ] **37.7** Openers given as a full path (IrfanView, sakura, Hidemaru) → Unchanged — these never went through `start` 〔未訳〕
- [ ] **37.8** `O` on a PDF → The picker lists Edge, Chrome, the default app, then the editors; each entry launches what it says 〔未訳〕

## 38. フォーカスの規則、それを描く 2 つのペインで — 全 9 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 39. ターミナルペインでの `<A-j>` / `<A-k>` — 0 / 9

- [ ] **39.1** `<C-t>`, run something long (`dir /s` or `ls -R`), then `<A-k>` → The scrollback goes **up** five lines per press 〔未訳〕
- [ ] **39.2** `<A-j>` → Back **down** five lines. Same direction as in the file list, where these scroll the preview 〔未訳〕
- [ ] **39.3** Hold `<A-k>` to the top, then `<A-j>` back → Stops at each end without overshooting — no dead presses coming back 〔未訳〕
- [ ] **39.4** `<S-PageUp>` / `<S-PageDown>` / `<S-Home>` / `<S-End>`, and the wheel → Unchanged 〔未訳〕
- [ ] **39.5** With the terminal **unfocused** (`<C-t>` back to the list), `<A-j>` → Scrolls the **preview**, not the terminal. The layer decides, not the key 〔未訳〕
- [ ] **39.6** In the pane, run a program that reads Alt+j — `nvim` with `nnoremap <A-j> :m+1<CR>` → **It does see the key** from v0.38.0 — see section 40. Before that it did not 〔未訳〕
- [ ] **39.7** `[[term.prepend_keymap]]` binding `<A-j>` to `noop`, then `<C-F5>` → The key does nothing **and still does not reach the shell** — anything bound here is consumed. Handing it back needs a full `[term] keymap = [...]` replacement 〔未訳〕
- [ ] **39.8** Alt+b / Alt+f / Alt+d at the shell prompt → Still reach readline. Only j and k were taken 〔未訳〕
- [ ] **39.9** `<F1>` from inside the pane → The term layer's list shows `<A-j>` / `<A-k>` with their descriptions 〔未訳〕

## 40. 全画面プログラムにスクロールのジェスチャーを渡す — 0 / 13

- [ ] **40.1** `<C-t>`, `nvim` a long file, `<A-j>` / `<A-k>` with `nnoremap <A-j> :m+1<CR>` bound → **nvim sees the key.** The v0.37.0 collision is gone 〔未訳〕
- [ ] **40.2** In the same nvim, `<S-PageUp>` / `<S-PageDown>` / `<S-Home>` / `<S-End>` → All reach nvim. Every `term_scroll` key is handed over, not just the two 〔未訳〕
- [ ] **40.3** In the same nvim, `<C-t>` → **Still filer's** — it leaves the pane, with nvim left running. Non-scrolling keys are never handed over 〔未訳〕
- [ ] **40.4** Quit nvim, then `<A-j>` / `<A-k>` at the shell prompt → Back to scrolling filer's scrollback. The handover follows the program, not a setting 〔未訳〕
- [ ] **40.5** The wheel inside nvim, and inside `less` → Scrolls the document. Before v0.38.0 it tried to walk a scrollback that does not exist, so nothing moved 〔未訳〕
- [ ] **40.6** The wheel at the shell prompt → Still walks the scrollback, unchanged 〔未訳〕
- [ ] **40.7** `less` a long file, `<S-PageUp>`, then `q` to quit, then `<S-PageUp>` again → Inside `less` it pages the document; after quitting it scrolls the pane's scrollback 〔未訳〕
- [ ] **40.8** In nvim with `set nonumber`, wheel up then down → Lands back where it started — one notch is a fixed number of arrows each way 〔未訳〕
- [ ] **40.9** A program using the alternate screen **and** application-cursor mode → The wheel's arrows arrive as SS3 (`ESC O A`), not CSI. nvim in insert mode is the easy check 〔未訳〕
- [ ] **40.10** At a `bash`/`zsh` prompt in the pane, type a few words, then `Alt-b` / `Alt-f` → The cursor moves **by word**. Before v0.38.0 nothing happened — the key was dropped with no bytes behind it 〔未訳〕
- [ ] **40.11** `Alt-d` at the same prompt → Deletes the word ahead 〔未訳〕
- [ ] **40.12** PowerShell (PSReadLine) in the pane, `Alt-b` / `Alt-f` → Same word motions 〔未訳〕
- [ ] **40.13** `Alt-j` / `Alt-k` at an ordinary prompt → **Still filer's scroll** — these two are bound in the `[term]` layer, and the prompt is not the alternate screen 〔未訳〕

## 41. spot パネルの 4 つの provider — 0 / 14

- [ ] **41.1** `<Tab>` on a `.zip` from the fixtures → An **Archive** section: format, entry and folder counts, unpacked size, ratio, `Encrypted: no` 〔未訳〕
- [ ] **41.2** `<Tab>` on a zip made **encrypted by 7-Zip** → `Encrypted: yes (entries need a password)`, and the counts are still there. **Cannot be unit-tested — this build of `zip` has no AES writer, so no encrypted fixture can be made in-tree** 〔未訳〕
- [ ] **41.3** `<Tab>` on a 7z made with "encrypt file names" → `Encrypted: yes (the listing itself)` and **no counts at all** (nothing below is known) 〔未訳〕
- [ ] **41.4** `<Tab>` on an archive with more than 20,000 entries → The panel arrives without the window stalling, and says `Scanned: first 20,000 entries` 〔未訳〕
- [ ] **41.5** `<Tab>` on a CRLF file saved by Notepad, then on an LF one → The `Line endings` row tells them apart, with counts 〔未訳〕
- [ ] **41.6** `<Tab>` on a Notepad "UTF-16 LE" save → `Encoding: UTF-16 LE`, `BOM: UTF-16 LE (FF FE)` — **not** treated as binary 〔未訳〕
- [ ] **41.7** `<Tab>` on a 2 GB log → Rows arrive promptly, `Scanned: first 1.0 M of …`, and **no `Final newline` row** (the end was never read) 〔未訳〕
- [ ] **41.8** `<Tab>` on each of the six release binaries → `Architecture` matches the triple the artifact is named for — `x86_64` / `aarch64` 〔未訳〕
- [ ] **41.9** `<Tab>` on `C:\Windows\explorer.exe`, then on a `.dll` → `Windows GUI` / `DLL` 〔未訳〕
- [ ] **41.10** `<Tab>` on a real `.docx` / `.xlsx` / `.pptx` saved by Office → Author, revision, times marked **UTC**, word / page / slide counts 〔未訳〕
- [ ] **41.11** `<Tab>` on an old `.doc` → **No Document section, and no error** 〔未訳〕
- [ ] **41.12** Look at the key column on every new section → No key runs into the value column (`overlay.rs` hard-codes `key_w = 130.0`) 〔未訳〕
- [ ] **41.13** `<A-j>` down into a new section's rows, then `y` → The right value is copied. **`Act::Copy` counts rows across every section, so the new sections shift the indices** 〔未訳〕
- [ ] **41.14** `<Tab>` on a folder on a slow network drive → The panel still follows the cursor; the spot worker is newest-wins 〔未訳〕

## 42. ミニマップのホバーカード — 全 13 件が自動

`cargo test` が全部見ているので、押すものはありません。

## 43. CSV / TSV を表として見る — 0 / 1

自動テスト済みなので下には出していない: 43.1, 43.2, 43.3, 43.4, 43.5, 43.6, 43.7, 43.8, 43.10, 43.11, 43.12, 43.13

- [ ] **43.9** A 50 MB CSV → Opens promptly, cut at `max_text_bytes`, footer says truncated 〔未訳〕

## 44. ディスク使用量 — 0 / 13

- [ ] **44.1** `gu` in a project with a `node_modules` → Children largest first, with bars; `node_modules` near the top with a total far bigger than its own entry 〔未訳〕
- [ ] **44.2** `gu`, then `<Esc>` → Back in the directory, cursor where it was. The walk stops (no CPU after leaving) 〔未訳〕
- [ ] **44.3** `gu` on a tree with 300k+ files → Finishes, and says the walk was cut short and the totals are floors 〔未訳〕
- [ ] **44.4** `gu` in a folder holding a `.gitignore`d `target/` or `build/` → It is **counted**, not skipped 〔未訳〕
- [ ] **44.5** `gu` where a hidden folder holds most of the space → It is counted, and visible 〔未訳〕
- [ ] **44.6** `gu` on a folder with a symlink/junction to a big tree → The link is one entry, not a second copy of the tree, and no hang 〔未訳〕
- [ ] **44.7** `gu` at `C:\` → Answers; the biggest folders are plausible against WizTree or Explorer's own 〔未訳〕
- [ ] **44.8** `gu` on a network share (UNC) → Answers or fails gracefully; `<Esc>` still gets out mid-walk 〔未訳〕
- [ ] **44.9** `gu`, then `j`/`k`, `y`, `d`, space to select → All the ordinary list keys work — this is the list, not a panel 〔未訳〕
- [ ] **44.10** `gu`, then `Enter` on a folder → Ordinary navigation: it leaves the view and enters the folder. `gu` again measures from there 〔未訳〕
- [ ] **44.11** `gu` while a usage view is already up → Refused with a message, not a view with no way back 〔未訳〕
- [ ] **44.12** `gu`, then `,` to re-sort → The order changes (as asked); `gu` again restores largest-first 〔未訳〕
- [ ] **44.13** Compare a folder's total against Explorer's own properties → Within rounding. **Hard links read high — that is documented, not a bug** 〔未訳〕

## 45. 2 つのフォルダを比べる — 0 / 9

自動テスト済みなので下には出していない: 45.1, 45.2, 45.5, 45.7

- [ ] **45.3** `n` / `N` → Walks between the rows that are not `=`, skipping matches. At the end it says so 〔未訳〕
- [ ] **45.4** `gg` / `G` → First and last row 〔未訳〕
- [ ] **45.6** A tree where one file differs in its last byte only → That row is `~`, not `=` 〔未訳〕
- [ ] **45.8** A folder on one side where the other has a file of that name → `~` 〔未訳〕
- [ ] **45.9** Select one file and one folder, `<A-d>` → Refused with "compare two files, or two folders — not one of each" 〔未訳〕
- [ ] **45.10** Two `node_modules` (100k+ paths) → Answers, or says it was cut short; the window does not freeze 〔未訳〕
- [ ] **45.11** Two trees differing only in where a symlink points → The link row reads as differing 〔未訳〕
- [ ] **45.12** Split the view, stand on a folder in each pane, `<A-d>` → Compares those two 〔未訳〕
- [ ] **45.13** `q` / `<Esc>` → Closes, and two **files** still compare line by line as before 〔未訳〕

## 46. spot パネルの Git セクション — 0 / 11

準備:

```powershell
# git リポジトリの中で実行すること。fixtures の `repo` がそれ
cd $HOME\Desktop\filer-fixtures\repo
git log -1 --format="%h %an %ad %s"    # 期待値の答え合わせ用
```

- [ ] **46.1** `<Tab>` on a committed file → A **Git** section: `Last change` is a short hash and `YYYY-MM-DD HH:MM`, then `Subject` and `Author` 〔未訳〕
- [ ] **46.2** Check it against `git log -1 -- <that file>` → The same commit. Not the repository's newest — **the newest that touched this path** 〔未訳〕
- [ ] **46.3** `<Tab>` on a file changed by more than one commit → `Commits` appears with the count 〔未訳〕
- [ ] **46.4** `<Tab>` on a file added by exactly one commit → **No `Commits` row** — one says nothing the date has not 〔未訳〕
- [ ] **46.5** `<Tab>` on a file in a history of 50+ commits touching it → `Commits` reads `50+`, not a wrong total. The cap is there so a directory near the root reads a page, not the whole history 〔未訳〕
- [ ] **46.6** `<Tab>` on a **directory** → The last commit that touched anything inside it 〔未訳〕
- [ ] **46.7** `<Tab>` on a file that is new and never committed (`git status` shows `?`) → **No Git section at all** — nothing in the history touches it 〔未訳〕
- [ ] **46.8** `<Tab>` somewhere that is not a repository → No Git section, and no pause before the panel draws 〔未訳〕
- [ ] **46.9** The same on a machine with no `git` on `PATH` → No Git section, no error, and the rest of the panel is unaffected 〔未訳〕
- [ ] **46.10** `<Tab>` on a file whose last subject has Japanese in it, or an emoji → Drawn intact, not mojibake — the format is NUL-separated so nothing needs quoting 〔未訳〕
- [ ] **46.11** Watch for a console window → **None flashes.** `git` is spawned with `CREATE_NO_WINDOW`, the same as the status worker 〔未訳〕
