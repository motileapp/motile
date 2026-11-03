//! The iroh endpoint the account's servers and clients connect to. Every stream carries one request:
//! a server's HTTP request or WebSocket, or a client's management command.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use iroh::endpoint::{Connection, PathEvent, PathEventStream, RecvStream, SendStream, presets};
use iroh::{Endpoint, SecretKey};
use motile_protocol::GATEWAY_ALPN;
use motile_protocol::frame::{read_frame, write_frame};
use motile_protocol::gateway_wire::{GatewayReply, GatewayRequest};
use motile_protocol::identity::DeviceKey;

use crate::access::{Access, Role};
use crate::gateway::Gateway;
use crate::mcp::passthrough::{self, McpIncoming};
use crate::upstream::Incoming;
use crate::{manage, upstream, websocket};

const REFUSED: u32 = 403;
const RECHECK_ACCESS_EVERY: Duration = Duration::from_secs(30);

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
    let mut builder = builder.secret_key(SecretKey::from_bytes(&key.to_bytes())).alpns(vec![GATEWAY_ALPN.to_vec()]);
    if let Some(port) = options.port {
        builder = builder.bind_addr(format!("0.0.0.0:{port}"))?;
    }
    builder.bind().await.context("The network endpoint couldn't be opened.")
}

pub async fn run(gateway: Arc<Gateway>, access: Arc<Access>, endpoint: Endpoint) {
    while let Some(incoming) = endpoint.accept().await {
        let gateway = gateway.clone();
        let access = access.clone();
        tokio::spawn(async move {
            let Ok(connection) = incoming.await else { return };
            handle_connection(gateway, access, connection).await;
        });
    }
}

/// Says in the log whether a device's traffic goes straight to it or through a relay, each time
/// that changes.
async fn log_paths(device: String, mut events: PathEventStream) {
    while let Some(event) = events.next().await {
        let PathEvent::Selected { remote_addr, .. } = event else { continue };
        let path = if remote_addr.is_relay() { "relayed" } else { "direct" };
        tracing::info!(device, path, "traffic path");
    }
}

async fn handle_connection(gateway: Arc<Gateway>, access: Arc<Access>, connection: Connection) {
    let device = connection.remote_id().to_string();
    let Some(role) = access.role(&device).await else {
        tracing::warn!(device, "refused a device that isn't of the gateway's account");
        connection.close(REFUSED.into(), b"This device isn't linked to the gateway's account.");
        return;
    };
    tracing::info!(device, ?role, "connected");
    tokio::spawn(log_paths(device.clone(), connection.path_events()));
    let mut recheck = tokio::time::interval(RECHECK_ACCESS_EVERY);
    loop {
        tokio::select! {
            stream = connection.accept_bi() => {
                let Ok((send, recv)) = stream else { break };
                let gateway = gateway.clone();
                let device = device.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_stream(gateway, role, device, send, recv).await {
                        tracing::debug!("stream ended: {error:#}");
                    }
                });
            }
            _ = recheck.tick() => {
                if access.listed(&device).await != Some(role) {
                    tracing::warn!(device, "closed a device that was removed from the account");
                    connection.close(REFUSED.into(), b"This device is no longer linked to the gateway's account.");
                    break;
                }
            }
        }
    }
    tracing::info!(device, "disconnected");
}

async fn handle_stream(
    gateway: Arc<Gateway>,
    role: Role,
    device: String,
    mut send: SendStream,
    mut recv: RecvStream,
) -> anyhow::Result<()> {
    let request: GatewayRequest = read_frame(&mut recv).await?.context("The stream ended before a request.")?;
    let reply = match (request, role) {
        (GatewayRequest::Http { thread, agent, method, path, headers, pin }, Role::Server) => {
            let incoming = Incoming { server: device, thread, agent, method, path, headers, pin };
            return upstream::serve(gateway, incoming, send, recv).await;
        }
        (GatewayRequest::WebSocket { thread, agent, path, headers, pin }, Role::Server) => {
            let incoming = Incoming { server: device, thread, agent, method: "GET".into(), path, headers, pin };
            return websocket::serve(gateway, incoming, send, recv).await;
        }
        (GatewayRequest::Mcp { thread, mcp_server, method, headers }, Role::Server) => {
            let incoming = McpIncoming { server: device, thread, mcp_server, method, headers };
            return passthrough::serve(gateway, incoming, send, recv).await;
        }
        (GatewayRequest::McpServers, Role::Server) => {
            GatewayReply::McpServers { mcp_servers: gateway.mcp_servers.given() }
        }
        (GatewayRequest::Pool { agent }, Role::Server) => GatewayReply::Pool { limits: gateway.pool(agent) },
        (GatewayRequest::Accounts, Role::Server) => GatewayReply::Accounts { accounts: gateway.state().accounts },
        (GatewayRequest::Manage { command }, Role::Client) => manage::handle(&gateway, command).await,
        _ => GatewayReply::Error { message: "This device can't ask the gateway that.".into() },
    };
    write_frame(&mut send, &reply).await?;
    send.finish()?;
    Ok(())
}
