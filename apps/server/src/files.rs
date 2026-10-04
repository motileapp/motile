//! The server's folders, for choosing where a chat works, the files in the folders threads work
//! in, and files the client sends as attachments.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use motile_protocol::wire::{FileEntry, FileKind, Message};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::agents::environment::Environment;
use crate::git;

use crate::media::POSTER;

const MAX_UPLOAD: u64 = 500 * 1024 * 1024;
/// A text is sent up to here; nobody reads more of it in a client.
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

/// Saves the next `size` bytes of the stream and returns where the file is. A poster is saved in
/// the folder of the video it stands for.
pub async fn receive_upload<R: AsyncRead + Unpin>(
    reader: &mut R,
    attachments: &Path,
    name: &str,
    size: u64,
    poster_of: Option<&str>,
) -> anyhow::Result<String> {
    if size > MAX_UPLOAD {
        bail!("Attachments can be at most 500 MB.");
    }
    let name =
        Path::new(name).file_name().and_then(|name| name.to_str()).context("The attachment needs a file name.")?;
    let (folder, path) = match poster_of {
        Some(video) => {
            let folder = upload_folder(attachments, video).context("That video isn't on your server.")?;
            (folder.clone(), folder.join(POSTER))
        }
        None => {
            let folder = attachments.join(uuid::Uuid::new_v4().simple().to_string());
            (folder.clone(), folder.join(name))
        }
    };
    tokio::fs::create_dir_all(&folder).await?;

    let mut file = tokio::fs::File::create(&path).await?;
    let copied = tokio::io::copy(&mut reader.take(size), &mut file).await?;
    file.flush().await?;
    if copied != size {
        let cut_off = if poster_of.is_some() {
            tokio::fs::remove_file(&path).await
        } else {
            tokio::fs::remove_dir_all(&folder).await
        };
        let _ = cut_off;
        bail!("The upload was cut off.");
    }
    Ok(path.to_string_lossy().into_owned())
}

/// The folder an uploaded file was given, if `path` is one of this server's uploads.
fn upload_folder(attachments: &Path, path: &str) -> Option<PathBuf> {
    let folder = Path::new(path).parent()?;
    let named_by_server = folder.file_name()?.to_str()?.chars().all(|character| character.is_ascii_hexdigit());
    (named_by_server && folder.parent()? == attachments && folder.is_dir()).then(|| folder.to_path_buf())
}

/// Removes an uploaded file. Files that were attached from elsewhere on the server stay.
pub fn remove_upload(attachments: &Path, path: &str) {
    let Some(folder) = upload_folder(attachments, path) else { return };
    let _ = std::fs::remove_dir_all(folder);
}

/// Removes the uploads that are older than `age` and that no message in `kept` has.
pub fn sweep_uploads(attachments: &Path, kept: &[String], age: Duration) {
    let Ok(folders) = std::fs::read_dir(attachments) else { return };
    let kept: HashSet<&Path> = kept.iter().filter_map(|path| Path::new(path).parent()).collect();
    for folder in folders.flatten() {
        let old = folder
            .metadata()
            .and_then(|details| details.modified())
            .is_ok_and(|modified| modified.elapsed().is_ok_and(|elapsed| elapsed > age));
        if old && !kept.contains(folder.path().as_path()) {
            let _ = std::fs::remove_dir_all(folder.path());
        }
    }
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

    async fn upload(attachments: &Path, name: &str, poster_of: Option<&str>) -> anyhow::Result<String> {
        receive_upload(&mut &b"bytes"[..], attachments, name, 5, poster_of).await
    }

    #[tokio::test]
    async fn a_poster_goes_next_to_its_video_and_nowhere_else() {
        let dir = tempfile::tempdir().unwrap();
        let attachments = dir.path().join("attachments");
        let video = upload(&attachments, "demo.mov", None).await.unwrap();

        let poster = upload(&attachments, "poster.jpg", Some(&video)).await.unwrap();
        assert_eq!(Path::new(&poster), Path::new(&video).with_file_name(POSTER));

        let outside = dir.path().join("elsewhere/demo.mov");
        std::fs::create_dir_all(outside.parent().unwrap()).unwrap();
        assert!(upload(&attachments, "poster.jpg", outside.to_str()).await.is_err());
        assert!(!outside.with_file_name(POSTER).exists());
    }

    #[tokio::test]
    async fn only_uploads_are_removed_and_only_old_unsent_ones_are_swept() {
        let dir = tempfile::tempdir().unwrap();
        let attachments = dir.path().join("attachments");
        let sent = upload(&attachments, "sent.png", None).await.unwrap();
        let unsent = upload(&attachments, "unsent.png", None).await.unwrap();
        let own = dir.path().join("project/own.png");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, b"bytes").unwrap();

        sweep_uploads(&attachments, &[], Duration::from_secs(3600));
        assert!(Path::new(&unsent).exists(), "a new upload may still be sent");

        sweep_uploads(&attachments, std::slice::from_ref(&sent), Duration::ZERO);
        assert!(Path::new(&sent).exists() && !Path::new(&unsent).exists());

        remove_upload(&attachments, own.to_str().unwrap());
        remove_upload(&attachments, &sent);
        assert!(own.exists() && !Path::new(&sent).exists());
    }
}
