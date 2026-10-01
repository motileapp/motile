//! Messages on a stream: a four-byte big-endian length, then JSON.

use std::io;

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_FRAME: u32 = 64 * 1024 * 1024;

pub async fn write_frame<W: AsyncWrite + Unpin, T: Serialize>(writer: &mut W, message: &T) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    let length = u32::try_from(body.len()).ok().filter(|length| *length <= MAX_FRAME);
    let Some(length) = length else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too large"));
    };
    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(&body).await
}

/// `None` when the stream ended cleanly between messages.
pub async fn read_frame<R: AsyncRead + Unpin, T: DeserializeOwned>(reader: &mut R) -> io::Result<Option<T>> {
    let mut header = [0u8; 4];
    match reader.read_exact(&mut header).await {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_be_bytes(header);
    if length > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too large"));
    }
    let mut body = vec![0u8; length as usize];
    reader.read_exact(&mut body).await?;
    Ok(Some(serde_json::from_slice(&body)?))
}
