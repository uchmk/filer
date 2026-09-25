//! What a file server offers, for the one path that is not a directory.
//!
//! `\\10.0.0.1` looks like a folder and is not one. There is nothing on any
//! disk to read: the shares behind it are names the server keeps, and asking
//! the filesystem for them fails whatever the server holds — `read_dir` returns
//! "The filename, directory name, or volume label syntax is incorrect" for a
//! host that is answering perfectly well. Explorer does not read it either; it
//! asks the network provider what is being shared and draws the answer as
//! folders. So does this.
//!
//! Through `mpr.dll` rather than `netapi32`'s `NetShareEnum`, because the
//! provider goes out over the same redirector session Explorer is using: a host
//! reachable in Explorer is reachable here, with the credentials already
//! established, and a host that needs a login it has not been given fails the
//! same way in both.

use std::path::Path;

use super::Entry;

/// The shares `host` (`\\name` or `\\10.0.0.1`) is offering, as directories.
#[cfg(windows)]
pub fn list(host: &Path) -> std::io::Result<Vec<Entry>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, HANDLE, NO_ERROR};
    use windows::Win32::NetworkManagement::WNet::{
        NETRESOURCEW, RESOURCE_GLOBALNET, RESOURCETYPE_ANY, RESOURCETYPE_DISK,
        RESOURCEUSAGE_CONTAINER, WNET_OPEN_ENUM_USAGE, WNetCloseEnum, WNetEnumResourceW,
        WNetOpenEnumW,
    };

    // `RESOURCEDISPLAYTYPE_SERVER`. Spelled out because the `windows` crate
    // keeps it under `Networking::WinSock`, a whole feature to enable for one
    // number, and the field it goes in is a bare `u32` anyway.
    const DISPLAY_SERVER: u32 = 2;

    let mut name: Vec<u16> = host.as_os_str().encode_wide().chain([0]).collect();
    let ask = NETRESOURCEW {
        dwScope: RESOURCE_GLOBALNET,
        dwType: RESOURCETYPE_ANY,
        // What kind of container this is. Without it the provider is left to
        // work out from the name alone that `\\10.0.0.1` is a server, and the
        // one that handles SMB would rather be told.
        dwDisplayType: DISPLAY_SERVER,
        dwUsage: RESOURCEUSAGE_CONTAINER.0,
        lpRemoteName: windows::core::PWSTR(name.as_mut_ptr()),
        ..Default::default()
    };

    let mut handle = HANDLE::default();
    let rc = unsafe {
        WNetOpenEnumW(
            RESOURCE_GLOBALNET,
            RESOURCETYPE_DISK,
            // Zero, not RESOURCEUSAGE_CONTAINER: here the flag is a filter on
            // what comes back, and asking for containers only, of a container
            // named outright, is the combination the SMB provider rejects.
            WNET_OPEN_ENUM_USAGE(0),
            Some(&ask),
            &mut handle,
        )
    };
    if rc != NO_ERROR {
        return Err(std::io::Error::from_raw_os_error(rc.0 as i32));
    }

    // `u64` rather than `u8`: the buffer is read back as `NETRESOURCEW`, whose
    // pointer fields need pointer alignment, which a byte vector does not
    // promise. 16 KiB holds a few hundred shares, and a server with more says
    // so rather than being cut off.
    let mut buf: Vec<u64> = vec![0; 2048];
    let mut out = Vec::new();
    let result = loop {
        let mut count = u32::MAX; // as many as fit
        let mut bytes = (buf.len() * 8) as u32;
        let rc = unsafe {
            WNetEnumResourceW(
                handle,
                &mut count,
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                &mut bytes,
            )
        };
        match rc {
            // A success that produced nothing would otherwise be asked again
            // for ever; the documented end is ERROR_NO_MORE_ITEMS, but a
            // provider that says nothing is also saying it is done.
            NO_ERROR if count == 0 => break Ok(()),
            NO_ERROR => {
                // SAFETY: on success the call has written `count` entries at
                // the front of the buffer, and the buffer outlives the slice.
                let found =
                    unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const NETRESOURCEW, count as usize) };
                for r in found {
                    // SAFETY: the provider's strings live in the same buffer,
                    // and are read before the next call overwrites it.
                    let remote = unsafe { r.lpRemoteName.to_string() };
                    let Ok(remote) = remote else { continue };
                    if remote.is_empty() {
                        continue;
                    }
                    let path = crate::util::normalize(Path::new(&remote));
                    let name = crate::util::file_name(&path);
                    out.push(Entry::directory(path, name));
                }
            }
            ERROR_NO_MORE_ITEMS => break Ok(()),
            // Not even one entry fitted; `bytes` now says what would do.
            ERROR_MORE_DATA => buf.resize((bytes as usize).div_ceil(8).max(buf.len() * 2), 0),
            e => break Err(std::io::Error::from_raw_os_error(e.0 as i32)),
        }
    };
    // Nothing to do about a close that fails, and the shares are already read.
    let _ = unsafe { WNetCloseEnum(handle) };
    result.map(|()| out)
}

/// Nowhere else has host-only UNC paths, so nothing asks for this off Windows;
/// it exists so that the call site does not have to be written twice.
#[cfg(not(windows))]
pub fn list(_host: &Path) -> std::io::Result<Vec<Entry>> {
    Err(std::io::Error::other("network shares are a Windows notion"))
}
