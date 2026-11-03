//! What a client asks of the gateway: its accounts, signing them in, its capture and its log, and its
//! MCP servers.

use motile_protocol::error_text;
use motile_protocol::gateway_wire::{GatewayReply, Manage, McpChange};

use crate::gateway::Gateway;

pub async fn handle(gateway: &Gateway, command: Manage) -> GatewayReply {
    match answer(gateway, command).await {
        Ok(reply) => reply,
        Err(error) => GatewayReply::Error { message: error_text(&error) },
    }
}

async fn answer(gateway: &Gateway, command: Manage) -> anyhow::Result<GatewayReply> {
    let state = |gateway: &Gateway| GatewayReply::State { state: gateway.state() };
    Ok(match command {
        Manage::State => state(gateway),
        Manage::SignIn { agent } => GatewayReply::SignInStarted {
            sign_in: gateway.sign_ins.start(&gateway.http, &gateway.upstreams, agent).await?,
        },
        Manage::FinishSignIn { id, code } => {
            let signed = gateway.sign_ins.finish(&gateway.http, &gateway.upstreams, &id, code.as_deref()).await?;
            let account = gateway.signed_in(signed);
            if let Err(error) = gateway.poll(&account).await {
                tracing::debug!("couldn't read a new account's limits: {error:#}");
            }
            state(gateway)
        }
        Manage::UpdateAccount { id, change } => {
            gateway.update_account(&id, change)?;
            state(gateway)
        }
        Manage::RemoveAccount { id } => {
            gateway.remove_account(&id)?;
            state(gateway)
        }
        Manage::Log { before, errors_only, limit } => {
            GatewayReply::Log { entries: gateway.store.log(before, errors_only, limit)? }
        }
        Manage::Captured { id } => GatewayReply::Captured { request: gateway.store.captured(id)? },
        Manage::Capture { capture } => {
            gateway.set_capture(capture)?;
            state(gateway)
        }
        Manage::Usage { since } => GatewayReply::Usage { rows: gateway.store.usage(since)? },
        Manage::AddMcpServer { url, name, key } => {
            GatewayReply::McpAdding { adding: gateway.add_mcp_server(&url, name, key).await? }
        }
        Manage::SignInMcpServer { id } => GatewayReply::McpAdding { adding: gateway.sign_mcp_server_in(&id).await? },
        Manage::FinishMcpSignIn { state, code } => {
            GatewayReply::McpAdding { adding: gateway.finish_mcp_server_sign_in(&state, &code).await? }
        }
        Manage::UpdateMcpServer { id, change } => {
            let refreshes = matches!(change, McpChange::Key { .. });
            gateway.update_mcp_server(&id, change)?;
            if refreshes {
                gateway.refresh_mcp_server(&id).await;
            }
            state(gateway)
        }
        Manage::RefreshMcpServer { id } => {
            gateway.refresh_mcp_server(&id).await;
            state(gateway)
        }
        Manage::RemoveMcpServer { id } => {
            gateway.remove_mcp_server(&id)?;
            state(gateway)
        }
        Manage::McpCalls { before, errors_only, limit } => {
            GatewayReply::McpCalls { calls: gateway.store.calls(before, errors_only, limit)? }
        }
        Manage::McpCall { id } => GatewayReply::McpCall { call: gateway.store.call(id)? },
        Manage::TryTool { id, tool, arguments } => {
            GatewayReply::ToolResult { result: gateway.try_tool(&id, &tool, arguments).await? }
        }
    })
}
