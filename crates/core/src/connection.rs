//! One connection to a server, and how long it took to get.

use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow, bail};
use iroh::endpoint::{ConnectionError, IdleTimeout, QuicTransportConfig, RecvStream, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey, TransportAddr};
use motile_protocol::frame::{read_frame, write_frame};
use motile_protocol::identity::DeviceKey;
use motile_protocol::wire::{FileKind, Message, Request};
use motile_protocol::{ALPN, media};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// More than a server sends of any file.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// A connection that hears nothing for this long is given up. Both sides keep it alive every 5s,
/// and QUIC's default of 30s left a dead connection looking connected for half a minute.
const IDLE_TIMEOUT: Duration = Duration::from_secs(15);

const CONNECTION_LOST: &str = "The connection to your server was lost";
pub const NOT_CONNECTED: &str = "Your server isn't connected. Try again when it is back.";

/// Whether an error says the server couldn't be reached, rather than that it refused.
pub fn offline(error: &str) -> bool {
    error.contains(CONNECTION_LOST) || error.ends_with(NOT_CONNECTED)
}

/// The connection failing, with what failed, so that it can be told apart.
pub fn lost(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{CONNECTION_LOST} ({error}). Try again when it is back.")
}

/// The connection failing, worded for a person. A malformed message keeps its own error.
fn lost_on_the_way(error: io::Error) -> anyhow::Error {
    if error.kind() == io::ErrorKind::InvalidData {
        return error.into();
    }
    lost(error)
}

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
    let idle = IdleTimeout::try_from(IDLE_TIMEOUT)?;
    let transport = QuicTransportConfig::builder().max_idle_timeout(Some(idle)).build();
    builder
        .secret_key(secret_key)
        .transport_config(transport)
        .bind()
        .await
        .context("The network endpoint couldn't be opened.")
}

