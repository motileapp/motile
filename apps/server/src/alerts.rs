//! What the user would want to hear of the server's threads: an agent finished, failed, waits for
//! the user or reached its usage limit, and what the agents are doing. The clients that watch hear
//! each alert as it happens; the account's phones are pushed it through the auth server, sealed.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use motile_protocol::alerts::{
    Alert, AlertKind, ENDED_WORK_STAYS_SECS, NewAlert, Pushed, ServerWork, WorkItem, WorkState, seal_pushed,
};
use motile_protocol::auth_api::{PushTarget, PushWork, SealedAlert, SealedFor};
use motile_protocol::now;
use motile_protocol::wire::{Ask, Interruption, Message, Project, Thread, TurnOutcome, TurnStarter};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc};
use tokio::time::Instant;

use crate::access::Access;
use crate::hub::Hub;

/// How long the end of a turn the server started by itself waits, as its agent may go on at once.
const WOKEN_END_WAITS: Duration = Duration::from_secs(5);
const WORK_EVERY: Duration = Duration::from_secs(2);
const WORK_TITLE_CHARS: usize = 60;
const TARGETS_STAY: Duration = Duration::from_secs(5 * 60);
const ALERTS_BUFFER: usize = 256;
/// How many parents up a thread's top parent is looked for.
const DEEPEST_PARENT: usize = 32;

pub struct Alerts {
    sender: broadcast::Sender<Alert>,
}

impl Alerts {
    /// Follows the hub's threads from now on. `server_id` is the server's device key.
    pub async fn start(hub: Arc<Hub>, access: Arc<Access>, server_id: String) -> Arc<Self> {
        let (sender, _) = broadcast::channel(ALERTS_BUFFER);
        let pushes = access.auth().is_some().then(|| Pusher::start(access));
        let subscription = hub.subscribe().await;
        let server_name = hub.server_info().hostname;
        let (releases, released) = mpsc::unbounded_channel();
        let watcher = Watcher {
            hub,
            server_id,
            server_name,
            alerts: sender.clone(),
            pushes,
            threads: HashMap::new(),
            projects: HashMap::new(),
            states: HashMap::new(),
            releases,
            sent: None,
            sent_at: None,
            held: false,
        };
        tokio::spawn(watcher.run(subscription.first, subscription.updates, released));
        Arc::new(Self { sender })
    }

    pub fn watch(&self) -> broadcast::Receiver<Alert> {
        self.sender.subscribe()
    }
}

/// A thread as last seen, with the latest end of a turn it alerted for or let pass.
struct Watched {
    thread: Thread,
    handled: Option<f64>,
}

/// The end of a turn the server started by itself, once it has waited.
struct Release {
    thread_id: String,
    at: f64,
    kind: AlertKind,
}

enum Push {
    Alert(Alert),
    Work(ServerWork),
}

struct Watcher {
    hub: Arc<Hub>,
    server_id: String,
    server_name: String,
    alerts: broadcast::Sender<Alert>,
    pushes: Option<mpsc::UnboundedSender<Push>>,
    threads: HashMap<String, Watched>,
    /// The projects' names, by id.
    projects: HashMap<String, String>,
    /// What each thread is doing and since when, for the work.
    states: HashMap<String, (WorkState, f64)>,
    releases: mpsc::UnboundedSender<Release>,
    /// The work last pushed, when, and whether newer work waits to be.
    sent: Option<ServerWork>,
    sent_at: Option<Instant>,
    held: bool,
}

impl Watcher {
    async fn run(
        mut self,
        first: Message,
        mut updates: broadcast::Receiver<Message>,
        mut released: mpsc::UnboundedReceiver<Release>,
    ) {
        self.take(first).await;
        loop {
            let due = self.work_due();
            tokio::select! {
                update = updates.recv() => match update {
                    Ok(message) => self.take(message).await,
                    Err(RecvError::Lagged(_)) => {
                        let subscription = self.hub.subscribe().await;
                        updates = subscription.updates;
                        self.take(subscription.first).await;
                    }
                    Err(RecvError::Closed) => return,
                },
                Some(release) = released.recv() => self.release(release),
                _ = tokio::time::sleep_until(due.unwrap_or_else(Instant::now)), if due.is_some() => {}
            }
            self.keep_work_sent();
        }
    }

