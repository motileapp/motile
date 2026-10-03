//! The images and videos this device has fetched from its servers. The servers keep them all; this
//! copy only makes them show at once, and without a connection. When it grows past its limit,
//! what was looked at longest ago goes first.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Two gigabytes, counted the way the Mac counts a file's size.
pub const LIMIT: u64 = 2_000_000_000;
const UNFINISHED: &str = "part";

pub struct MediaCache {
    folder: PathBuf,
}

impl MediaCache {
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }

    /// The file if this device has it, marked as looked at now.
    pub fn get(&self, id: &str) -> Option<PathBuf> {
        let file = self.file(id)?;
        let opened = std::fs::File::options().write(true).open(&file).ok()?;
        let _ = opened.set_modified(SystemTime::now());
        Some(file)
    }

    /// Where the file is kept. `None` for a name that isn't a plain file's.
    pub fn file(&self, id: &str) -> Option<PathBuf> {
        let plain =
            !id.starts_with('.') && id.chars().all(|character| character.is_ascii_alphanumeric() || character == '.');
        (plain && !id.is_empty()).then(|| self.folder.join(id))
    }

    /// Where a download is written until all of it has arrived.
    pub fn unfinished(&self, id: &str) -> Option<PathBuf> {
        std::fs::create_dir_all(&self.folder).ok()?;
        Some(self.file(id)?.with_added_extension(UNFINISHED))
    }

    /// Keeps a copy of a file this device sent to a server, under the name the server gives it.
    pub fn keep(&self, id: &str, file: &Path) {
        let Some(unfinished) = self.unfinished(id) else { return };
        if self.file(id).is_some_and(|kept| kept.exists()) {
            return;
        }
        if std::fs::copy(file, &unfinished).is_err() || finish(&unfinished).is_err() {
            let _ = std::fs::remove_file(&unfinished);
        }
    }

    pub fn size(&self) -> u64 {
        self.files().iter().map(|(_, size, _)| size).sum()
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }

    /// Removes the files looked at longest ago until the rest fit in `limit`.
    pub fn trim(&self, limit: u64) {
        let mut files = self.files();
        files.sort_by_key(|(_, _, seen)| *seen);
        let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
        for (file, size, _) in files {
            if total <= limit {
                return;
            }
            if std::fs::remove_file(&file).is_ok() {
                total -= size;
            }
        }
    }

    fn files(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        let Ok(entries) = std::fs::read_dir(&self.folder) else { return Vec::new() };
        let files = entries.flatten().filter_map(|entry| {
            let details = entry.metadata().ok().filter(|details| details.is_file())?;
            let path = entry.path();
            let finished = path.extension().is_none_or(|extension| extension != UNFINISHED);
            finished.then(|| (path, details.len(), details.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
        });
        files.collect()
    }
}

/// Puts a finished download in its place.
pub fn finish(unfinished: &Path) -> std::io::Result<PathBuf> {
    let file = unfinished.with_extension("");
    std::fs::rename(unfinished, &file)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn put(cache: &MediaCache, id: &str, bytes: usize, seen_ago: u64) {
        let unfinished = cache.unfinished(id).unwrap();
        std::fs::write(&unfinished, vec![0u8; bytes]).unwrap();
        let file = finish(&unfinished).unwrap();
        let seen = SystemTime::now() - Duration::from_secs(seen_ago);
        std::fs::File::options().write(true).open(file).unwrap().set_modified(seen).unwrap();
    }

    #[test]
    fn what_was_looked_at_longest_ago_goes_first() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MediaCache::new(dir.path().join("media"));
        put(&cache, "old.png", 400, 300);
        put(&cache, "older.mp4", 400, 600);
        put(&cache, "new.png", 400, 10);
        assert_eq!(cache.size(), 1200);

        // Looking at the oldest makes it the newest.
        assert!(cache.get("older.mp4").is_some());
        cache.trim(900);

        assert_eq!(cache.size(), 800);
        assert!(cache.get("old.png").is_none());
        assert!(cache.get("older.mp4").is_some() && cache.get("new.png").is_some());
    }

    #[test]
    fn a_download_counts_once_it_has_finished_and_clearing_removes_everything() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MediaCache::new(dir.path().join("media"));
        let unfinished = cache.unfinished("shot.png").unwrap();
        std::fs::write(&unfinished, [0u8; 100]).unwrap();
        assert_eq!(cache.size(), 0);
        assert!(cache.get("shot.png").is_none());

        assert_eq!(finish(&unfinished).unwrap(), cache.file("shot.png").unwrap());
        assert_eq!(cache.size(), 100);

        let sent = dir.path().join("sent.png");
        std::fs::write(&sent, [0u8; 40]).unwrap();
        cache.keep("sent.png", &sent);
        assert_eq!(cache.size(), 140);

        cache.clear();
        assert_eq!(cache.size(), 0);
        assert!(cache.file("../cache.sqlite").is_none());
    }
}
