//! The iroh endpoint clients connect to. A connection is accepted only from a device linked to the
//! server's account; every stream on it carries one request.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;
use iroh::endpoint::{Connection, RecvStream, SendStream, presets};
use iroh::{Endpoint, SecretKey};
use motile_protocol::frame::{read_frame, write_frame};
use motile_protocol::identity::DeviceKey;
use motile_protocol::wire::{Message, Request};
use motile_protocol::{ALPN, error_text};
use tokio::io::AsyncReadExt;
use tokio::sync::broadcast::error::RecvError;

use crate::access::Access;
use crate::hub::{GitRun, Hub, ListSubscription, ThreadSubscription};
use crate::media::MediaStore;
use crate::{files, update};

const NOT_LINKED: u32 = 403;
const RECHECK_ACCESS_EVERY: Duration = Duration::from_secs(30);
/// Long enough for the client to hear that the update is installed.
const RESTART_AFTER: Duration = Duration::from_millis(500);
/// A long transcript is sent in pieces so the client can show the first ones while the rest travel.
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
            connection.close(NOT_LINKED.into(), b"This device isn't linked to the server's account.");
            return;
        }
        tracing::info!(device, "client connected");
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
                        connection.close(NOT_LINKED.into(), b"This device is no longer linked to the server's account.");
                        break;
                    }
                }
            }
        }
        tracing::info!(device, "client disconnected");
    }

    async fn handle_stream(self, mut send: SendStream, mut recv: RecvStream) -> anyhow::Result<()> {
        let request: Request = read_frame(&mut recv).await?.context("The stream ended before a request.")?;
        let hub = &self.hub;
        let reply = match request {
            Request::Subscribe => return follow_list(send, hub.subscribe().await).await,
            Request::UpdateServer => return update_server(send, hub).await,
            Request::GitRun { project_id, action, thread_id, message, paths, new_branch } => {
                let run = GitRun { action, thread_id, message, paths, new_branch };
                return git_run(send, hub.clone(), project_id, run).await;
            }
            Request::Media { id } => return send_media(send, &hub.media, &id).await,
            Request::ReadFile { project_id, thread_id, path } => {
                return send_file(send, hub, &project_id, thread_id.as_deref(), &path).await;
            }
            Request::Open { thread_id, since } => match hub.open(&thread_id, since).await {
                Ok(subscription) => return follow_thread(send, subscription).await,
                Err(error) => Err(error),
            },
            Request::Send { thread_id, new_thread, text, attachments } => {
                hub.send(thread_id, new_thread, text, attachments).await.map(|thread_id| Message::Sent { thread_id })
            }
            Request::Answer { thread_id, approval_id, allow, answers } => {
                hub.answer(&thread_id, &approval_id, allow, answers).await.map(|_| Message::Ok)
            }
            Request::SendQueued { thread_id, message_id } => {
                hub.send_queued(&thread_id, &message_id).await.map(|_| Message::Ok)
            }
            Request::CancelQueued { thread_id, message_id } => {
                hub.cancel_queued(&thread_id, &message_id).await.map(|_| Message::Ok)
            }
            Request::Stop { thread_id } => {
                hub.stop(&thread_id).await;
                Ok(Message::Ok)
            }
            Request::Update { thread_id, change } => hub.update(&thread_id, change).await.map(|_| Message::Ok),
            Request::Delete { thread_id } => hub.delete(&thread_id).await.map(|_| Message::Ok),
            Request::AddProject { path } => hub.add_project(&path).await.map(|_| Message::Ok),
            Request::RemoveProject { project_id } => hub.remove_project(&project_id).await.map(|_| Message::Ok),
            Request::ProjectIcon { project_id } => hub.project_icon(&project_id).await,
            Request::SetProjectIcon { project_id, path } => {
                hub.set_project_icon(&project_id, path).await.map(|_| Message::Ok)
            }
            Request::Branches { project_id } => hub.branches(&project_id).await,
            Request::SwitchBranch { project_id, branch, create } => {
                hub.switch_branch(&project_id, &branch, create).await.map(|_| Message::Ok)
            }
            Request::GitStatus { project_id, thread_id, fetch } => {
                hub.git_status(&project_id, thread_id.as_deref(), fetch).await
            }
            Request::Diff { project_id, thread_id, scope } => hub.diff(&project_id, thread_id.as_deref(), scope).await,
            Request::PullRequest { project_id, thread_id, number } => {
                hub.pull_request(&project_id, thread_id.as_deref(), number).await
            }
            Request::PullRequestEdit { project_id, thread_id, number, edit } => {
                hub.pull_request_edit(&project_id, thread_id.as_deref(), number, edit).await
            }
            Request::PullRequests { project_id, thread_id, state } => {
                hub.pull_requests(&project_id, thread_id.as_deref(), state).await
            }
            Request::LinkPullRequest { thread_id, number } => {
                hub.link_pull_request(&thread_id, number).await.map(|_| Message::Ok)
            }
            Request::WatchPullRequest { thread_id, watch } => {
                hub.watch_pull_request(&thread_id, watch).await.map(|_| Message::Ok)
            }
            Request::SetPullRequestSettings { done_on_merge, remove_merged_worktrees } => {
                hub.set_pull_request_settings(done_on_merge, remove_merged_worktrees).map(|_| Message::Ok)
            }
            Request::PullRequestAction { project_id, thread_id, number, action, method, text } => {
                hub.pull_request_action(&project_id, thread_id.as_deref(), number, action, method, text.as_deref())
                    .await
            }
            Request::ListFiles { project_id, thread_id, path } => {
                hub.list_files(&project_id, thread_id.as_deref(), &path).await
            }
            Request::SetTextModel { model } => hub.set_text_model(model).map(|_| Message::Ok),
            Request::SetBranchInstructions { instructions } => {
                hub.set_branch_instructions(instructions).map(|_| Message::Ok)
            }
            Request::SetProjectSetup { project_id, script } => {
                hub.set_project_setup(&project_id, script).await.map(|_| Message::Ok)
            }
            Request::NewProject { name } => {
                hub.new_project(&name).await.map(|project_id| Message::ProjectAdded { project_id })
            }
            Request::GithubStatus => Ok(Message::Github { state: hub.github().await }),
            Request::GithubRepos => hub.github_repos().await,
            Request::CloneRepo { repo } => {
                hub.clone_repo(&repo).await.map(|project_id| Message::ProjectAdded { project_id })
            }
            Request::ListDir { path, icons, hidden } => {
                files::list_dir(path.as_deref(), &hub.server_info().home, icons, hidden)
            }
            Request::Upload { name, size, poster_of } => {
                let saved =
                    files::receive_upload(&mut recv, &self.attachments, &name, size, poster_of.as_deref()).await;
                saved.map(|path| Message::Uploaded { path })
            }
        };
        let reply = reply.unwrap_or_else(|error| Message::Error { message: error_text(&error) });
        write_frame(&mut send, &reply).await?;
        send.finish()?;
        Ok(())
    }
}

