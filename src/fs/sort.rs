use std::cmp::Ordering;

use serde::Deserialize;

use super::entry::Entry;
use crate::util;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    None,
    #[serde(alias = "mtime")]
    Mtime,
    #[serde(alias = "btime")]
    Btime,
    #[serde(alias = "atime")]
    Atime,
    Extension,
    Alphabetical,
    Natural,
    Size,
    Random,
}

impl SortBy {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None,
            "mtime" | "modified" => Self::Mtime,
            "btime" | "created" => Self::Btime,
            "atime" | "accessed" => Self::Atime,
            "extension" | "ext" => Self::Extension,
            "alphabetical" | "alpha" => Self::Alphabetical,
            "natural" => Self::Natural,
            "size" => Self::Size,
            "random" => Self::Random,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Mtime => "mtime",
            Self::Btime => "btime",
            Self::Atime => "atime",
            Self::Extension => "extension",
            Self::Alphabetical => "alphabetical",
            Self::Natural => "natural",
            Self::Size => "size",
            Self::Random => "random",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SortSpec {
    pub by: SortBy,
    pub reverse: bool,
    pub dir_first: bool,
    pub sensitive: bool,
}

impl Default for SortSpec {
    fn default() -> Self {
        Self { by: SortBy::Natural, reverse: false, dir_first: true, sensitive: false }
    }
}

impl SortSpec {
    pub fn apply(&self, entries: &mut [Entry]) {
        let by = self.by;
        let sensitive = self.sensitive;
        let rev = self.reverse;
        let dir_first = self.dir_first;

        if by == SortBy::Random {
            let mut seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E3779B97F4A7C15)
                | 1;
            // Fisher-Yates with a xorshift source; no rand dependency needed.
            for i in (1..entries.len()).rev() {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let j = (seed % (i as u64 + 1)) as usize;
                entries.swap(i, j);
            }
            if dir_first {
                entries.sort_by(|a, b| dir_rank(a).cmp(&dir_rank(b)));
            }
            return;
        }

        entries.sort_by(|a, b| {
            if dir_first {
                match dir_rank(a).cmp(&dir_rank(b)) {
                    Ordering::Equal => {}
                    o => return o,
                }
            }
            let ord = match by {
                SortBy::None => Ordering::Equal,
                SortBy::Mtime => cmp_time(a.modified, b.modified),
                SortBy::Btime => cmp_time(a.created, b.created),
                SortBy::Atime => cmp_time(a.accessed, b.accessed),
                SortBy::Size => a.len.cmp(&b.len),
                SortBy::Extension => match (a.ext.as_deref(), b.ext.as_deref()) {
                    (Some(x), Some(y)) => util::alpha_cmp(x, y, sensitive),
                    (None, Some(_)) => Ordering::Less,
                    (Some(_), None) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                },
                SortBy::Alphabetical => util::alpha_cmp(&a.name, &b.name, sensitive),
                SortBy::Natural | SortBy::Random => {
                    util::natural_cmp(&a.name, &b.name, sensitive)
                }
            };
            let ord = if rev { ord.reverse() } else { ord };
            // Stable tie-break so redraws never shuffle equal keys.
            match ord {
                Ordering::Equal => util::natural_cmp(&a.name, &b.name, sensitive),
                o => o,
            }
        });
    }
}

fn dir_rank(e: &Entry) -> u8 {
    if e.is_dir_like() {
        0
    } else {
        1
    }
}

fn cmp_time(a: Option<std::time::SystemTime>, b: Option<std::time::SystemTime>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.cmp(&y),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
