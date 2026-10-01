//! Putting back what a [`OpKind::Trash`](super::ops::OpKind::Trash) job took
//! away, so that `u` can undo a delete.
//!
//! `trash::os_limited` can read the trash back on Windows and on
//! Freedesktop-compliant unixes. macOS has no API for it — its Trash is only
//! readable through the Finder — so there [`SUPPORTED`] is false and `u` says
//! so instead of pretending.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use trash::TrashItem;

/// Whether the trash can be read back on this platform. Mirrors the `cfg` that
/// gates `trash::os_limited`.
pub const SUPPORTED: bool = cfg!(any(
    windows,
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
));

/// What to tell the person where [`SUPPORTED`] is false.
pub const UNSUPPORTED: &str = "restoring from the trash is not supported on this platform";

/// The trashed items that answer to the paths asked for, by [`key`].
pub type Found = HashMap<String, TrashItem>;

/// Read the trash once and keep only what could put `paths` back. The whole
/// trash is walked, so this belongs on a worker thread.
pub fn found(paths: &[PathBuf]) -> Result<Found, String> {
    Ok(newest_per_path(list_trash()?, paths))
}

/// Put one path back where it was. `found` comes from [`found`].
pub fn put_back(found: &Found, path: &Path) -> Result<(), String> {
    let name = crate::util::file_name(path);
    let Some(item) = found.get(&key(path)) else {
        return Err(format!("{name}: not in the trash"));
    };
    // One item per call: a path blocked by something that took its place must
    // not hold back the rest, and the error has to be able to name it.
    restore_one(item.clone()).map_err(|e| format!("{name}: {e}"))
}

/// The newest trashed item for each of `wanted`. Deleting the same name twice
/// leaves two items behind, and the one to put back is the one that was there
/// a moment ago.
fn newest_per_path(items: Vec<TrashItem>, wanted: &[PathBuf]) -> Found {
    // The trash can hold thousands of items and every one of them is looked up
    // here, so the paths asked for go in a set rather than a list.
    let keys: HashSet<String> = wanted.iter().map(|p| key(p)).collect();
    let mut out: Found = HashMap::new();
    for item in items {
        let k = key(&item.original_path());
        if !keys.contains(&k) {
            continue;
        }
        match out.get(&k) {
            Some(old) if old.time_deleted >= item.time_deleted => {}
            _ => {
                out.insert(k, item);
            }
        }
    }
    out
}

/// How a path is matched against the trash's record of where it came from.
/// Windows reports the original parent as the shell knows it, which can differ
/// in case from the path the listing built.
fn key(p: &Path) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.into_owned()
    }
}

#[cfg(any(
    windows,
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
fn list_trash() -> Result<Vec<TrashItem>, String> {
    trash::os_limited::list().map_err(|e| format!("trash: {e}"))
}

#[cfg(not(any(
    windows,
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
fn list_trash() -> Result<Vec<TrashItem>, String> {
    Err(UNSUPPORTED.into())
}

#[cfg(any(
    windows,
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
fn restore_one(item: TrashItem) -> Result<(), String> {
    trash::os_limited::restore_all([item]).map_err(|e| say(&e))
}

/// The trash crate's error, in words. `RestoreCollision`'s own rendering is
/// its Debug form, recycle-bin ids and all; what it means is plain, and the
/// step is kept, so a second `u` works once the way is clear (#83).
// Only restoring calls it, and macOS has no restoring to call it from.
#[cfg_attr(any(target_os = "macos", target_os = "ios", target_os = "android"), allow(dead_code))]
fn say(e: &trash::Error) -> String {
    match e {
        trash::Error::RestoreCollision { .. } => {
            "a file by that name is already there. Move it away and press u again".to_owned()
        }
        other => other.to_string(),
    }
}

#[cfg(not(any(
    windows,
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
fn restore_one(_item: TrashItem) -> Result<(), String> {
    Err(UNSUPPORTED.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(parent: &str, name: &str, time_deleted: i64) -> TrashItem {
        TrashItem {
            id: format!("{parent}/{name}/{time_deleted}").into(),
            name: name.into(),
            original_parent: PathBuf::from(parent),
            time_deleted,
        }
    }

    /// #83: a collision reads as what to do, not as the bin's internal ids.
    #[test]
    fn a_collision_says_what_to_do() {
        let e = trash::Error::RestoreCollision { path: PathBuf::from("x"), remaining_items: Vec::new() };
        assert_eq!(say(&e), "a file by that name is already there. Move it away and press u again");
    }

    #[test]
    fn keeps_the_newest_of_two_by_the_same_name() {
        let dir = if cfg!(windows) { r"C:\work" } else { "/work" };
        let items = vec![item(dir, "a.txt", 100), item(dir, "a.txt", 200), item(dir, "b.txt", 50)];
        let wanted = vec![PathBuf::from(dir).join("a.txt")];

        let found = newest_per_path(items, &wanted);

        assert_eq!(found.len(), 1, "b.txt was not asked for");
        assert_eq!(found[&key(&wanted[0])].time_deleted, 200);
    }

    #[test]
    fn a_path_that_is_not_in_the_trash_is_simply_absent() {
        let dir = if cfg!(windows) { r"C:\work" } else { "/work" };
        let wanted = vec![PathBuf::from(dir).join("gone.txt")];

        let found = newest_per_path(vec![item(dir, "other.txt", 1)], &wanted);

        assert!(found.is_empty());
        let err = put_back(&found, &wanted[0]).unwrap_err();
        assert!(err.contains("not in the trash"), "{err}");
    }

    /// The recycle bin can name the drive or a directory in a different case
    /// than the listing did, and it is still the same file.
    #[cfg(windows)]
    #[test]
    fn windows_matches_regardless_of_case() {
        let items = vec![item(r"C:\Work", "A.txt", 1)];
        let wanted = vec![PathBuf::from(r"c:\work\a.txt")];

        assert_eq!(newest_per_path(items, &wanted).len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn unix_case_is_part_of_the_name() {
        let items = vec![item("/work", "A.txt", 1)];
        let wanted = vec![PathBuf::from("/work/a.txt")];

        assert!(newest_per_path(items, &wanted).is_empty());
    }
}
