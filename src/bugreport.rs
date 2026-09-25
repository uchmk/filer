//! Open a bug report with the machine's own answers already filled in.
//!
//! The report form asks for a version, an architecture and an OS build before
//! it asks what went wrong. Those are the three things the program knows about
//! itself and the reporter has to go and look up — and looking them up is where
//! a report gets abandoned. GitHub's issue forms take query parameters keyed by
//! the field `id`, so the program can answer its own questions and leave the
//! human with only the part it cannot know: what happened, and which keys.

/// Where the form lives. A fork should point this at its own tracker; the
/// program cannot know it has been forked, and silently filing reports against
/// somebody else's repository would be worse than not opening a browser.
const REPO: &str = "https://github.com/uchmk/filer";

/// The template's filename, and the `id`s of the two fields filled in here,
/// from `.github/ISSUE_TEMPLATE/bug_report.yml`. Renaming a field there without
/// changing it here does not break anything loudly: GitHub ignores a parameter
/// it does not recognise, and the field simply comes up empty.
const TEMPLATE: &str = "bug_report.yml";

/// The URL that opens a pre-filled report.
pub fn url() -> String {
    format!(
        "{REPO}/issues/new?template={TEMPLATE}&version={}&os={}",
        encode(&version_line()),
        encode(&os_line()),
    )
}

/// What `--version` prints, and for the same reason: with more than one Windows
/// binary in circulation, which one is running is the first thing to settle.
fn version_line() -> String {
    format!("filer {} ({})", env!("CARGO_PKG_VERSION"), std::env::consts::ARCH)
}

/// The OS line, in the shape the form's own PowerShell snippet produces, so a
/// report that was filled in by hand and one that came from here read alike.
///
/// `ProcessArchitecture` is this program's own architecture, which is the
/// question worth answering on Windows on ARM: an x64 build running under
/// emulation is the case that quietly misleads everyone, and here the program
/// is the emulated process, so it can say so directly.
#[cfg(windows)]
fn os_line() -> String {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;

    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // `RtlGetVersion` rather than `GetVersionEx`, which lies about anything
    // past Windows 8 unless the binary carries a compatibility manifest.
    let build = if unsafe { RtlGetVersion(&mut info) }.is_ok() {
        // Windows 11 still reports itself as major version 10; the build number
        // is what tells the two apart.
        let name = if info.dwBuildNumber >= 22000 { "Windows 11" } else { "Windows 10" };
        format!("{name} (build {})", info.dwBuildNumber)
    } else {
        "Windows (version unavailable)".to_owned()
    };
    format!("OS: {build}\nProcess arch: {}", std::env::consts::ARCH)
}

#[cfg(not(windows))]
fn os_line() -> String {
    format!("OS: {}\nProcess arch: {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Percent-encode for a query string.
///
/// Written out rather than pulled in: it is a dozen lines against a new
/// dependency, and the rule is small enough to state. Everything outside the
/// unreserved set of RFC 3986 is escaped, which is stricter than a browser
/// needs and never wrong.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_characters_a_query_string_cannot_carry() {
        assert_eq!(encode("filer 0.1.0 (x86_64)"), "filer%200.1.0%20%28x86_64%29");
        // The ones that would end the parameter or start another.
        assert_eq!(encode("a&b=c#d"), "a%26b%3Dc%23d");
        // A newline separates the OS line from the architecture line.
        assert_eq!(encode("a\nb"), "a%0Ab");
        assert_eq!(encode("-_.~"), "-_.~");
    }

    #[test]
    fn encodes_beyond_ascii_one_byte_at_a_time() {
        // UTF-8 is escaped per byte, not per character: a path or a config
        // value in a report may well be Japanese.
        assert_eq!(encode("あ"), "%E3%81%82");
    }

    #[test]
    fn the_url_carries_the_template_and_both_fields() {
        let u = url();
        assert!(u.starts_with("https://github.com/uchmk/filer/issues/new?"), "{u}");
        assert!(u.contains("template=bug_report.yml"), "{u}");
        assert!(u.contains("&version=filer%20"), "{u}");
        assert!(u.contains("&os=OS%3A%20"), "{u}");
        // Nothing unescaped can have leaked in and split the parameters.
        assert!(!u.contains(' '), "{u}");
    }

    #[test]
    fn the_version_line_names_the_architecture() {
        let v = version_line();
        assert!(v.contains(env!("CARGO_PKG_VERSION")), "{v}");
        assert!(v.contains(std::env::consts::ARCH), "{v}");
    }
}