    async fn take(&mut self, message: Message) {
        match message {
            Message::Welcome { threads, projects, .. } => {
                self.projects = names(projects);
                let listed: HashSet<&str> = threads.iter().map(|thread| thread.id.as_str()).collect();
                self.threads.retain(|id, _| listed.contains(id.as_str()));
                self.states.retain(|id, _| listed.contains(id.as_str()));
                for thread in threads {
                    self.observe(thread).await;
                }
            }
            Message::Projects { projects } => self.projects = names(projects),
            Message::ThreadUpsert { thread } => self.observe(thread).await,
            Message::ThreadDeleted { thread_id } => {
                self.threads.remove(&thread_id);
                self.states.remove(&thread_id);
            }
            _ => {}
        }
    }

    async fn observe(&mut self, thread: Thread) {
        let watched = self.threads.get(&thread.id);
        let prior = watched.map(|watched| &watched.thread);
        let observed = observe(prior, &thread, watched.and_then(|watched| watched.handled), now());
        note_state(&mut self.states, &thread, now());
        self.threads.insert(thread.id.clone(), Watched { thread: thread.clone(), handled: observed.handled });
        for call in observed.calls {
            self.answer(&thread, call).await;
        }
    }

    async fn answer(&mut self, thread: &Thread, call: Call) {
        match call {
            Call::Waits(kind) => {
                let top = top_of(thread, |id| self.threads.get(id).map(|watched| &watched.thread)).clone();
                if top.archived_at.is_some() {
                    return;
                }
                let detail = match (top.id == thread.id, kind) {
                    (false, _) => Some(thread.title.clone()),
                    (true, AlertKind::Approval) => self.hub.waiting_call(&thread.id).await,
                    (true, _) => None,
                };
                self.alert(&top, kind, detail);
            }
            Call::Limit => self.alert(thread, AlertKind::Limit, None),
            Call::Ended { at, after } => {
                let turn = match self.hub.store().turn_ended_since(&thread.id, after, at) {
                    Ok(turn) => turn,
                    Err(error) => {
                        tracing::warn!(thread_id = thread.id, "couldn't read how a turn ended: {error:#}");
                        return;
                    }
                };
                let Some((starter, kind)) = turn.and_then(|(starter, outcome)| Some((starter, ended_kind(outcome)?)))
                else {
                    return;
                };
                if starter != TurnStarter::Server {
                    return self.ended(thread, kind, at);
                }
                let releases = self.releases.clone();
                let thread_id = thread.id.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(WOKEN_END_WAITS).await;
                    let _ = releases.send(Release { thread_id, at, kind });
                });
            }
        }
    }

    /// Alerts for the end of a turn the server started by itself, if the thread still rests there.
    fn release(&mut self, release: Release) {
        let Some(watched) = self.threads.get(&release.thread_id) else { return };
        let thread = &watched.thread;
        let still = settled(thread) && thread.turn_ended_at == Some(release.at);
        if !still || thread.archived_at.is_some() || snoozed(thread, release.at, now()) {
            return;
        }
        let thread = thread.clone();
        self.ended(&thread, release.kind, release.at);
    }

    fn ended(&mut self, thread: &Thread, kind: AlertKind, at: f64) {
        let state = if kind == AlertKind::Failed { WorkState::Failed } else { WorkState::Finished };
        self.states.insert(thread.id.clone(), (state, at));
        self.alert(thread, kind, None);
    }

    fn alert(&self, thread: &Thread, kind: AlertKind, detail: Option<String>) {
        let new = NewAlert {
            server_id: self.server_id.clone(),
            thread_id: thread.id.clone(),
            kind,
            thread_title: thread.title.clone(),
            project: self.projects.get(&thread.project_id).cloned().unwrap_or_default(),
            detail,
        };
        let alert = Alert::new(new, uuid::Uuid::new_v4().to_string(), now());
        let _ = self.alerts.send(alert.clone());
        if let Some(pushes) = &self.pushes {
            let _ = pushes.send(Push::Alert(alert));
        }
    }

    /// Pushes the work when it changed, at most once every `WORK_EVERY`, and once on start, so that
    /// what the server said before it stopped doesn't stay on the phones.
    fn keep_work_sent(&mut self) {
        let Some(pushes) = &self.pushes else { return };
        let now = now();
        self.states.retain(|_, (state, since)| !ended(*state) || *since + ENDED_WORK_STAYS_SECS > now);
        let items = work_items(self.threads.values().map(|watched| &watched.thread), &self.states, &self.projects);
        let unchanged = self.sent.as_ref().is_some_and(|sent| sent.items == items);
        self.held = !unchanged;
        if unchanged || self.sent_at.is_some_and(|at| at.elapsed() < WORK_EVERY) {
            return;
        }
        let work = ServerWork::new(self.server_id.clone(), self.server_name.clone(), items, now);
        let _ = pushes.send(Push::Work(work.clone()));
        self.sent = Some(work);
        self.sent_at = Some(Instant::now());
        self.held = false;
    }

    /// When the work is to be looked at again: once held work may go, or an ended thread leaves it.
    fn work_due(&self) -> Option<Instant> {
        self.pushes.as_ref()?;
        let held = self.sent_at.filter(|_| self.held).map(|at| at + WORK_EVERY);
        let leaves = self
            .states
            .values()
            .filter(|(state, _)| ended(*state))
            .map(|(_, since)| since + ENDED_WORK_STAYS_SECS)
            .min_by(f64::total_cmp)
            .map(|at| Instant::now() + Duration::from_secs_f64((at - now()).max(0.0)));
        held.into_iter().chain(leaves).min()
    }
}

