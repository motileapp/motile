//! What a client asks of the proxy: its accounts, signing them in, its capture and its log.

use motile_protocol::error_text;
use motile_protocol::proxy_wire::{Manage, ProxyReply};

use crate::proxy::Proxy;

pub async fn handle(proxy: &Proxy, command: Manage) -> ProxyReply {
    match answer(proxy, command).await {
        Ok(reply) => reply,
        Err(error) => ProxyReply::Error { message: error_text(&error) },
    }
}

async fn answer(proxy: &Proxy, command: Manage) -> anyhow::Result<ProxyReply> {
    let state = |proxy: &Proxy| ProxyReply::State { state: proxy.state() };
    Ok(match command {
        Manage::State => state(proxy),
        Manage::SignIn { agent } => {
            ProxyReply::SignInStarted { sign_in: proxy.sign_ins.start(&proxy.http, &proxy.upstreams, agent).await? }
        }
        Manage::FinishSignIn { id, code } => {
            let signed = proxy.sign_ins.finish(&proxy.http, &proxy.upstreams, &id, code.as_deref()).await?;
            let account = proxy.signed_in(signed);
            if let Err(error) = proxy.poll(&account).await {
                tracing::debug!("couldn't read a new account's limits: {error:#}");
            }
            state(proxy)
        }
        Manage::UpdateAccount { id, change } => {
            proxy.update_account(&id, change)?;
            state(proxy)
        }
        Manage::RemoveAccount { id } => {
            proxy.remove_account(&id)?;
            state(proxy)
        }
        Manage::Log { before, errors_only, limit } => {
            ProxyReply::Log { entries: proxy.store.log(before, errors_only, limit)? }
        }
        Manage::Captured { id } => ProxyReply::Captured { request: proxy.store.captured(id)? },
        Manage::Capture { capture } => {
            proxy.set_capture(capture)?;
            state(proxy)
        }
        Manage::Usage { since } => ProxyReply::Usage { rows: proxy.store.usage(since)? },
    })
}
