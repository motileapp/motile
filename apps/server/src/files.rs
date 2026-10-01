//! The host's folders, for choosing where a chat works, and files the app sends as attachments.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use motile_protocol::wire::Message;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

const MAX_UPLOAD: u64 = 500 * 1024 * 1024;

pub fn list_dir(path: Option<&str>, home: &str) -> anyhow::Result<Message> {
    let path = PathBuf::from(path.filter(|path| !path.is_empty()).unwrap_or(home));
    if !path.is_absolute() {
        bail!("{} isn't an absolute path.", path.display());
    }
    let entries = std::fs::read_dir(&path).with_context(|| format!("{} can't be opened.", path.display()))?;
    let mut folders: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    folders.sort_by_key(|name| name.to_lowercase());
    Ok(Message::Dir {
        path: path.to_string_lossy().into_owned(),
        parent: path.parent().map(|parent| parent.to_string_lossy().into_owned()),
        folders,
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