/// What a thread's change calls for.
#[derive(Debug, PartialEq)]
enum Call {
    /// It waits for the user.
    Waits(AlertKind),
    Limit,
    /// Its turn ended at `at`; how is up to the turns that ended after `after`.
    Ended {
        at: f64,
        after: Option<f64>,
    },
}

#[derive(Debug, PartialEq)]
struct Observed {
    calls: Vec<Call>,
    /// The latest end of a turn alerted for or let pass, now.
    handled: Option<f64>,
}

/// What the change from `prior` to `next` calls for, given the latest end of a turn `handled`. The
/// first sight of a thread calls for nothing, and nor does an archived thread. A thread another
/// thread's agent started only calls for what it waits for, which its top parent alerts for.
fn observe(prior: Option<&Thread>, next: &Thread, handled: Option<f64>, now: f64) -> Observed {
    let fresh_end = next.turn_ended_at.filter(|at| settled(next) && handled.is_none_or(|handled| *at > handled));
    let mut observed = Observed { calls: Vec::new(), handled: fresh_end.or(handled) };
    let Some(prior) = prior else { return observed };
    if next.archived_at.is_some() {
        return observed;
    }
    if next.needs_approval && (!prior.needs_approval || prior.asking != next.asking) {
        observed.calls.push(Call::Waits(waiting_kind(next.asking)));
    }
    if next.parent.is_some() {
        return observed;
    }
    if limited(next) && !limited(prior) {
        observed.calls.push(Call::Limit);
    }
    if let Some(at) = fresh_end
        && !snoozed(next, at, now)
    {
        observed.calls.push(Call::Ended { at, after: handled });
    }
    observed
}

/// The thread at the top of the threads whose agents started `thread`, as far as `find` knows them.
fn top_of<'a>(thread: &'a Thread, find: impl Fn(&str) -> Option<&'a Thread>) -> &'a Thread {
    let mut top = thread;
    for _ in 0..DEEPEST_PARENT {
        let Some(parent) = top.parent.as_ref().and_then(|parent| find(&parent.thread_id)) else { break };
        top = parent;
    }
    top
}

