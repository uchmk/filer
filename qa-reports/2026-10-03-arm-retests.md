# 2026-10-03 ARM64 レーン: Re-tests of changed behaviour（長い `TEMP` での `cargo test`）

無人実行（`auto-wintest.ps1 -Lane arm` が 22:02 に起動）。worktree は `C:\dev\filer-armtest`、
scratch は `C:\Users\yuu06\AppData\Local\Temp\filer-scratch\run-20261003-220207`（67 桁。この機械に RAM ディスクは無い）。
証拠は `C:\dev\filer-evidence\2026-10-03-arm-retests\`（`cargo test` の全ログ、負の対照のペインの行、親のないシェルの数）。

## どの節を取ったか

ARM64 の順番表の最初の節、**Re-tests of changed behaviour**。開いていたのは 1 項目だけで、
それがこのレーンのために書かれたもの（v0.73.29、#203 finding 1）なので、それを押した。

- 「the test suite」の行（毎回やる）は同じ実行で兼ねている。
- x64 の順番表の Re-tests にある新しい 2 行（**12.11a** と **13.12a**）は、**この機械では押せない**。
  どちらも `R:` が RAM ディスクであることが前提で、12.11a は「`d` が RAM ディスクで断られ、`R:` と `D` を名指す」こと、
  13.12a は「RAM ディスクへのジャンクションの解決」そのものを見る行。この機械の scratch は `C:` の上なので
  `d` はごみ箱に入って**成功してしまい**、行の期待値が成り立たない（`Test-Path R:\` = False）。x64 レーンの行として残す。
- **48.2 / 48.6** は「次のリリースまで何も無い」。`git tag --sort=-creatordate` の先頭は `v0.72.2` のままで、`Cargo.toml` は 0.73.29。押すものが無い。

## 環境

| | |
| --- | --- |
| 版 | 0.73.29（`1b39f25` = `origin/main` から `test/arm-retests`） |
| 実行したもの | `target\release\filer.exe`、`filer env` が `Process arch : aarch64` / `OS arch : aarch64` = **ネイティブ** |
| OS | Windows 11 Home 10.0.28000、PowerShell 7.6.6 |
| ツールチェイン | cargo 1.98.1 / rustc 1.98.1（CI と同じ stable） |
| ConPTY | `scripts\fetch-conpty.ps1` で `1.24.260710001 (arm64)` を `target\release` に置いた |
| 特権 | **無し**（`IsInRole('Administrators')` = False）。この実行の全部が特権を要らない |
| RAM ディスク | **無し**（`Test-Path R:\` = False）。`TEMP` / `TMP` はスクリプトが決めた 67 桁の scratch |
| 画面 | `Get-Process LogonUI` = 0（実行の最初と最後）。filer の窓を手で駆動していないので入力デスクトップは問わない |

- **親のないシェルの数（#180）: 前 57 シェル / 48 親なし → 後 57 / 48。増加なし。**
- 実行の終わりに `Get-Process filer` = 0。

## Re-tests: 長い `TEMP` のままスイート全体が緑（631 / 0）

**結果: 緑。** スクリプトが決めた scratch を `TEMP` に入れたまま、短い `C:\t33` 風の `TEMP` に逃げずに回した。

| 実行 | `test result` |
| --- | --- |
| 1 回目 | `ok. 631 passed; 0 failed; 0 ignored`（4.26 s、ビルドを含めて 30.1 s） |
| 2 回目（ゆらぎの確認） | `ok. 631 passed; 0 failed; 0 ignored`（3.83 s） |

どちらも `filer-com` の `tests::it_waits_only_for_what_answers_in_text` が別に 1 / 0。合計 **632 / 0**。
#205（0.73.25）は長い `TEMP` で 629 / 1 だったので、**落ちていた 1 件が落ちなくなり、件数も 630 → 631 に増えている。**

`app::escape_and_max_preview::sending_a_name_opens_a_closed_pane` は、全体の中で `... ok`（ログ 633 行目）。
**単独でも 5 回連続で緑**（1.92 / 1.95 / 1.95 / 2.02 / 2.00 s）。#203 は「長い `TEMP` なら毎回落ちる」と書いていたので、5 回は偶然ではない。

### 緑が本物かを、負の対照で確かめた

v0.73.29 の修正は**テスト側の変更**なので、ただ緑になっただけでは
「折り返しが起きなくなった（＝この実行は何も試していない）」のかもしれない。そこで `src\app.rs` を
**修正前の行ごとの `contains` に一時的に戻して**同じ `TEMP` で回した。

```
test app::escape_and_max_preview::sending_a_name_opens_a_closed_pane ... FAILED
panicked at src\app.rs:8193: the name never reached the shell
test result: FAILED. 0 passed; 1 failed ... finished in 20.47s
```

**修正前なら同じ機械・同じ `TEMP` で落ちる。**つまり折り返しは今も起きており、緑は修正が効いた結果。
そのときのペインの行（80 桁。`eprintln` を足して読んだ。括弧は桁数）:

```
ROW[80]: "❯  C:\Users\yuu06\AppData\Local\Temp\filer-scratch\run-20261003-220207\filer-q35"
ROW[80]: "-send-app-escape_and_max_preview-sending_a_name_opens_a_closed_pane-7784\q35-mar"
ROW[7]:  "ker.txt"
```

`q35-marker.txt` が **`q35-mar` と `ker.txt` に割れて 2 行にまたがっている**。どの 1 行も名前を含まないので、
行ごとに探すと見つからない。#203 が立てた筋のとおりで、**行をつないでから探す v0.73.29 がその場所を直している。**
`src\app.rs` はこのあと `git checkout -- src/app.rs` で戻し、`git status` は空（この PR に負の対照は入っていない）。

### 同じ種類の見落としが他に無いかも見た

ペインの中身を**行ごとに**文字列として探している所は、`src/` 全体で**この 1 か所だけ**だった
（`crate::terminal::snapshot(` の呼び出しは `src\app.rs:8186` のみ。`with_grid` の他の使い方は
`alt_screen` / `app_cursor` / `ui\term.rs` の描画で、文字列を探していない）。長い `TEMP` で同じ形で
割れるテストは他に無い。

## findings

**無し。**この実行で押した項目はすべて期待どおりだった。

## Proposals

### 1. 折り返しで見つからなかったときのメッセージが、嘘に近い

- **何に行き当たったか**: 負の対照を回したとき、落ちたテストが言ったのは
  `the name never reached the shell`（名前はシェルに届かなかった）。**届いていた。**
  ペインには `…\q35-mar` / `ker.txt` と出ていて、割れていただけ。
  何が起きたのかを知るために、自分で `eprintln!` を足してペインの行を印刷する必要があった。
  #203 が同じ所で時間を使っているので、2 回払っている。
- **どう変えるか**: 待ちきれなかったときの `assert!` を、ペインの snapshot を添えた形にする。
  つないだ文字列と行の桁数を出せば、「届いていないのか、折り返しているのか、プロンプトがまだ出ていないのか」が
  その場で分かる。`assert!(…, "the name never reached the shell; pane was:\n{}", rows.join("\n"))` の形。
- **なぜ**: 失敗の文言が原因を名指していないテストは、落ちるたびに同じ調査を繰り返させる。
  ペインを相手にするテストは、この機械でしか落ちない（長い `TEMP`）ので、調査する者はいつもこのレーン。
- **大きさ**: 数行。`echoed` のクロージャをつないだ文字列を返す形にして、`assert!` に渡すだけ。

### 2. テスト側の修正は、直した条件そのものを主張しておくべき

- **何に行き当たったか**: v0.73.29 が緑になったことは、**それ自体では修正が効いた証拠にならない**。
  折り返しが起きなくなっただけかもしれない。確かめるために修正を手で巻き戻して回す（上の負の対照）必要があった。
  この手間は、この行を再テストする実行が毎回払うことになる。
- **どう変えるか**: このテストに「折り返しが起きていること」を 1 行書き足す。つないだ文字列が
  フルパスを含むことを確かめれば（`contains(dir.to_string_lossy())`）、**どの 1 行にも収まらない長さであること**が
  テスト自身の主張になる。あるいは 1 行に収まっていてもよいのだから、「つないだ結果に名前がある」を
  別の短い `TEMP` でも回す形にしてもよい。
- **なぜ**: 「環境がある条件を満たすときだけ落ちる」バグの修正は、その条件が消えると黙って意味を失う。
  この機械の scratch は毎回 67 桁だが、パスの作り方が変われば短くなる。
- **大きさ**: 1 行の `assert!`。

### 3. `fetch-conpty.ps1` を `pwsh -File` で呼ぶと、持ち主のプロファイルの失敗が先に出る

- **何に行き当たったか**: 役割定義の前置きどおり `pwsh -File …\fetch-conpty.ps1` を回したら、
  成功の行（`ConPTY 1.24.260710001 (arm64) -> …`）の**前に** 5 行のエラーが出た:
  `Set-PSReadLineOption -PredictionSource History` が
  `the console output doesn't support virtual terminal processing or it's redirected` で失敗している。
  持ち主の `$PROFILE` の 32 行目で、出力がリダイレクトされている無人実行では必ず出る。
- **どう変えるか**: 前置きの行を `pwsh -NoProfile -File …` にする。
- **なぜ**: ログの先頭に赤い 5 行があると、無人実行の記録を後から読む者（と次の実行の Agent）が
  ConPTY の取得が失敗したのかを確かめ直すことになる。スクリプトは持ち主のプロファイルを 1 つも要らない。
  おまけに `$PROFILE` の読み込みは 765 ms かかっている（ペインのログに出ていた）。
- **大きさ**: 一語。`windows-role.md` の「How to work」の 1 行。

## Votes

**この実行で入れる票は無い。**`状態: 投票中` は Q57 と Q58 の 2 件で、**どちらも `投票` 欄に
`arm:` の行がすでにある**（Q57 は `arm: 1`、Q58 は `arm: 2`。#203 が入れた）。
役割定義の「`投票` 欄に自分のレーンの行がまだ無いもの」に当たるものが無い。
どちらも `多数決: 1` が 2026-10-03 に揃っており、2026-10-04 から進めてよい状態。

## Queue

ARM64 の順番表（`windows-role.md` の「The ARM64 lane」）について。

1. **「Re-tests of changed behaviour」の行の本文を空にする。**開いていた 1 項目
   （v0.73.29 / #203 finding 1 の長い `TEMP` での `cargo test`）はこの実行で片付いた。
   x64 の行と同じ言い方で `Nothing else is open in this row right now; go on to the next row.` に
   してほしい。**行そのものは残す**（「First, always」と「このレーン自身の長い scratch で `cargo test` を回す」は
   毎回の規則として効いている）。これをしないと、次の実行がまた同じ `cargo test` 1 本だけで終わる。
2. **「the test suite」の行の版の並びに `0.73.29 (631 / 0, #<この PR>)` を足す。**
   0.73.25 が長い `TEMP` で 629 / 1 だったことと合わせて読めるように、`631 / 0、長い scratch のまま` と
   書き添えてもらえると、この行が何を見ているかが残る。
3. 削除の提案は無い。これで次の実行は **「19. the wheel, on ARM64」**（5 行）を取る。
