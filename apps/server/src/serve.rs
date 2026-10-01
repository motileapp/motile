//! The iroh endpoint apps connect to. A connection is accepted only from a device linked to the
//! host's account; every stream on it carries one request.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use iroh::endpoint::{Connection, RecvStream, SendStream, presets};
use iroh::{Endpoint, SecretKey};
use motile_protocol::ALPN;
use motile_protocol::frame::{read_frame, write_frame};
use motile_protocol::identity::DeviceKey;
use motile_protocol::wire::{Message, Request};
use tokio::sync::broadcast::error::RecvError;

use crate::access::Access;
use crate::files;
use crate::hub::{Hub, ListSubscription, ThreadSubscription};

const NOT_LINKED: u32 = 403;
const RECHECK_ACCESS_EVERY: Duration = Duration::from_secs(30);
/// A long transcript is sent in pieces so the app can show the first ones while the rest travel.
const ITEMS_PER_MESSAGE: usize = 200;

#[derive(Default)]
pub struct BindOptions {
    /// No relays and no address lookup: reachable only at the addresses it is bound to.
    pub local_only: bool,
    pub port: Option<u16>,
}

pub async fn bind(key: &DeviceKey, options: &BindOptions) -> anyhow::Result<Endpoint> {
    let builder = match options.local_only {
        true => Endpoint::builder(presets::Minimal),
        false => Endpoint::builder(presets::N0),
    };
    let mut builder = builder.secret_key(SecretKey::from_bytes(&key.to_bytes())).alpns(vec![ALPN.to_vec()]);
    if let Some(port) = options.port {
        builder = builder.bind_addr(format!("0.0.0.0:{port}"))?;
    }
    builder.bind().await.context("The network endpoint couldn't be opened.")
}

#[derive(Clone)]
pub struct Server {
    pub hub: Arc<Hub>,
    pub access: Arc<Access>,
    pub attachments: PathBuf,
}

impl Server {
    pub async fn run(self, endpoint: Endpoint) {
        while let Some(incoming) = endpoint.accept().await {
            let server = self.clone();
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else { return };
                server.handle_connection(connection).await;
            });
        }
    }

    async fn handle_connection(self, connection: Connection) {
        let device = connection.remote_id().to_string();
        if !self.access.allows(&device).await {
            tracing::warn!(device, "refused a device that isn't linked");
            connection.close(NOT_LINKED.into(), b"This device isn't linked to the host's account.");
            return;
        }
        tracing::info!(device, "app connected");
        let mut recheck = tokio::time::interval(RECHECK_ACCESS_EVERY);
        loop {
            tokio::select! {
                stream = connection.accept_bi() => {
                    let Ok((send, recv)) = stream else { break };
                    let server = self.clone();
                    tokio::spawn(async move {
                        if let Err(error) = server.handle_stream(send, recv).await {
                            tracing::debug!("stream ended: {error:#}");
                        }
                    });
                }
                _ = recheck.tick() => {
                    if !self.access.is_listed(&device).await {
                        tracing::warn!(device, "closed a device that was removed from the account");
                        connection.close(NOT_LINKED.into(), b"This device is no longer linked to the host's account.");
                        break;
                    }
                }
            }
        }
        tracing::info!(device, "app disconnected");
    }

    async fn handle_stream(self, mut send: SendStream, mut recv: RecvStream) -> anyhow::Result<()> {
        let request: Request = read_frame(&mut recv).await?.context("The stream ended before a request.")?;
        let hub = &self.hub;
        let reply = match request {
            Request::Subscribe => return follow_list(send, hub.subscribe().await).await,
            Request::Open { thread_id, since } => match hub.open(&thread_id, since).await {
                Ok(subscription) => return follow_thread(send, subscription).await,
                Err(error) => Err(error),
            },
            Request::Send { thread_id, new_thread, text, attachments } => {
                hub.send(thread_id, new_thread, text, attachments).await.map(|thread_id| Message::Sent { thread_id })
            }
            Request::Allow { thread_id, denials } => hub.allow(&thread_id, denials).await.map(|_| Message::Ok),
            Request::Stop { thread_id } => {
                hub.stop(&thread_id).await;
                Ok(Message::Ok)
            }
            Request::Update { thread_id, change } => hub.update(&thread_id, change).await.map(|_| Message::Ok),
            Request::Delete { thread_id } => hub.delete(&thread_id).await.map(|_| Message::Ok),
            Request::AddProject { path } => hub.add_project(&path).await.map(|_| Message::Ok),
            Request::RemoveProject { project_id } => hub.remove_project(&project_id).await.map(|_| Message::Ok),
            Request::ListDir { path } => files::list_dir(path.as_deref(), &hub.host_info().home),
            Request::Upload { name, size } => {
                let saved = files::receive_upload(&mut recv, &self.attachments, &name, size).await;
                saved.map(|path| Message::Uploaded { path })
            }
        };
        let reply = reply.unwrap_or_else(|error| Message::Error { message: format!("{error:#}") });
        write_frame(&mut send, &reply).await?;
        send.finish()?;
        Ok(())
    }
}

async fn follow_list(mut send: SendStream, subscription: ListSubscription) -> anyhow::Result<()> {
    write_frame(&mut send, &subscription.first).await?;
    follow(send, subscription.updates).await
}

async fn follow_thread(mut send: SendStream, subscription: ThreadSubscription) -> anyhow::Result<()> {
    let opened = Message::Opened { reset: subscription.reset, activity: subscription.activity };
    write_frame(&mut send, &opened).await?;
    for items in subscription.items.chunks(ITEMS_PER_MESSAGE) {
        write_frame(&mut send, &Message::Items { items: items.to_vec() }).await?;
    }
    write_frame(&mut send, &Message::Synced { rev: subscription.rev }).await?;
    follow(send, subscription.updates).await
}

/// Sends every update until the app stops listening.
async fn follow(mut send: SendStream, mut updates: tokio::sync::broadcast::Receiver<Message>) -> anyhow::Result<()> {
    loop {
        tokio::select! {
            update = updates.recv() => match update {
                Ok(message) => write_frame(&mut send, &message).await?,
                // An app that fell behind opens the stream again and catches up from its revision.
                Err(RecvError::Lagged(_)) | Err(RecvError::Closed) => break,
            },
            _ = send.stopped() => break,
        }
    }
    send.finish()?;
    Ok(())
}