/// The threads the phones show, the ones waiting for the user first, then those that work, then
/// those that ended, each the most recent first.
fn work_items<'a>(
    threads: impl Iterator<Item = &'a Thread>,
    states: &HashMap<String, (WorkState, f64)>,
    projects: &HashMap<String, String>,
) -> Vec<WorkItem> {
    let shown = threads.filter(|thread| thread.parent.is_none() && thread.archived_at.is_none());
    let items: Vec<WorkItem> = shown
        .filter_map(|thread| {
            let (state, since) = *states.get(&thread.id)?;
            Some(WorkItem {
                thread_id: thread.id.clone(),
                title: cut(&thread.title, WORK_TITLE_CHARS),
                project: projects.get(&thread.project_id).cloned().unwrap_or_default(),
                state,
                since,
            })
        })
        .collect();
    items
}

/// Keeps what the thread is doing now and since when; an ended turn is noted once it alerts.
fn note_state(states: &mut HashMap<String, (WorkState, f64)>, thread: &Thread, now: f64) {
    let state = match () {
        _ if thread.needs_approval => WorkState::Waiting,
        _ if thread.running || thread.monitoring => WorkState::Working,
        _ => {
            if states.get(&thread.id).is_some_and(|(state, _)| !ended(*state)) {
                states.remove(&thread.id);
            }
            return;
        }
    };
    if states.get(&thread.id).is_none_or(|(kept, _)| *kept != state) {
        states.insert(thread.id.clone(), (state, now));
    }
}

fn settled(thread: &Thread) -> bool {
    !thread.running && !thread.monitoring && !thread.needs_approval
}

fn limited(thread: &Thread) -> bool {
    matches!(thread.interruption, Some(Interruption::Limit { .. }))
}

/// Snoozed now, or when its turn ended at `at`.
fn snoozed(thread: &Thread, at: f64, now: f64) -> bool {
    thread.snoozed_until.is_some_and(|until| until > at.min(now))
}

fn ended(state: WorkState) -> bool {
    matches!(state, WorkState::Finished | WorkState::Failed)
}

fn waiting_kind(asking: Option<Ask>) -> AlertKind {
    match asking {
        Some(Ask::Question) => AlertKind::Question,
        Some(Ask::Plan) => AlertKind::Plan,
        _ => AlertKind::Approval,
    }
}

fn ended_kind(outcome: TurnOutcome) -> Option<AlertKind> {
    match outcome {
        TurnOutcome::Done => Some(AlertKind::Finished),
        TurnOutcome::Failed => Some(AlertKind::Failed),
        _ => None,
    }
}

fn names(projects: Vec<Project>) -> HashMap<String, String> {
    projects.into_iter().map(|project| (project.id, project.name)).collect()
}

fn cut(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        return text.to_string();
    }
    let kept: String = text.chars().take(chars - 1).collect();
    format!("{}…", kept.trim_end())
}

/// Pushes alerts and work to the account's phones through the auth server, one after another.
struct Pusher {
    access: Arc<Access>,
    targets: Option<(Instant, Vec<PushTarget>)>,
}

impl Pusher {
    fn start(access: Arc<Access>) -> mpsc::UnboundedSender<Push> {
        let (sender, pushes) = mpsc::unbounded_channel();
        tokio::spawn(Self { access, targets: None }.run(pushes));
        sender
    }

    async fn run(mut self, mut pushes: mpsc::UnboundedReceiver<Push>) {
        while let Some(push) = pushes.recv().await {
            if let Err(error) = self.push(push).await {
                tracing::warn!("couldn't push to the account's phones: {error:#}");
                self.targets = None;
            }
        }
    }

    async fn push(&mut self, push: Push) -> anyhow::Result<()> {
        let access = self.access.clone();
        let Some((auth, key)) = access.auth() else { return Ok(()) };
        let targets = match &self.targets {
            Some((at, targets)) if at.elapsed() < TARGETS_STAY => targets.clone(),
            _ => {
                let targets = auth.push_targets(key).await?;
                self.targets = Some((Instant::now(), targets.clone()));
                targets
            }
        };
        match push {
            Push::Alert(alert) => {
                let sealed = seal_alert(&targets, alert);
                if sealed.is_empty() {
                    return Ok(());
                }
                auth.push_alerts(key, &sealed).await
            }
            Push::Work(work) => {
                let (working, waiting) = (work.working, work.waiting);
                let sealed = seal_work(&targets, work);
                if sealed.is_empty() {
                    return Ok(());
                }
                auth.push_work(key, &PushWork { working, waiting, sealed }).await
            }
        }
    }
}

