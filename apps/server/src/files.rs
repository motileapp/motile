//! The server's folders, for choosing where a chat works, the files in the folders threads work
//! in, and files the app sends as attachments.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, bail};
use motile_protocol::wire::{FileEntry, FileKind, Message};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::agents::environment::Environment;
use crate::git;

const MAX_UPLOAD: u64 = 500 * 1024 * 1024;
/// A text is sent up to here; nobody reads more of it in an app.
const MAX_TEXT: u64 = 1024 * 1024;
const MAX_IMAGE: u64 = 20 * 1024 * 1024;
const IMAGES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "tiff", "ico"];
/// How much of a file is looked at to tell text from other data.
const SNIFFED: usize = 8000;

/// Where `path` is inside `root`. One that leads out of it, also through a link, is refused,
/// and so is git's own folder.
fn inside(root: &str, path: &str) -> anyhow::Result<PathBuf> {
    let plain = Path::new(path).components().all(|part| matches!(part, Component::Normal(name) if name != ".git"));
    if !plain {
        bail!("{path} isn't a path inside the folder.");
    }
    let root = Path::new(root).canonicalize().with_context(|| format!("{root} can't be opened."))?;
    let found = root.join(path).canonicalize().with_context(|| format!("{path} can't be opened."))?;
    if !found.starts_with(&root) {
        bail!("{path} leads out of the folder.");
    }
    Ok(found)
}

/// What is in the folder at `path` inside `root`: folders first, each kind by name.
pub async fn list_files(root: &str, path: &str, environment: &Environment) -> anyhow::Result<Message> {
    let folder = inside(root, path)?;
    let listed = std::fs::read_dir(&folder).with_context(|| format!("{path} can't be opened."))?;
    let mut entries = Vec::new();
    for entry in listed.filter_map(Result::ok) {
        let Ok(name) = entry.file_name().into_string() else { continue };
        if name == ".git" {
            continue;
        }
        entries.push(FileEntry { folder: entry.path().is_dir(), name, ignored: false });
    }
    entries.sort_by_key(|entry| (!entry.folder, entry.name.to_lowercase()));
    let from_root = |entry: &FileEntry| Path::new(path).join(&entry.name).to_string_lossy().into_owned();
    let ignored = git::ignored(root, environment, &entries.iter().map(from_root).collect::<Vec<_>>()).await;
    for entry in &mut entries {
        entry.ignored = ignored.contains(&from_root(entry));
    }
    Ok(Message::Files { entries })
}

/// The file at `path` inside `root`, what kind it is, its size and how many of its bytes to send.
pub async fn open_file(root: &str, path: &str) -> anyhow::Result<(tokio::fs::File, FileKind, u64, u64)> {
    let found = inside(root, path)?;
    if !found.is_file() {
        bail!("{path} isn't a file.");
    }
    let mut file = tokio::fs::File::open(&found).await.with_context(|| format!("{path} can't be opened."))?;
    let size = file.metadata().await?.len();
    let extension = found.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase();
    if IMAGES.contains(&extension.as_str()) {
        let kind = if size <= MAX_IMAGE { FileKind::Image } else { FileKind::Binary };
        return Ok((file, kind, size, if kind == FileKind::Image { size } else { 0 }));
    }
    let mut start = vec![0; SNIFFED.min(size as usize)];
    file.read_exact(&mut start).await?;
    file.rewind().await?;
    if start.contains(&0) {
        return Ok((file, FileKind::Binary, size, 0));
    }
    Ok((file, FileKind::Text, size, size.min(MAX_TEXT)))
}

pub fn list_dir(path: Option<&str>, home: &str, icons: bool, hidden: bool) -> anyhow::Result<Message> {
    let path = PathBuf::from(path.filter(|path| !path.is_empty()).unwrap_or(home));
    if !path.is_absolute() {
        bail!("{} isn't an absolute path.", path.display());
    }
    let entries = std::fs::read_dir(&path).with_context(|| format!("{} can't be opened.", path.display()))?;
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let Ok(name) = entry.file_name().into_string() else { continue };
        if name.starts_with('.') && !hidden {
            continue;
        }
        if entry.path().is_dir() {
            folders.push(name);
        } else if icons && crate::icons::is_icon(&entry.path()) {
            files.push(name);
        }
    }
    folders.sort_by_key(|name| name.to_lowercase());
    files.sort_by_key(|name| name.to_lowercase());
    Ok(Message::Dir {
        path: path.to_string_lossy().into_owned(),
        parent: path.parent().map(|parent| parent.to_string_lossy().into_owned()),
        folders,
        files,
    })
}

/// Saves the next `size` bytes of the stream and returns where the file is.
pub async fn receive_upload<R: AsyncRead + Unpin>(
    reader: &mut R,
    attachments: &Path,
    name: &str,
    size: u64,
) -> anyhow::Result<String> {
    if size > MAX_UPLOAD {
        bail!("Attachments can be at most 500 MB.");
    }
    let name =
        Path::new(name).file_name().and_then(|name| name.to_str()).context("The attachment needs a file name.")?;
    let folder = attachments.join(uuid::Uuid::new_v4().simple().to_string());
    tokio::fs::create_dir_all(&folder).await?;
    let path = folder.join(name);

    let mut file = tokio::fs::File::create(&path).await?;
    let copied = tokio::io::copy(&mut reader.take(size), &mut file).await?;
    file.flush().await?;
    if copied != size {
        let _ = tokio::fs::remove_dir_all(&folder).await;
        bail!("The upload was cut off.");
    }
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_stays_inside_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("secret"), "token").unwrap();
        std::os::unix::fs::symlink(dir.path().join("secret"), root.join("link")).unwrap();
        let root = root.to_string_lossy().into_owned();

        assert!(inside(&root, "src/main.rs").unwrap().ends_with("project/src/main.rs"));
        assert!(inside(&root, "").unwrap().ends_with("project"));
        for refused in ["../secret", "src/../../secret", "/etc/hosts", "link", ".git", ".git/config", "missing"] {
            assert!(inside(&root, refused).is_err(), "{refused}");
        }
    }
}
