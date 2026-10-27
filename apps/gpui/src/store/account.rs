//! Signing in and out, and the servers of the account.

use gpui_kit::prelude::*;
use motile_core::api::{AccountView, Command};

use motile_protocol::wire::RestartWhen;

use super::Store;
use crate::media::pictures::download;
use crate::models::{EnrollToken, Server, ServerUpdate, is_older, now};

impl Store {
    pub(super) fn apply_account(&mut self, account: AccountView, cx: &mut Context<Self>) {
        let was_signed_in = self.account.signed_in;
        let picture_before = self.account.user.as_ref().and_then(|user| user.picture.clone());
        self.account = account;
        if was_signed_in && !self.account.signed_in {
            self.open_empty_draft(cx);
            self.enroll_token = None;
            self.usage = super::usage::Usage::default();
        }
        let picture = self.account.user.as_ref().and_then(|user| user.picture.clone());
        if picture != picture_before || (picture.is_some() && self.account_picture.is_none()) {
            self.fetch_account_picture(picture, cx);
        }
    }

    /// Keeps the account's picture as a file, for the sidebar's account button. It is fetched
    /// once for each address.
    fn fetch_account_picture(&mut self, url: Option<String>, cx: &mut Context<Self>) {
        self.account_picture = None;
        let Some(url) = url else { return };
        let file = self.data_dir.join(format!("account-picture-{:016x}", picture_key(&url)));
        if file.exists() {
            self.account_picture = Some(file);
            return;
        }
        cx.spawn(async move |this, cx| {
            let saved = file.clone();
            let fetched = cx.background_executor().spawn(async move { download(&url, &saved) }).await;
            if !fetched {
                return;
            }
            let _ = this.update(cx, |store, cx| {
                store.account_picture = Some(file);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn apply_servers(&mut self, servers: Vec<Server>, cx: &mut Context<Self>) {
        let composer_was_connected = self.composer_server().is_some_and(Server::connected);
        let were_connected: std::collections::HashSet<String> =
            self.servers.iter().filter(|server| server.connected()).map(|server| server.id.clone()).collect();
        self.servers = servers;
        let connected: Vec<String> = self
            .servers
            .iter()
            .filter(|server| server.connected() && !were_connected.contains(&server.id))
            .map(|server| server.id.clone())
            .collect();
        for server_id in connected {
            self.linear_read(&server_id);
            self.ricrac_read(&server_id);
        }
        if !composer_was_connected && self.composer_server().is_some_and(Server::connected) {
            self.read_git(
                true,
                None::<fn(&mut Store, Vec<motile_protocol::wire::ChangedFile>, &mut Context<Store>)>,
                cx,
            );
            self.read_worktree_start(true);
        }
        let known: std::collections::HashSet<String> = self.servers.iter().map(|server| server.id.clone()).collect();
        self.projects.retain(|project| known.contains(&project.server_id));
        self.no_projects.retain(|project| known.contains(&project.server_id));
        if self.threads.values().any(|thread| !known.contains(&thread.server_id)) {
            let threads = std::mem::take(&mut self.threads);
            self.set_threads(threads.into_iter().filter(|(_, thread)| known.contains(&thread.server_id)).collect());
        }
        // A server that is back with another version has finished updating.
        for server in self.servers.iter().filter(|server| server.connected()) {
            let finished = self
                .server_updates
                .get(&server.id)
                .is_some_and(|update| update.restarting && server.version != update.from);
            if finished {
                self.server_updates.remove(&server.id);
            }
        }
        self.show_server_updates(false);
        self.ensure_draft_project();
        self.resend_drafts(cx);
        // The server has arrived; the install command has done its job.
        if self.shows_add_server && self.servers.len() > self.add_server_count {
            self.shows_add_server = false;
        }
    }

    /// Opens the browser on the sign-in. It ends at `motile://auth`, which the system hands back
    /// to the app; `complete_sign_in` takes it from there.
    pub fn sign_in(&mut self, cx: &mut Context<Self>) {
        if self.signing_in {
            return;
        }
        self.signing_in = true;
        self.sign_in_error = None;
        self.ask(Command::BeginSignIn, |store, result, cx| {
            let url = result.ok().and_then(|value| value["url"].as_str().map(String::from));
            let Some(url) = url else {
                store.sign_in_failed("The sign-in couldn't be started.");
                return;
            };
            let Some((session, answer)) = crate::sign_in::start(&url, motile_protocol::APP_REDIRECT, cx) else {
                // Without the system's sheet the browser sends the sign-in back to `motile://auth`.
                cx.open_url(&url);
                return;
            };
            store.sign_in_session = Some(session);
            cx.spawn(async move |this, cx| {
                let callback = answer.await.ok().flatten();
                let _ = this.update(cx, |store, cx| {
                    store.sign_in_session = None;
                    match callback {
                        Some(url) => store.complete_sign_in(url),
                        None => store.signing_in = false,
                    }
                    cx.notify();
                });
            })
            .detach();
        });
        cx.notify();
    }

    /// Gives up waiting for the browser, which can't say that the user left it.
    pub fn cancel_sign_in(&mut self) {
        self.sign_in_session = None;
        self.signing_in = false;
    }

    /// The browser has sent the sign-in back to the app.
    pub fn complete_sign_in(&mut self, url: String) {
        self.signing_in = true;
        self.ask(Command::CompleteSignIn { url }, |store, result, _| match result {
            Err(error) => store.sign_in_failed(error),
            Ok(_) => store.signing_in = false,
        });
    }

    /// Signs in on an auth server that allows it without Google. Used for local work.
    pub fn dev_sign_in(&mut self, email: String) {
        self.signing_in = true;
        self.ask(Command::DevSignIn { email }, |store, result, _| match result {
            Err(error) => store.sign_in_failed(error),
            Ok(_) => store.signing_in = false,
        });
    }

    fn sign_in_failed(&mut self, message: impl Into<String>) {
        self.signing_in = false;
        self.sign_in_error = Some(message.into());
    }

    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        self.close_settings();
        self.drafts.clear();
        self.attachments_by_key.clear();
        self.thread_drafts.clear();
        self.prefs.remove("drafts");
        self.open_empty_draft(cx);
        self.send(Command::SignOut);
    }

    /// Asks for an install command and keeps looking for the server it will link.
    pub fn prepare_to_add_server(&mut self) {
        self.add_server_count = self.servers.len();
        self.send(Command::WatchServers { on: true });
        if self.enroll_token.as_ref().is_some_and(|token| !token.is_expired(now())) {
            return;
        }
        self.regenerate_enroll_token();
    }

    /// Asks for a new install command, for when the last one has expired.
    pub fn regenerate_enroll_token(&mut self) {
        if self.enroll_token_pending {
            return;
        }
        self.enroll_token_pending = true;
        self.ask(Command::CreateEnrollToken, |store, result, _| {
            store.enroll_token_pending = false;
            match result {
                Ok(value) => store.enroll_token = serde_json::from_value::<EnrollToken>(value).ok(),
                Err(error) => store.fail(error),
            }
        });
    }

    pub fn stop_adding_server(&mut self) {
        self.send(Command::WatchServers { on: false });
        // A token links one server; the next server gets a new one.
        if self.servers.len() != self.add_server_count {
            self.enroll_token = None;
        }
    }

    /// Whether the server runs an older version than the newest release.
    pub fn is_outdated(&self, server: &Server) -> bool {
        server.connected() && is_older(&server.version, self.updater.latest.as_deref())
    }

    /// The update a server is putting in place: as it says itself, or as this client started it.
    pub fn server_update<'a>(&'a self, server: &'a Server) -> Option<&'a ServerUpdate> {
        server.update.as_ref().or_else(|| self.server_updates.get(&server.id))
    }

    /// Whether an agent works on the server now.
    pub fn is_busy(&self, server: &Server) -> bool {
        self.threads.values().any(|thread| thread.server_id == server.id && thread.busy())
    }

    /// Has the server install the newest release and start it: `when` its agents work, once they
    /// have finished or at once. A server that can't choose refuses while they work.
    pub fn update_server(&mut self, server: &Server, when: RestartWhen) {
        let current = self.server_update(server);
        if current.is_some_and(|update| update.waiting) && when == RestartWhen::Now {
            return self.restart_now(server);
        }
        if current.is_some() {
            return;
        }
        let id = server.id.clone();
        self.server_updates.insert(id.clone(), ServerUpdate::new(&server.version));
        let when = server.can_choose_restart().then_some(when);
        self.ask(Command::UpdateServer { server_id: id.clone(), when }, move |store, result, cx| match result {
            Ok(_) => {
                if let Some(update) = store.server_updates.get_mut(&id) {
                    update.waiting = false;
                    update.restarting = true;
                }
                // If the server never says it is back, the row stops waiting for it.
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_secs(60)).await;
                    let _ = this.update(cx, |store, cx| {
                        if store.server_updates.get(&id).is_some_and(|update| update.restarting) {
                            store.server_updates.remove(&id);
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            Err(error) => {
                store.server_updates.remove(&id);
                store.fail(error);
            }
        });
    }

    /// Has a server that waits for its agents to update stop them and restart now. The request
    /// that started the update answers once it restarts.
    fn restart_now(&mut self, server: &Server) {
        let command = Command::UpdateServer { server_id: server.id.clone(), when: Some(RestartWhen::Now) };
        self.ask(command, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    pub fn remove_server(&mut self, server: &Server) {
        self.ask(Command::RemoveServer { server_id: server.id.clone() }, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }

    /// Removes the server if it is still linked, and deletes every copy of its threads.
    pub fn delete_server_completely(&mut self, server: &Server) {
        self.ask(Command::DeleteServerCompletely { server_id: server.id.clone() }, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }
}

fn picture_key(url: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    hasher.finish()
}