/// Installs the latest release while telling the client how far the download is, then starts the
/// new program in this one's place.
async fn update_server(mut send: SendStream, hub: &Hub) -> anyhow::Result<()> {
    static UPDATING: AtomicBool = AtomicBool::new(false);
    let refusal = if hub.any_running().await {
        Some("Agents are still working. Update your server when they have finished.")
    } else if UPDATING.swap(true, Ordering::SeqCst) {
        Some("This server is already updating.")
    } else {
        None
    };
    if let Some(refusal) = refusal {
        write_frame(&mut send, &Message::Error { message: refusal.to_string() }).await?;
        send.finish()?;
        return Ok(());
    }

    // Where the program is, asked before it is replaced: afterwards the answer is the old file.
    let program = std::env::current_exe()?;
    let download_url = std::env::var("MOTILE_DOWNLOAD_URL").unwrap_or_else(|_| update::DOWNLOAD_URL.to_string());
    let (reports, mut progress) = tokio::sync::mpsc::unbounded_channel();
    let installing = tokio::spawn({
        let program = program.clone();
        async move {
            let report = |received, total| {
                let _ = reports.send((received, total));
            };
            update::install_latest(&download_url, &program, report).await
        }
    });
    while let Some((received, total)) = progress.recv().await {
        // The client may have gone; the update goes on without it.
        let _ = write_frame(&mut send, &Message::Updating { received, total }).await;
    }
    let installed = installing.await?;
    UPDATING.store(false, Ordering::SeqCst);
    if let Err(error) = installed {
        write_frame(&mut send, &Message::Error { message: error_text(&error) }).await?;
        send.finish()?;
        return Ok(());
    }
    let _ = write_frame(&mut send, &Message::Ok).await;
    let _ = send.finish();
    tracing::info!("updated; starting the new server");
    tokio::time::sleep(RESTART_AFTER).await;
    update::request_restart(program);
    Ok(())
}

/// Commits, pushes and the like, telling the client each stage as it starts. The run goes on when
/// the client has gone.
async fn git_run(mut send: SendStream, hub: Arc<Hub>, project_id: String, run: GitRun) -> anyhow::Result<()> {
    let (stages, mut started) = tokio::sync::mpsc::unbounded_channel();
    let running = tokio::spawn(async move {
        let report = |stage| {
            let _ = stages.send(stage);
        };
        hub.git_run(&project_id, run, report).await
    });
    while let Some(stage) = started.recv().await {
        let _ = write_frame(&mut send, &Message::GitProgress { stage }).await;
    }
    let reply = running.await?.unwrap_or_else(|error| Message::Error { message: error_text(&error) });
    write_frame(&mut send, &reply).await?;
    send.finish()?;
    Ok(())
}

async fn send_media(mut send: SendStream, media: &MediaStore, id: &str) -> anyhow::Result<()> {
    let (file, size) = match media.open(id).await {
        Ok(opened) => opened,
        Err(error) => {
            write_frame(&mut send, &Message::Error { message: error_text(&error) }).await?;
            send.finish()?;
            return Ok(());
        }
    };
    write_frame(&mut send, &Message::Media { size }).await?;
    tokio::io::copy(&mut file.take(size), &mut send).await?;
    send.finish()?;
    Ok(())
}

async fn send_file(
    mut send: SendStream,
    hub: &Hub,
    project_id: &str,
    thread_id: Option<&str>,
    path: &str,
) -> anyhow::Result<()> {
    let (file, kind, size, sent) = match hub.open_file(project_id, thread_id, path).await {
        Ok(opened) => opened,
        Err(error) => {
            write_frame(&mut send, &Message::Error { message: error_text(&error) }).await?;
            send.finish()?;
            return Ok(());
        }
    };
    write_frame(&mut send, &Message::File { kind, size, sent }).await?;
    tokio::io::copy(&mut file.take(sent), &mut send).await?;
    send.finish()?;
    Ok(())
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

/// Sends every update until the client stops listening.
async fn follow(mut send: SendStream, mut updates: tokio::sync::broadcast::Receiver<Message>) -> anyhow::Result<()> {
    loop {
        tokio::select! {
            update = updates.recv() => match update {
                Ok(message) => write_frame(&mut send, &message).await?,
                // A client that fell behind opens the stream again and catches up from its revision.
                Err(RecvError::Lagged(_)) | Err(RecvError::Closed) => break,
            },
            _ = send.stopped() => break,
        }
    }
    send.finish()?;
    Ok(())
}
