//! One connection to a server, and how long it took to get.

use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use iroh::endpoint::{ConnectionError, RecvStream, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use motile_protocol::frame::{read_frame, write_frame};
use motile_protocol::identity::DeviceKey;
use motile_protocol::wire::{FileKind, Message, Request};
use motile_protocol::{ALPN, media};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// More than a server sends of any file.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// A server's key, optionally with an address to reach it at directly: `key` or `key@ip:port`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerAddr {
    pub key: String,
    pub direct: Option<SocketAddr>,
}

impl FromStr for ServerAddr {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> anyhow::Result<Self> {
        let (key, direct) = match text.split_once('@') {
            Some((key, addr)) => (key, Some(addr.parse().context("The server's address should be ip:port.")?)),
            None => (text, None),
        };
        if EndpointId::from_str(key).is_err() {
            bail!("{key} isn't a server key.");
        }
        Ok(Self { key: key.to_string(), direct })
    }
}

impl ServerAddr {
    fn endpoint_addr(&self) -> anyhow::Result<EndpointAddr> {
        let addr = EndpointAddr::new(EndpointId::from_str(&self.key)?);
        Ok(match self.direct {
            Some(direct) => addr.with_ip_addr(direct),
            None => addr,
        })
    }
}

/// `local_only` skips relays and address lookup, so servers must be given with an address.
pub async fn bind(key: &DeviceKey, local_only: bool) -> anyhow::Result<Endpoint> {
    let builder = match local_only {
        true => Endpoint::builder(presets::Minimal),
        false => Endpoint::builder(presets::N0),
    };
    let secret_key = SecretKey::from_bytes(&key.to_bytes());
    builder.secret_key(secret_key).bind().await.context("The network endpoint couldn't be opened.")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathKind {
    Relay,
    Direct,
}

/// The code a server closes with when the device isn't allowed on it.
pub const REFUSED: u32 = 403;

pub struct Closed {
    /// The server turned this device away; dialing again won't help until that changes.
    pub refused: bool,
    pub reason: String,
}

#[derive(Clone)]
pub struct Connection {
    inner: iroh::endpoint::Connection,
    /// The connection only works while its endpoint is alive.
    _endpoint: Endpoint,
    pub dialed: Instant,
    pub connect_time: Duration,
}

impl Connection {
    pub async fn dial(endpoint: &Endpoint, server: &ServerAddr) -> anyhow::Result<Self> {
        let dialed = Instant::now();
        let inner =
            endpoint.connect(server.endpoint_addr()?, ALPN).await.map_err(|error| anyhow::anyhow!("{error:#}"))?;
        Ok(Self { inner, _endpoint: endpoint.clone(), dialed, connect_time: dialed.elapsed() })
    }

    /// The path data is sent on right now, and its round-trip time.
    pub fn path(&self) -> Option<(PathKind, Duration)> {
        let paths = self.inner.paths();
        let selected = paths.iter().find(|path| path.is_selected())?;
        let kind = if selected.is_relay() { PathKind::Relay } else { PathKind::Direct };
        Some((kind, selected.rtt()))
    }

    pub fn path_events(&self) -> iroh::endpoint::PathEventStream {
        self.inner.path_events()
    }

    /// Resolves when the connection is gone, with why.
    pub async fn closed(&self) -> Closed {
        match self.inner.closed().await {
            ConnectionError::ApplicationClosed(close) => Closed {
                refused: close.error_code == REFUSED.into(),
                reason: String::from_utf8_lossy(&close.reason).into_owned(),
            },
            other => Closed { refused: false, reason: other.to_string() },
        }
    }

    pub fn close(&self) {
        self.inner.close(0u32.into(), b"bye");
    }

    pub async fn request(&self, request: &Request) -> anyhow::Result<Message> {
        let (mut send, mut recv) = self.inner.open_bi().await?;
        write_frame(&mut send, request).await?;
        send.finish()?;
        read_frame(&mut recv).await?.context("Your server didn't answer. Try again.")
    }

    /// For `Subscribe` and `Open`: every message until the stream is dropped.
    pub async fn follow(&self, request: &Request) -> anyhow::Result<Follow> {
        let (mut send, recv) = self.inner.open_bi().await?;
        write_frame(&mut send, request).await?;
        send.finish()?;
        Ok(Follow { recv })
    }

    /// Sends a file to the server, telling `progress` how many of its bytes have gone. Returns
    /// its path there, and the name the server gives its contents if it is an image or a video.
    pub async fn upload(
        &self,
        path: &Path,
        poster_of: Option<String>,
        mut progress: impl FnMut(u64, u64),
    ) -> anyhow::Result<(String, Option<String>)> {
        let name = path.file_name().and_then(|name| name.to_str()).context("The attachment needs a file name.")?;
        let mut file =
            tokio::fs::File::open(path).await.with_context(|| format!("{} can't be read.", path.display()))?;
        let size = file.metadata().await?.len();
        // The server keeps every poster as a JPEG.
        let poster = poster_of.as_ref().map(|_| ("jpg".to_string(), false));
        let shown = poster.or_else(|| media::kind(path)).filter(|_| size > 0 && size <= media::MAX_SIZE);

        let (mut send, mut recv) = self.inner.open_bi().await?;
        write_frame(&mut send, &Request::Upload { name: name.to_string(), size, poster_of }).await?;
        let mut namer = media::Namer::default();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut sent = 0;
        while sent < size {
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                bail!("{} changed while it was sent.", path.display());
            }
            send.write_all(&buffer[..read]).await?;
            if shown.is_some() {
                namer.update(&buffer[..read]);
            }
            sent += read as u64;
            progress(sent, size);
        }
        send.finish()?;
        let media_id = shown.map(|(extension, _)| namer.id(&extension));
        match read_frame(&mut recv).await?.context("Your server didn't answer. Try again.")? {
            Message::Uploaded { path } => Ok((path, media_id)),
            Message::Error { message } => bail!("{message}"),
            other => bail!("Unexpected answer to an upload: {other:?}"),
        }
    }

    /// A file in a folder threads work in: what kind it is, its size, and the bytes the server
    /// sends of a text. Those of an image or a video are written to `shown`.
    pub async fn file(&self, request: &Request, shown: &Path) -> anyhow::Result<(FileKind, u64, Vec<u8>)> {
        let (mut send, mut recv) = self.inner.open_bi().await?;
        write_frame(&mut send, request).await?;
        send.finish()?;
        let (kind, size, sent) = match read_frame(&mut recv).await?.context("Your server didn't answer. Try again.")? {
            Message::File { kind, size, sent } => (kind, size, sent),
            Message::Error { message } => bail!("{message}"),
            other => bail!("Unexpected answer to a file request: {other:?}"),
        };
        if matches!(kind, FileKind::Image | FileKind::Video) {
            receive(&mut recv, shown, sent, |_, _| {}).await?;
            return Ok((kind, size, Vec::new()));
        }
        if sent > MAX_FILE_BYTES {
            bail!("The file is too large to show.");
        }
        let mut bytes = vec![0; sent as usize];
        recv.read_exact(&mut bytes).await.context("The file was cut off.")?;
        Ok((kind, size, bytes))
    }

    /// Fetches the server's copy of an image, a video or a file an agent sent into `file`, telling
    /// `progress` how many of its bytes have arrived.
    pub async fn media(&self, id: &str, file: &Path, progress: impl FnMut(u64, u64)) -> anyhow::Result<()> {
        let (mut send, mut recv) = self.inner.open_bi().await?;
        write_frame(&mut send, &Request::Media { id: id.to_string() }).await?;
        send.finish()?;
        let size = match read_frame(&mut recv).await?.context("Your server didn't answer. Try again.")? {
            Message::Media { size } => size,
            Message::Error { message } => bail!("{message}"),
            other => bail!("Unexpected answer to a media request: {other:?}"),
        };
        receive(&mut recv, file, size, progress).await
    }
}

/// Writes the next `size` bytes of the stream to `file`, telling `progress` how many have arrived.
async fn receive(
    recv: &mut RecvStream,
    file: &Path,
    size: u64,
    mut progress: impl FnMut(u64, u64),
) -> anyhow::Result<()> {
    let mut output = tokio::fs::File::create(file).await?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut received = 0;
    while received < size {
        let wanted = buffer.len().min((size - received) as usize);
        let read = recv.read(&mut buffer[..wanted]).await?.context("The download was cut off. Try again.")?;
        output.write_all(&buffer[..read]).await?;
        received += read as u64;
        progress(received, size);
    }
    output.flush().await?;
    Ok(())
}

pub struct Follow {
    recv: RecvStream,
}

impl Follow {
    /// `None` when the server ended the stream.
    pub async fn next(&mut self) -> anyhow::Result<Option<Message>> {
        Ok(read_frame(&mut self.recv).await?)
    }
}
