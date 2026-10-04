//! Signing in and out, and the servers of the account.

use gpui_kit::prelude::*;
use motile_core::api::{AccountView, Command};

use super::{ServerUpdate, Store};
use crate::models::{EnrollToken, Server, is_older, now};

impl Store {
    pub(super) fn apply_account(&mut self, account: AccountView, cx: &mut Context<Self>) {
        let was_signed_in = self.account.signed_in;
        self.account = account;
        if was_signed_in && !self.account.signed_in {
            self.open_empty_draft(cx);
            self.enroll_token = None;
        }
    }

    pub(super) fn apply_servers(&mut self, servers: Vec<Server>) {
        self.servers = servers;
        let known: std::collections::HashSet<String> = self.servers.iter().map(|server| server.id.clone()).collect();
        self.projects.retain(|project| known.contains(&project.server_id));
        self.threads.retain(|_, thread| known.contains(&thread.server_id));
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
        self.ensure_draft_project();
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
            let Some((session, answer)) = crate::sign_in::start(&url) else {
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

    /// The browser has sent the sign-in back to the app.
    pub fn complete_sign_in(&mut self, url: String) {
        self.signing_in = true;
        self.ask(Command::CompleteSignIn { url }, |store, result, _| match result {
            Err(error) => store.sign_in_failed(error),
            Ok(_) => store.signing_in = false,
        });
    }

    /// Stops waiting for the browser, when the user has given up on it.
    pub fn cancel_sign_in(&mut self) {
        self.signing_in = false;
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
        if self.enroll_token.as_ref().is_some_and(|token| token.expires_at - now() > 600.) {
            return;
        }
        self.ask(Command::CreateEnrollToken, |store, result, _| match result {
            Ok(value) => store.enroll_token = serde_json::from_value::<EnrollToken>(value).ok(),
            Err(error) => store.fail(error),
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

    /// Has the server install the newest release and start it.
    pub fn update_server(&mut self, server: &Server) {
        if self.server_updates.contains_key(&server.id) {
            return;
        }
        let id = server.id.clone();
        self.server_updates
            .insert(id.clone(), ServerUpdate { from: server.version.clone(), fraction: None, restarting: false });
        self.ask(Command::UpdateServer { server_id: id.clone() }, move |store, result, cx| match result {
            Ok(_) => {
                if let Some(update) = store.server_updates.get_mut(&id) {
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

    pub fn remove_server(&mut self, server: &Server) {
        self.ask(Command::RemoveServer { server_id: server.id.clone() }, |store, result, _| {
            if let Err(error) = result {
                store.fail(error);
            }
        });
    }
}
