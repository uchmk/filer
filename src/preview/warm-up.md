# Warm-up

Parsed by the preview worker while it is idle, so that the first Markdown file
of a session is not the one that pays for reading and compiling the grammar.

## Inline

Text with **bold**, *italic*, ~~struck~~, `code`, a [link](https://example.com),
an ![image](a.png), a <https://example.com> autolink, <kbd>Ctrl</kbd>, and a
footnote[^1]. 日本語の段落も、`コード` と **太字** を混ぜて置く。

- [ ] a task
- [x] a done task
  1. nested `item`
> a quote with *emphasis*

| Column | Other |
| --- | :---: |
| `cell` | **bold** |

---

[^1]: The footnote.

```rust
fn main() {
    let v: Vec<u8> = vec![1, 2, 3];
    println!("{v:?}");
}
```

```toml
[package]
name = "x"
```

```powershell
Get-ChildItem -Path C:\dev | Where-Object { $_.Name -like "*.md" }
```

```sh
cargo test --release -- --nocapture
```
