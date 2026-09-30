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
pub fn version_line() -> String {
    format!("filer {} ({})", env!("CARGO_PKG_VERSION"), std::env::consts::ARCH)
}

/// The OS line, in the shape the form's own PowerShell snippet produces, so a
/// report that was filled in by hand and one that came from here read alike.
///
/// `ProcessArchitecture` is this program's own architecture, which is the
/// question worth answering on Windows on ARM: an x64 build running under
/// emulation is the case that quietly misleads everyone, and here the program
/// is the emulated process, so it can say so directly.
/// Everything the PowerShell snippet in the report form prints, so that a
/// report filled in from here needs no correcting afterwards. It used to print
/// less, and the first person to use it went and pasted the fuller version over
/// the top -- which the next `<F12>` then overwrote, since the value lives in
/// the URL.
///
/// Both architectures, because one of them is the whole point. On Windows on
/// ARM an x64 build runs under emulation and every ordinary check agrees it is
/// on x64; the machine underneath is what `GetNativeSystemInfo` answers, and it
/// is unaffected by the emulation. When the two lines disagree, the
/// disagreement is the finding.
#[cfg(windows)]
pub fn os_line() -> String {
    format!(
        "OS: {}\nOS arch: {}\nProcess arch: {}",
        windows_name(),
        native_arch(),
        std::env::consts::ARCH,
    )
}

/// `Windows 11 Pro 25H2 (build 26200.9457)`, assembled from the two places
/// Windows keeps the pieces.
#[cfg(windows)]
fn windows_name() -> String {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;

    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // `RtlGetVersion` rather than `GetVersionEx`, which lies about anything
    // past Windows 8 unless the binary carries a compatibility manifest.
    if unsafe { RtlGetVersion(&mut info) }.is_err() {
        return "Windows (version unavailable)".to_owned();
    }

    // Not `ProductName` from the registry: on Windows 11 it still reads
    // "Windows 10 Pro", which is the single most misleading string on the
    // machine. The build number is what actually separates the two.
    let mut name = if info.dwBuildNumber >= 22000 { "Windows 11" } else { "Windows 10" }.to_owned();

    // The registry spells two editions the way nobody else does: "Professional"
    // is "Pro" on the box and the About page, and "Core" is Home -- the ARM64
    // laptop's report read `Windows 11 Core`, which names no edition anyone
    // has bought.
    if let Some(ed) = reg_string("EditionID") {
        name.push(' ');
        name.push_str(edition_name(&ed));
    }
    // 25H2 and friends. Absent on builds old enough to use `ReleaseId`, and a
    // missing feature update is not worth an apology in the middle of a line.
    if let Some(display) = reg_string("DisplayVersion") {
        name.push(' ');
        name.push_str(&display);
    }
    match reg_dword("UBR") {
        Some(ubr) => format!("{name} (build {}.{ubr})", info.dwBuildNumber),
        None => format!("{name} (build {})", info.dwBuildNumber),
    }
}

/// `EditionID` as the edition is sold. Only the two the registry spells
/// differently are mapped; anything else (`Enterprise`, `Education`) already
/// reads right.
#[cfg(any(windows, test))]
fn edition_name(id: &str) -> &str {
    match id {
        "Professional" => "Pro",
        "Core" => "Home",
        other => other,
    }
}

/// The architecture of the machine, not of this process -- the whole reason
/// the line exists is to show the two disagreeing under emulation.
///
/// `IsWow64Process2`'s native machine first. `GetNativeSystemInfo` was used
/// alone until v0.51.2 on the strength of its name, and ARM64's x64 emulation
/// answers it with AMD64 for compatibility: the ARM64 laptop's x64 build said
/// `OS arch x86_64`, so a report from an emulated build claimed an x64
/// machine (TESTING.md 26.6 / 25.5). Only the *native* half is read: for an
/// x64 process on ARM64 the process half comes back `UNKNOWN`, since that is
/// emulation and not WOW64. `IsWow64Process2` is linked at load time and needs
/// Windows 10 1709; everything older is long out of support.
#[cfg(windows)]
fn native_arch() -> &'static str {
    use windows::Win32::System::SystemInformation::{
        GetNativeSystemInfo, IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
        IMAGE_FILE_MACHINE_I386, PROCESSOR_ARCHITECTURE_AMD64, PROCESSOR_ARCHITECTURE_ARM64,
        PROCESSOR_ARCHITECTURE_INTEL, SYSTEM_INFO,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};

    let (mut process, mut native) = (IMAGE_FILE_MACHINE::default(), IMAGE_FILE_MACHINE::default());
    if unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, Some(&mut native)) }.is_ok() {
        if native == IMAGE_FILE_MACHINE_ARM64 {
            return "aarch64";
        } else if native == IMAGE_FILE_MACHINE_AMD64 {
            return "x86_64";
        } else if native == IMAGE_FILE_MACHINE_I386 {
            return "x86";
        }
    }

    let mut si = SYSTEM_INFO::default();
    unsafe { GetNativeSystemInfo(&mut si) };
    // Spelled the way Rust spells `std::env::consts::ARCH`, so the two lines
    // can be compared without translating between them.
    let arch = unsafe { si.Anonymous.Anonymous.wProcessorArchitecture };
    if arch == PROCESSOR_ARCHITECTURE_AMD64 {
        "x86_64"
    } else if arch == PROCESSOR_ARCHITECTURE_ARM64 {
        "aarch64"
    } else if arch == PROCESSOR_ARCHITECTURE_INTEL {
        "x86"
    } else {
        "unknown"
    }
}

/// One string from `CurrentVersion`. `None` for anything that goes wrong: a
/// bug report missing the edition is still a bug report.
#[cfg(windows)]
fn reg_string(name: &str) -> Option<String> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut buf = [0u16; 128];
    let mut bytes = std::mem::size_of_val(&buf) as u32;
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    }
    .is_ok()
    .then_some(())?;
    // The count comes back in bytes and includes the terminator.
    let chars = (bytes as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..chars.min(buf.len())]))
}

/// The update build revision -- the `.9457` that `winver` shows and the version
/// struct does not carry.
#[cfg(windows)]
fn reg_dword(name: &str) -> Option<u32> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD};

    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut value = 0u32;
    let mut bytes = std::mem::size_of::<u32>() as u32;
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&mut bytes),
        )
    }
    .is_ok()
    .then_some(())?;
    Some(value)
}

#[cfg(not(windows))]
pub fn os_line() -> String {
    // No emulation story to tell here, so the two arch lines would say the same
    // thing twice.
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

    /// Home and Pro as they are sold, not as the registry spells them; the rest
    /// untouched. Runs everywhere: it is a table, not a Windows call.
    #[test]
    fn editions_read_as_they_are_sold() {
        assert_eq!(edition_name("Core"), "Home");
        assert_eq!(edition_name("Professional"), "Pro");
        assert_eq!(edition_name("Enterprise"), "Enterprise");
        assert_eq!(edition_name("Education"), "Education");
    }

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
