# assets

| File | What it is |
| --- | --- |
| `icon.svg` | The icon, and the only copy that is edited |
| `icon.ico` | The same drawing at 16, 32, 48, 64, 128 and 256 pixels, for Windows |

`icon.ico` is generated, not drawn. After changing the SVG, rebuild it:

```
cargo run --example make-icon
```

Both are used. The window's icon — the title bar, Alt+Tab, the taskbar button
of a running kura — is rasterized from the SVG at startup, so it works on every
platform and needs nothing but the vector. The icon Explorer draws on
`kura.exe` itself is the `.ico`, compiled into the binary as a Windows resource
by `build.rs`, which only happens when both the host and the target are Windows:
it needs the SDK's `rc.exe`.

## Licensing

The rest of this repository is under MIT or Apache-2.0, at your option. **The
artwork in this directory is not part of that grant.** It is the project's mark
rather than its code, and is kept here so the program can be built with it, not
so it can be reused elsewhere. A fork should replace `icon.svg` with its own and
rerun the command above.