/// Has an endpoint that is being replaced stop using the relays at once, instead of once its
/// connections have closed. `relay` is the account's relay the endpoint follows.
pub async fn leave_relays(endpoint: &Endpoint, relay: Option<&str>) {
    let relays: Vec<RelayUrl> = motile_protocol::relay::relay_map(relay).urls();
    for relay in &relays {
        endpoint.remove_relay(relay).await;
    }
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
        Self::dial_on(endpoint, server, &[], ALPN).await
    }

    /// Dials the device speaking `alpn`, trying the addresses it was last `known` at too, so that a
    /// new endpoint doesn't have to look them up first.
    pub async fn dial_on(
        endpoint: &Endpoint,
        server: &ServerAddr,
        known: &[TransportAddr],
        alpn: &[u8],
    ) -> anyhow::Result<Self> {
        let dialed = Instant::now();
        let address = server.endpoint_addr()?.with_addrs(known.iter().cloned());
        let inner = endpoint.connect(address, alpn).await.map_err(|error| anyhow::anyhow!("{error:#}"))?;
        Ok(Self { inner, _endpoint: endpoint.clone(), dialed, connect_time: dialed.elapsed() })
    }

    /// The path data is sent on right now, and its round-trip time.
    pub fn path(&self) -> Option<(PathKind, Duration)> {
        let paths = self.inner.paths();
        let selected = paths.iter().find(|path| path.is_selected())?;
        let kind = if selected.is_relay() { PathKind::Relay } else { PathKind::Direct };
        Some((kind, selected.rtt()))
    }

    /// The addresses the server is reached at on this connection: its relay, and its own.
    pub fn addresses(&self) -> Vec<TransportAddr> {
        self.inner.paths().iter().map(|path| path.remote_addr().clone()).collect()
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

    pub fn is_closed(&self) -> bool {
        self.inner.close_reason().is_some()
    }

    pub fn close(&self) {
        self.inner.close(0u32.into(), b"bye");
    }

    pub async fn request(&self, request: &Request) -> anyhow::Result<Message> {
        self.ask(request).await?.context("Your server didn't answer. Try again.")
    }

    /// Whether the connection still carries a request there and back within `within`. A server
    /// from before `Ping` ends the stream without an answer, which counts.
    pub async fn ping(&self, within: Duration) -> Result<(), String> {
        match tokio::time::timeout(within, self.ask::<_, Message>(&Request::Ping)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => Err(format!("a check failed: {error:#}")),
            Err(_) => Err(format!("a check got no answer within {}s", within.as_secs())),
        }
    }

    /// Whether `other` is this same connection.
    pub fn is(&self, other: &Connection) -> bool {
        self.inner.stable_id() == other.inner.stable_id()
    }

    /// Sends one message on a stream of its own and reads the one that answers it. `None` when the
    /// stream ended without one.
    pub async fn ask<Q: Serialize, A: DeserializeOwned>(&self, request: &Q) -> anyhow::Result<Option<A>> {
        let (mut send, mut recv) = self.inner.open_bi().await.map_err(lost)?;
        write_frame(&mut send, request).await.map_err(lost_on_the_way)?;
        send.finish().map_err(lost)?;
        read_frame(&mut recv).await.map_err(lost_on_the_way)
    }

    /// For `Subscribe` and `Open`: every message until the stream is dropped.
    pub async fn follow(&self, request: &Request) -> anyhow::Result<Follow> {
        let (mut send, recv) = self.inner.open_bi().await.map_err(lost)?;
        write_frame(&mut send, request).await.map_err(lost_on_the_way)?;
        send.finish().map_err(lost)?;
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

        let (mut send, mut recv) = self.inner.open_bi().await.map_err(lost)?;
        write_frame(&mut send, &Request::Upload { name: name.to_string(), size, poster_of })
            .await
            .map_err(lost_on_the_way)?;
        let mut namer = media::Namer::default();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut sent = 0;
        while sent < size {
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                bail!("{} changed while it was sent.", path.display());
            }
            send.write_all(&buffer[..read]).await.map_err(lost)?;
            if shown.is_some() {
                namer.update(&buffer[..read]);
            }
            sent += read as u64;
            progress(sent, size);
        }
        send.finish().map_err(lost)?;
        let media_id = shown.map(|(extension, _)| namer.id(&extension));
        match read_frame(&mut recv).await.map_err(lost_on_the_way)?.context("Your server didn't answer. Try again.")? {
            Message::Uploaded { path } => Ok((path, media_id)),
            Message::Error { message } => bail!("{message}"),
            other => bail!("Unexpected answer to an upload: {other:?}"),
        }
    }

    /// A file in a folder threads work in: what kind it is, its size, and the bytes the server
    /// sends of a text. Those of an image or a video are written to `shown`.
    pub async fn file(&self, request: &Request, shown: &Path) -> anyhow::Result<(FileKind, u64, Vec<u8>)> {
        let (mut send, mut recv) = self.inner.open_bi().await.map_err(lost)?;
        write_frame(&mut send, request).await.map_err(lost_on_the_way)?;
        send.finish().map_err(lost)?;
        let (kind, size, sent) = match read_frame(&mut recv)
            .await
            .map_err(lost_on_the_way)?
            .context("Your server didn't answer. Try again.")?
        {
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
        let (mut send, mut recv) = self.inner.open_bi().await.map_err(lost)?;
        write_frame(&mut send, &Request::Media { id: id.to_string() }).await.map_err(lost_on_the_way)?;
        send.finish().map_err(lost)?;
        let size = match read_frame(&mut recv)
            .await
            .map_err(lost_on_the_way)?
            .context("Your server didn't answer. Try again.")?
        {
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
        let read =
            recv.read(&mut buffer[..wanted]).await.map_err(lost)?.context("The download was cut off. Try again.")?;
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
        read_frame(&mut self.recv).await.map_err(lost_on_the_way)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server on a runtime of its own, so that stopping the runtime silences it without a word.
    fn server_that_can_vanish() -> (tokio::runtime::Runtime, ServerAddr) {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let address = runtime.block_on(async {
            let endpoint = Endpoint::builder(presets::Minimal).alpns(vec![ALPN.to_vec()]).bind().await.unwrap();
            let accepting = endpoint.clone();
            tokio::spawn(async move {
                while let Some(incoming) = accepting.accept().await {
                    let Ok(connection) = incoming.await else { continue };
                    tokio::spawn(async move { connection.closed().await });
                }
            });
            let port = endpoint.bound_sockets().iter().find(|address| address.is_ipv4()).unwrap().port();
            format!("{}@127.0.0.1:{port}", endpoint.id()).parse().unwrap()
        });
        (runtime, address)
    }

    #[test]
    fn a_server_is_dialed_at_the_addresses_it_was_known_at() {
        let (_server, address) = server_that_can_vanish();
        let client = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let endpoint = client.block_on(bind(&DeviceKey::generate(), true)).unwrap();
        let known = vec![TransportAddr::Ip(address.direct.unwrap())];
        let unlisted = ServerAddr { direct: None, ..address };

        let dialed = client.block_on(Connection::dial_on(&endpoint, &unlisted, &known, ALPN)).unwrap();

        assert!(dialed.addresses().contains(&known[0]));
    }

    #[test]
    fn a_server_gone_silent_is_given_up_before_quic_would() {
        let (server, address) = server_that_can_vanish();
        let client = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let endpoint = client.block_on(bind(&DeviceKey::generate(), true)).unwrap();
        let connection = client.block_on(Connection::dial(&endpoint, &address)).unwrap();

        let silent = Instant::now();
        server.shutdown_background();
        client.block_on(connection.closed());

        assert!(silent.elapsed() < Duration::from_secs(25), "took {:?}", silent.elapsed());
    }
}
