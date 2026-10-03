//! The server's folders, for choosing where a chat works, and files the app sends as attachments.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use motile_protocol::wire::Message;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use crate::media::POSTER;

const MAX_UPLOAD: u64 = 500 * 1024 * 1024;

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
            let folder = upload_folder(attachments, video).context("That video isn't on the server.")?;
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
