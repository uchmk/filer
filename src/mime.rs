//! Extension-based mime guessing, good enough for `[open].rules` and for
//! deciding how to preview a file. Content sniffing happens in the preview
//! worker, which is the only place that reads bytes.

use crate::fs::Entry;

pub fn guess(entry: &Entry) -> &'static str {
    if entry.is_dir_like() {
        return "inode/directory";
    }
    let Some(ext) = entry.ext.as_deref() else {
        return by_name(&entry.name);
    };
    match ext {
        // text & code
        "txt" | "log" | "text" | "me" => "text/plain",
        "md" | "markdown" | "mdx" => "text/markdown",
        "rs" => "text/rust",
        "go" => "text/go",
        "py" | "pyi" => "text/x-python",
        "rb" => "text/x-ruby",
        "c" | "h" => "text/x-c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "text/x-c++",
        "cs" => "text/x-csharp",
        "java" | "kt" | "kts" => "text/x-java",
        "js" | "mjs" | "cjs" | "jsx" => "text/javascript",
        "ts" | "tsx" => "text/typescript",
        "html" | "htm" | "xhtml" => "text/html",
        "css" | "scss" | "sass" | "less" => "text/css",
        "json" | "jsonc" => "application/json",
        "toml" => "text/toml",
        "yaml" | "yml" => "text/yaml",
        "xml" => "text/xml",
        "ini" | "cfg" | "conf" | "properties" | "editorconfig" => "text/plain",
        "sh" | "bash" | "zsh" | "fish" => "text/x-shellscript",
        "ps1" | "psm1" => "text/x-powershell",
        "bat" | "cmd" => "text/x-batch",
        "lua" => "text/x-lua",
        "sql" => "text/x-sql",
        "php" => "text/x-php",
        "swift" => "text/x-swift",
        "zig" => "text/zig",
        "vim" => "text/x-vim",
        "csv" | "tsv" => "text/csv",
        "diff" | "patch" => "text/x-diff",
        "lock" => "text/plain",

        // images
        "png" => "image/png",
        "jpg" | "jpeg" | "jfif" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "ico" => "image/vnd.microsoft.icon",
        "webp" => "image/webp",
        "tif" | "tiff" => "image/tiff",
        "qoi" => "image/qoi",
        "pnm" | "pgm" | "ppm" | "pbm" => "image/x-portable-anymap",
        "avif" => "image/avif",
        "heic" | "heif" => "image/heic",
        "jxl" => "image/jxl",
        "psd" => "image/vnd.adobe.photoshop",
        "svg" => "image/svg+xml",

        // audio / video
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "m4a" | "aac" => "audio/aac",
        "opus" => "audio/opus",
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",

        // archives & binaries
        "zip" => "application/zip",
        "gz" | "tgz" => "application/gzip",
        "bz2" => "application/x-bzip2",
        "xz" => "application/x-xz",
        "zst" => "application/zstd",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "tar" => "application/x-tar",
        "iso" | "img" => "application/x-iso9660-image",
        "exe" | "msi" | "com" => "application/vnd.microsoft.portable-executable",
        "dll" | "so" | "dylib" | "pdb" | "lib" | "a" | "o" | "obj" => "application/x-sharedlib",
        "pdf" => "application/pdf",
        "doc" | "docx" => "application/msword",
        "xls" | "xlsx" => "application/vnd.ms-excel",
        "ppt" | "pptx" => "application/vnd.ms-powerpoint",
        "ttf" | "otf" | "ttc" | "woff" | "woff2" => "font/sfnt",
        "db" | "sqlite" | "sqlite3" => "application/vnd.sqlite3",

        _ => "application/octet-stream",
    }
}

fn by_name(name: &str) -> &'static str {
    match name {
        "Makefile" | "makefile" | "Dockerfile" | "LICENSE" | "README" | "COPYING"
        | "CHANGELOG" | "AUTHORS" | "NOTICE" | ".gitignore" | ".gitattributes"
        | ".editorconfig" | ".env" | ".npmrc" | ".bashrc" | ".zshrc" | ".profile" => "text/plain",
        _ => "application/octet-stream",
    }
}

pub fn is_text(mime: &str) -> bool {
    mime.starts_with("text/") || matches!(mime, "application/json")
}

pub fn is_image(mime: &str) -> bool {
    mime.starts_with("image/")
}

/// Images this build can actually decode; the rest fall back to a metadata card.
pub fn is_decodable_image(mime: &str) -> bool {
    matches!(
        mime,
        "image/png"
            | "image/jpeg"
            | "image/gif"
            | "image/bmp"
            | "image/vnd.microsoft.icon"
            | "image/webp"
            | "image/tiff"
            | "image/qoi"
            | "image/x-portable-anymap"
    )
}

/// Syntect language token for a mime / extension pair. The mime wins so that
/// variants syntect doesn't know by extension (`mdx`, `jsonc`) still
/// get their base language.
pub fn syntax_hint(mime: &str, ext: Option<&str>) -> Option<String> {
    let by_mime = match mime {
        "text/markdown" => Some("md"),
        "text/html" => Some("html"),
        "text/css" => Some("css"),
        "application/json" => Some("json"),
        "text/toml" => Some("toml"),
        "text/yaml" => Some("yaml"),
        "text/xml" => Some("xml"),
        _ => None,
    };
    by_mime.or(ext).map(str::to_string)
}
