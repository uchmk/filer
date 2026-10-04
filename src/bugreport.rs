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

/// The URL that opens a pre-filled report: the version and OS lines, and
/// any `extra` fields as `(id, text)`, ids from the template.
pub fn url(extra: &[(&str, String)]) -> String {
    let mut u = format!(
        "{REPO}/issues/new?template={TEMPLATE}&version={}&os={}",
        encode(&version_line()),
        encode(&os_line()),
    );
    for (id, text) in extra.iter().filter(|(_, t)| !t.is_empty()) {
        u.push_str(&format!("&{id}={}", encode(text)));
    }
    u
}

/// What the program knew when `<F12>` was pressed, beyond the version and
/// the OS (Q64): the last error it raised, how it was drawing, and which
/// config files it read -- by name, `yazi\keymap.toml`, never by path, since
/// a path carries the user's name onto a public tracker.
pub fn context(last_error: Option<&str>, loaded: &[std::path::PathBuf]) -> String {
    let mut lines = Vec::new();
    if let Some(e) = last_error {
        lines.push(format!("Last error: {e}"));
    }
    if let Some(info) = crate::runinfo::load() {
        if !info.adapter.is_empty() {
            lines.push(format!("Rendering: {} ({}, {})", info.adapter, info.backend, info.device));
        }
        if info.ppp > 0.0 {
            lines.push(format!("Scale: {:.0}%", info.ppp * 100.0));
        }
    }
    let names: Vec<String> = loaded.iter().map(|p| short_name(p)).collect();
    lines.push(format!("Config: {}", if names.is_empty() { "defaults only".into() } else { names.join(", ") }));
    lines.join("\n")
}

/// `…\yazi\config\keymap.toml` as `yazi\config\keymap.toml` -- the folders
/// that say whose file it is, without the home directory above them.
fn short_name(p: &std::path::Path) -> String {
    let parts: Vec<String> = p.components().rev().take(3).map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let tail: Vec<&String> = parts.iter().rev().skip_while(|c| !matches!(c.as_str(), "yazi" | "filer")).collect();
    let keep = if tail.is_empty() { parts.iter().rev().skip(1).collect() } else { tail };
    keep.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(std::path::MAIN_SEPARATOR_STR)
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
        process_arch(native_arch(), std::env::consts::ARCH),
    )
}

/// The process's own architecture, saying so outright when it is not the
/// machine's. Two lines that disagree were the finding, but only to a reader
/// who compared them; #84 read past it twice.
#[cfg(any(windows, test))]
fn process_arch(native: &str, own: &str) -> String {
    if native == own { own.to_owned() } else { format!("{own} (emulated on {native})") }
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

    /// An x64 build on an ARM64 machine says it is emulated in the line itself.
    #[test]
    fn an_emulated_process_says_so() {
        assert_eq!(process_arch("aarch64", "x86_64"), "x86_64 (emulated on aarch64)");
        assert_eq!(process_arch("aarch64", "aarch64"), "aarch64");
    }

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

    /// Q64: a config file is named by its folder and name, never its path.
    #[test]
    fn a_config_file_is_named_without_its_path() {
        let sep = std::path::MAIN_SEPARATOR;
        let p = |s: &str| std::path::PathBuf::from(s.replace('/', &sep.to_string()));
        assert_eq!(short_name(&p("/home/someone/.config/yazi/keymap.toml")), format!("yazi{sep}keymap.toml"));
        assert_eq!(short_name(&p("/Users/someone/AppData/Roaming/yazi/config/yazi.toml")), format!("yazi{sep}config{sep}yazi.toml"));
        assert_eq!(short_name(&p("/somewhere/else/conf/filer.toml")), format!("conf{sep}filer.toml"));
        let c = context(Some("Copy: a.txt: denied"), &[p("/home/someone/.config/filer/filer.toml")]);
        assert!(c.starts_with("Last error: Copy: a.txt: denied\n"), "{c}");
        assert!(c.ends_with(&format!("Config: filer{sep}filer.toml")), "{c}");
        assert!(!c.contains("someone"), "no home directory: {c}");
    }

    #[test]
    fn the_url_carries_extra_fields_by_id() {
        let u = url(&[("keys", "j j".into()), ("context", String::new())]);
        assert!(u.ends_with("&keys=j%20j"), "{u}");
        assert!(!u.contains("context="), "an empty field is left out: {u}");
    }

    #[test]
    fn the_url_carries_the_template_and_both_fields() {
        let u = url(&[]);
        assert!(u.starts_with("https://github.com/uchmk/filer/issues/new?"), "{u}");
        assert!(u.contains("template=bug_report.yml"), "{u}");
        assert!(u.contains("&version=filer%20"), "{u}");
        assert!(u.contains("&os=OS%3A%20"), "{u}");
        // Nothing unescaped can have leaked in and split the parameters.
        assert!(!u.contains(' '), "{u}");
    }

    /// The form itself, as GitHub reads it.
    const FORM: &str = include_str!("../.github/ISSUE_TEMPLATE/bug_report.yml");

    /// Row 26.2 of TESTING.md, in part: every heading on the form is in English and Japanese,
    /// `What happened / 何が起きたか`, the form's own title included (Q63).
    #[test]
    fn every_heading_on_the_form_is_in_both_languages() {
        let headings: Vec<&str> = FORM
            .lines()
            .filter_map(|l| l.trim_start().strip_prefix("label: ").or_else(|| l.strip_prefix("name: ")))
            .collect();
        assert!(headings.len() >= 8, "the form's title and its fields: {headings:?}");
        for h in headings {
            let (en, ja) = h.split_once(" / ").unwrap_or_else(|| panic!("{h:?} has no ` / `"));
            assert!(en.is_ascii() && !en.trim().is_empty(), "English first: {h:?}");
            assert!(!ja.is_ascii(), "Japanese second: {h:?}");
        }
    }

    /// Row 26.2 of TESTING.md, in part: each field the link fills in is one the form has.
    /// GitHub drops a parameter it does not know without a word, so a field
    /// renamed in the form would simply come up empty.
    #[test]
    fn the_fields_the_link_fills_are_the_forms_own() {
        let ids: Vec<&str> = FORM.lines().filter_map(|l| l.trim_start().strip_prefix("id: ")).collect();
        let u = url(&[("keys", "j".into()), ("context", "Config: defaults only".into())]);
        let (_, query) = u.split_once('?').unwrap();
        for (name, _) in query.split('&').filter_map(|p| p.split_once('=')) {
            match name {
                "template" => assert_eq!(&u[..u.find('?').unwrap()], format!("{REPO}/issues/new")),
                id => assert!(ids.contains(&id), "{id:?} is not a field of the form: {ids:?}"),
            }
        }
        assert!(query.contains(&format!("template={TEMPLATE}&")), "{u}");
        for id in ["version", "os", "keys", "context"] {
            assert!(query.contains(&format!("&{id}=")), "{id} is filled: {u}");
        }
    }

    #[test]
    fn the_version_line_names_the_architecture() {
        let v = version_line();
        assert!(v.contains(env!("CARGO_PKG_VERSION")), "{v}");
        assert!(v.contains(std::env::consts::ARCH), "{v}");
    }
}
