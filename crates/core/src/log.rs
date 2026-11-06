//! The client's log: what the core did, written to stderr and to `client.log` in the client's data
//! folder, so that a problem can be looked into after the fact. The file starts over once it is
//! `LIMIT` long, and the one before is kept as `client.old.log`.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const LIMIT: u64 = 5 * 1024 * 1024;
const FILTER: &str = "warn,motile_core=info";

/// The log in a client's data folder.
pub fn file(data_dir: &Path) -> PathBuf {
    data_dir.join("client.log")
}

fn old_file(data_dir: &Path) -> PathBuf {
    data_dir.join("client.old.log")
}

/// Logs to stderr and to the data folder's log, and logs a panic too. `MOTILE_LOG` sets what is
/// logged. Only the first call does anything.
pub fn init(data_dir: &Path) {
    if tracing::dispatcher::has_been_set() {
        return;
    }
    let filter = std::env::var("MOTILE_LOG").unwrap_or_else(|_| FILTER.to_string());
    let log = LogFile::open(data_dir);
    let started = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(move || Tee(log))
        .try_init()
        .is_ok();
    if !started {
        return;
    }
    let reported = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("{info}");
        reported(info);
    }));
    tracing::info!("started");
}

struct LogFile {
    data_dir: PathBuf,
    open: Mutex<Option<(File, u64)>>,
}

impl LogFile {
    fn open(data_dir: &Path) -> Option<&'static LogFile> {
        std::fs::create_dir_all(data_dir).ok()?;
        let file = OpenOptions::new().create(true).append(true).open(file(data_dir)).ok()?;
        let length = file.metadata().map_or(0, |metadata| metadata.len());
        let log = LogFile { data_dir: data_dir.to_path_buf(), open: Mutex::new(Some((file, length))) };
        Some(Box::leak(Box::new(log)))
    }

    fn write(&self, line: &[u8]) {
        let mut open = self.open.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if open.as_ref().is_some_and(|(_, length)| *length >= LIMIT) {
            *open = self.start_over();
        }
        let Some((file, length)) = open.as_mut() else { return };
        if file.write_all(line).is_ok() {
            *length += line.len() as u64;
        }
    }

    fn start_over(&self) -> Option<(File, u64)> {
        let _ = std::fs::rename(file(&self.data_dir), old_file(&self.data_dir));
        let file = OpenOptions::new().create(true).write(true).truncate(true).open(file(&self.data_dir)).ok()?;
        Some((file, 0))
    }
}

struct Tee(Option<&'static LogFile>);

impl Write for Tee {
    fn write(&mut self, line: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(line);
        if let Some(log) = self.0 {
            log.write(line);
        }
        Ok(line.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_over_past_the_limit_and_keeps_the_last() {
        let dir = std::env::temp_dir().join(format!("motile-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = LogFile::open(&dir).unwrap();
        let line = vec![b'a'; 1024 * 1024];
        for _ in 0..5 {
            log.write(&line);
        }
        log.write(b"after\n");
        assert_eq!(std::fs::read(file(&dir)).unwrap(), b"after\n");
        assert_eq!(std::fs::metadata(old_file(&dir)).unwrap().len(), 5 * 1024 * 1024);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
