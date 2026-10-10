//! Compile the Windows resource that gives `kura.exe` its own icon.
//!
//! This is the icon Explorer draws on the file, and the one a shortcut or a
//! pinned taskbar entry uses. It is a different thing from the window's icon,
//! which `main.rs` sets at runtime from the same SVG — a resource is baked into
//! the binary and belongs to the file rather than to the running program.
//!
//! Only when the host is Windows *and* the target is Windows: building the
//! resource needs the Windows SDK's `rc.exe`, which a Linux or macOS host does
//! not have. A cross build to Windows from elsewhere therefore goes without the
//! file icon rather than failing over a nicety — the window's icon, which is
//! rendered from the SVG at startup, still works everywhere.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    embed_icon();
}

#[cfg(windows)]
fn embed_icon() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    // A warning rather than an error: an executable without its icon is worth
    // having, and whoever is building it should hear about it rather than be
    // stopped by it.
    if let Err(e) = res.compile() {
        println!("cargo:warning=kura.exe has no icon: {e}");
    }
}