/// The alert sealed for each phone that wants its kind.
fn seal_alert(targets: &[PushTarget], alert: Alert) -> Vec<SealedAlert> {
    let (thread_id, kind) = (alert.thread_id.clone(), alert.kind);
    let pushed = Pushed::Alert(alert);
    let wanting = targets.iter().filter(|target| target.choices.allows(kind));
    wanting
        .filter_map(|target| {
            let sealed = seal_pushed(&target.device, &pushed)?;
            Some(SealedAlert { device: target.device.clone(), thread_id: thread_id.clone(), kind, sealed })
        })
        .collect()
}

/// The work sealed for each phone that shows it.
fn seal_work(targets: &[PushTarget], work: ServerWork) -> Vec<SealedFor> {
    let pushed = Pushed::Work(work);
    let showing = targets.iter().filter(|target| target.live);
    showing
        .filter_map(|target| {
            let sealed = seal_pushed(&target.device, &pushed)?;
            Some(SealedFor { device: target.device.clone(), sealed })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use motile_protocol::alerts::{AlertChoices, PushKey};
    use motile_protocol::identity::DeviceKey;
    use motile_protocol::wire::ThreadParent;

    use super::*;

    fn thread(id: &str) -> Thread {
        serde_json::from_value(serde_json::json!({
            "id": id, "title": format!("Thread {id}"), "project_id": "p", "cwd": "/tmp", "agent": "claude",
            "model": null, "effort": null, "access": "full", "created_at": 0.0, "updated_at": 0.0, "done_at": null,
            "running": false, "needs_approval": false, "turn_ended_at": 10.0, "rev": 0
        }))
        .unwrap()
    }

    fn running(id: &str) -> Thread {
        Thread { running: true, ..thread(id) }
    }

    fn asking(id: &str, ask: Ask) -> Thread {
        Thread { needs_approval: true, asking: Some(ask), ..running(id) }
    }

    fn ended_at(id: &str, at: f64) -> Thread {
        Thread { turn_ended_at: Some(at), ..thread(id) }
    }

    fn child_of(parent: &str, thread: Thread) -> Thread {
        Thread { parent: Some(ThreadParent { thread_id: parent.into(), tool_id: None }), ..thread }
    }

    fn limit() -> Option<Interruption> {
        Some(Interruption::Limit { resets_at: None, continues: false })
    }

    fn calls(prior: &Thread, next: &Thread) -> Vec<Call> {
        observe(Some(prior), next, prior.turn_ended_at, 100.0).calls
    }

    #[test]
    fn a_thread_seen_first_or_archived_never_alerts() {
        let first = observe(None, &asking("t", Ask::Approval), None, 100.0);
        assert_eq!(first, Observed { calls: vec![], handled: None });
        let first_ended = observe(None, &ended_at("t", 50.0), Some(10.0), 100.0);
        assert_eq!(first_ended, Observed { calls: vec![], handled: Some(50.0) }, "its end is let pass");

        let archived = Thread { archived_at: Some(1.0), ..ended_at("t", 50.0) };
        let observed = observe(Some(&running("t")), &archived, Some(10.0), 100.0);
        assert_eq!(observed, Observed { calls: vec![], handled: Some(50.0) });
        assert_eq!(calls(&running("t"), &Thread { archived_at: Some(1.0), ..asking("t", Ask::Approval) }), vec![]);
    }

    #[test]
    fn waiting_for_the_user_alerts_once_for_each_ask() {
        assert_eq!(calls(&running("t"), &asking("t", Ask::Approval)), vec![Call::Waits(AlertKind::Approval)]);
        assert_eq!(calls(&running("t"), &asking("t", Ask::Question)), vec![Call::Waits(AlertKind::Question)]);
        assert_eq!(calls(&asking("t", Ask::Question), &asking("t", Ask::Plan)), vec![Call::Waits(AlertKind::Plan)]);
        assert_eq!(calls(&asking("t", Ask::Approval), &asking("t", Ask::Approval)), vec![]);
        let unasked = Thread { asking: None, ..asking("t", Ask::Approval) };
        assert_eq!(calls(&running("t"), &unasked), vec![Call::Waits(AlertKind::Approval)]);

        let snoozed = Thread { snoozed_until: Some(1000.0), ..asking("t", Ask::Approval) };
        assert_eq!(calls(&running("t"), &snoozed), vec![Call::Waits(AlertKind::Approval)], "a snooze lets it through");
    }

    #[test]
    fn reaching_the_usage_limit_alerts_once() {
        let limited = Thread { interruption: limit(), ..running("t") };
        assert_eq!(calls(&running("t"), &limited), vec![Call::Limit]);
        assert_eq!(calls(&limited, &limited), vec![]);
        let restarted = Thread { interruption: Some(Interruption::Restart), ..running("t") };
        assert_eq!(calls(&running("t"), &restarted), vec![]);
    }

    #[test]
    fn a_turn_that_ends_alerts_once_the_thread_settles() {
        assert_eq!(calls(&running("t"), &ended_at("t", 50.0)), vec![Call::Ended { at: 50.0, after: Some(10.0) }]);

        let monitoring = Thread { monitoring: true, ..ended_at("t", 50.0) };
        let observed = observe(Some(&running("t")), &monitoring, Some(10.0), 100.0);
        assert_eq!(observed, Observed { calls: vec![], handled: Some(10.0) }, "it still watches");
        let observed = observe(Some(&monitoring), &ended_at("t", 50.0), Some(10.0), 100.0);
        assert_eq!(observed.calls, vec![Call::Ended { at: 50.0, after: Some(10.0) }]);

        let observed = observe(Some(&ended_at("t", 50.0)), &ended_at("t", 50.0), Some(50.0), 100.0);
        assert_eq!(observed, Observed { calls: vec![], handled: Some(50.0) }, "that end was alerted for");
    }

    #[test]
    fn a_snoozed_thread_lets_its_end_pass() {
        let snoozed = Thread { snoozed_until: Some(1000.0), ..ended_at("t", 50.0) };
        let observed = observe(Some(&running("t")), &snoozed, Some(10.0), 100.0);
        assert_eq!(observed, Observed { calls: vec![], handled: Some(50.0) });

        let snoozed_then = Thread { snoozed_until: Some(60.0), ..ended_at("t", 50.0) };
        assert_eq!(observe(Some(&running("t")), &snoozed_then, Some(10.0), 100.0).calls, vec![]);
        let woke_before = Thread { snoozed_until: Some(40.0), ..ended_at("t", 50.0) };
        assert_eq!(observe(Some(&running("t")), &woke_before, Some(10.0), 100.0).calls.len(), 1);
    }

    #[test]
    fn a_thread_an_agent_started_only_alerts_for_what_it_waits_for() {
        let child = |thread| child_of("parent", thread);
        assert_eq!(
            calls(&child(running("c")), &child(asking("c", Ask::Question))),
            vec![Call::Waits(AlertKind::Question)]
        );
        assert_eq!(calls(&child(running("c")), &child(ended_at("c", 50.0))), vec![]);
        let limited = Thread { interruption: limit(), ..running("c") };
        assert_eq!(calls(&child(running("c")), &child(limited)), vec![]);

        let top = thread("top");
        let middle = child_of("top", thread("middle"));
        let bottom = child_of("middle", thread("bottom"));
        let threads: HashMap<&str, &Thread> = [("top", &top), ("middle", &middle)].into();
        assert_eq!(top_of(&bottom, |id| threads.get(id).copied()).id, "top");
        assert_eq!(top_of(&top, |id| threads.get(id).copied()).id, "top");
        let orphan = child_of("gone", thread("orphan"));
        assert_eq!(top_of(&orphan, |id| threads.get(id).copied()).id, "orphan", "it alerts for itself");
    }

    #[test]
    fn the_work_shows_the_waiting_then_the_working_then_the_ended_top_threads() {
        let long = Thread { title: "A".repeat(80), ..running("long") };
        let threads = [
            asking("waits", Ask::Approval),
            running("works"),
            long,
            ended_at("finished", 50.0),
            child_of("works", running("child")),
            Thread { archived_at: Some(1.0), ..running("archived") },
            thread("idle"),
        ];
        let mut states = HashMap::new();
        for (index, thread) in threads.iter().enumerate() {
            note_state(&mut states, thread, index as f64);
        }
        states.insert("finished".into(), (WorkState::Finished, 50.0));
        let projects = HashMap::from([("p".to_string(), "motile".to_string())]);
        let items =
            ServerWork::new(String::new(), String::new(), work_items(threads.iter(), &states, &projects), 0.0).items;
        let shown: Vec<(&str, WorkState)> = items.iter().map(|item| (item.thread_id.as_str(), item.state)).collect();
        assert_eq!(
            shown,
            [
                ("waits", WorkState::Waiting),
                ("long", WorkState::Working),
                ("works", WorkState::Working),
                ("finished", WorkState::Finished),
            ]
        );
        assert_eq!(items[1].title.chars().count(), WORK_TITLE_CHARS);
        assert!(items[1].title.ends_with('…'));
        assert_eq!(items[0].project, "motile");

        let many: Vec<Thread> = (0..10).map(|index| running(&index.to_string())).collect();
        for thread in &many {
            note_state(&mut states, thread, 1.0);
        }
        let work = ServerWork::new(String::new(), String::new(), work_items(many.iter(), &states, &projects), 0.0);
        assert_eq!((work.items.len(), work.working), (motile_protocol::alerts::WORK_ITEMS, 10));
    }

    #[test]
    fn a_thread_keeps_its_state_since_it_came_to_it() {
        let mut states = HashMap::new();
        note_state(&mut states, &running("t"), 1.0);
        note_state(&mut states, &running("t"), 2.0);
        assert_eq!(states["t"], (WorkState::Working, 1.0));
        note_state(&mut states, &asking("t", Ask::Approval), 3.0);
        assert_eq!(states["t"], (WorkState::Waiting, 3.0));
        note_state(&mut states, &thread("t"), 4.0);
        assert!(!states.contains_key("t"), "it rests");
        states.insert("t".into(), (WorkState::Finished, 5.0));
        note_state(&mut states, &thread("t"), 6.0);
        assert_eq!(states["t"], (WorkState::Finished, 5.0), "its end stays");
    }

    #[test]
    fn an_alert_is_sealed_for_each_phone_that_wants_it() {
        let (phone, quiet, other) = (DeviceKey::generate(), DeviceKey::generate(), DeviceKey::generate());
        let target = |key: &DeviceKey, choices, live| PushTarget { device: key.public(), choices, live };
        let targets = [
            target(&phone, AlertChoices::default(), true),
            target(&quiet, AlertChoices { finished: false, ..AlertChoices::default() }, false),
        ];
        let new = NewAlert {
            server_id: "s".into(),
            thread_id: "t".into(),
            kind: AlertKind::Finished,
            thread_title: "Fix the login".into(),
            project: "motile".into(),
            detail: None,
        };
        let alert = Alert::new(new, "a".into(), 1.0);
        let sealed = seal_alert(&targets, alert.clone());
        assert_eq!(sealed.len(), 1);
        assert_eq!((sealed[0].device.as_str(), sealed[0].thread_id.as_str()), (phone.public().as_str(), "t"));
        assert_eq!(PushKey::of_device(&phone).open_pushed(&sealed[0].sealed), Some(Pushed::Alert(alert)));
        assert_eq!(PushKey::of_device(&other).open(&sealed[0].sealed), None);

        let work = ServerWork::new("s".into(), "studio".into(), vec![], 1.0);
        let sealed = seal_work(&targets, work.clone());
        assert_eq!(sealed.len(), 1, "only the phone that shows the work");
        assert_eq!(PushKey::of_device(&phone).open_pushed(&sealed[0].sealed), Some(Pushed::Work(work)));
    }

    #[test]
    fn a_long_title_is_cut() {
        assert_eq!(cut("Fix the login", 60), "Fix the login");
        assert_eq!(cut("abcdef", 4), "abc…");
        assert_eq!(cut("abcd", 4), "abcd");
    }
}
