//! Carries the account's threads between its servers, so that every server keeps a copy of every
//! other server's. A round asks each connected server how far its own log and its copies go, then
//! moves what each one lacks, batch by batch, from the origin itself or, for a removed server, from
//! the server whose copy goes furthest. The servers only keep what they don't have yet, so two
//! clients relaying at once, or a round cut off halfway, leave every copy whole.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use motile_protocol::wire::{ChangeBatch, Message, Request, Watermark};

use crate::connection::Connection;
use crate::link::Link;

/// The first protocol whose servers keep copies of each other's threads.
pub const SYNC_PROTOCOL: u32 = 30;
/// How long a server may take to answer one request of a round.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(60);

/// Something that answers requests to a server.
pub trait Ask: Clone + Send + Sync + 'static {
    fn ask(&self, request: Request) -> impl Future<Output = anyhow::Result<Message>> + Send;
}

impl Ask for Arc<Link> {
    async fn ask(&self, request: Request) -> anyhow::Result<Message> {
        self.request(&request).await
    }
}

impl Ask for Connection {
    async fn ask(&self, request: Request) -> anyhow::Result<Message> {
        match self.request(&request).await? {
            Message::Error { message } => Err(anyhow::anyhow!(message)),
            message => Ok(message),
        }
    }
}

/// A server of the account, by its name and platform, as the copies of it are labelled.
#[derive(Clone, Debug, PartialEq)]
pub struct Origin {
    pub name: String,
    pub platform: String,
}

/// How far a server's own log goes, and its copies of the others'.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    pub own: Watermark,
    pub copies: HashMap<String, Watermark>,
}

/// One server's log carried to another, starting after `after`.
#[derive(Clone, Debug, PartialEq)]
pub struct Transfer {
    pub origin: String,
    /// The server the batches come from: the origin itself, or another that has a copy.
    pub source: String,
    pub target: String,
    pub after: u64,
    /// How far the log went when the round started.
    pub until: u64,
    /// The target's copy is made again: the origin's log went back.
    pub reset: bool,
}

impl Transfer {
    fn is_from_origin(&self) -> bool {
        self.source == self.origin
    }
}

/// What a round did: how far every server's logs went after it, which origins' copies changed,
/// and why it couldn't copy to a server, by server id.
#[derive(Debug, Default)]
pub struct Round {
    pub marks: HashMap<String, Marks>,
    pub changed: HashSet<String>,
    pub failed: HashMap<String, Failure>,
}

/// Why a round couldn't copy to a server: the origin it was copying, if it got that far.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub origin: Option<String>,
    pub error: String,
}

/// What a round says while it runs.
#[derive(Clone, Debug)]
pub enum Progress {
    /// What each server lacks, once the connected ones have said how far their logs go.
    Planned(Vec<Transfer>),
    /// A batch of `origin`'s log reached `target`.
    Carried { origin: String, target: String, mark: Watermark },
}

/// The server whose copy of `origin` goes furthest, among the ones in `marks`.
pub fn best_holder(marks: &HashMap<String, Marks>, origin: &str) -> Option<String> {
    let copies = marks.iter().filter_map(|(server, marks)| Some((server, marks.copies.get(origin)?)));
    copies.max_by(|a, b| a.1.change.cmp(&b.1.change).then_with(|| b.0.cmp(a.0))).map(|(server, _)| server.clone())
}

/// What each server lacks of the `origins`' logs, and where it comes from. On a `metered` network
/// a copy isn't started from the beginning, which can be large; it waits for another network.
pub fn plan(marks: &HashMap<String, Marks>, origins: &[String], metered: bool) -> Vec<Transfer> {
    let mut targets: Vec<&String> = marks.keys().collect();
    targets.sort();
    let mut transfers = Vec::new();
    for origin in origins {
        let (source, log) = match marks.get(origin) {
            Some(own) => (origin.clone(), &own.own),
            None => {
                let Some(holder) = best_holder(marks, origin) else { continue };
                let log = &marks[&holder].copies[origin];
                (holder, log)
            }
        };
        for target in targets.iter().filter(|target| **target != origin && **target != &source) {
            let copy = marks[*target].copies.get(origin);
            let from_origin = &source == origin;
            let (after, reset) = match copy {
                None => (0, false),
                Some(copy) if copy.log_id != log.log_id && from_origin => (0, false),
                Some(copy) if copy.log_id != log.log_id => continue,
                Some(copy) if copy.change > log.change && from_origin => (0, true),
                Some(copy) if copy.change >= log.change => continue,
                Some(copy) => (copy.change, false),
            };
            if metered && after == 0 {
                continue;
            }
            transfers.push(Transfer {
                origin: origin.clone(),
                source: source.clone(),
                target: (*target).clone(),
                after,
                until: log.change,
                reset,
            });
        }
    }
    transfers
}

async fn ask<P: Ask>(peer: &P, request: Request) -> anyhow::Result<Message> {
    tokio::time::timeout(ANSWER_TIMEOUT, peer.ask(request)).await.map_err(|_| anyhow::anyhow!("no answer in time"))?
}

async fn marks_of<P: Ask>(peer: &P) -> anyhow::Result<Marks> {
    let Message::Watermarks { own, copies } = ask(peer, Request::Watermarks).await? else {
        anyhow::bail!("the server didn't say how far its logs go");
    };
    let copies = copies.into_iter().map(|copy| (copy.origin, copy.watermark)).collect();
    Ok(Marks { own, copies })
}

async fn batch<P: Ask>(source: &P, transfer: &Transfer, after: u64) -> anyhow::Result<ChangeBatch> {
    let origin = (!transfer.is_from_origin()).then(|| transfer.origin.clone());
    let Message::Changes { batch } = ask(source, Request::Changes { origin, after }).await? else {
        anyhow::bail!("the server didn't answer with its changes");
    };
    Ok(batch)
}

/// Carries one log to one server, fetching each batch while the one before is kept. Says where
/// the target's copy ended, if it changed.
async fn carry<P: Ask>(
    source: &P,
    target: &P,
    transfer: &Transfer,
    origin: &Origin,
    report: &(dyn Fn(Progress) + Send + Sync),
) -> anyhow::Result<Option<Watermark>> {
    let mut next = batch(source, transfer, transfer.after).await?;
    let mut reset = transfer.reset;
    let mut reached = None;
    loop {
        let current = next;
        if current.up_to <= current.after && !reset {
            return Ok(reached);
        }
        let mark = Watermark { log_id: current.log_id.clone(), change: current.up_to };
        let apply = Request::Apply {
            origin: transfer.origin.clone(),
            name: origin.name.clone(),
            platform: origin.platform.clone(),
            batch: current.clone(),
            reset,
        };
        let more = current.more;
        let fetched = if more {
            let (applied, fetched) = tokio::join!(ask(target, apply), batch(source, transfer, current.up_to));
            applied?;
            Some(fetched)
        } else {
            ask(target, apply).await?;
            None
        };
        report(Progress::Carried {
            origin: transfer.origin.clone(),
            target: transfer.target.clone(),
            mark: mark.clone(),
        });
        reached = Some(mark);
        let Some(fetched) = fetched else { return Ok(reached) };
        next = fetched?;
        reset = false;
    }
}

/// Runs a round between the connected `peers`, by server id, for the account's `origins`, telling
/// `report` what it plans and each batch it carries.
pub async fn round<P: Ask>(
    peers: HashMap<String, P>,
    origins: HashMap<String, Origin>,
    metered: bool,
    report: impl Fn(Progress) + Send + Sync + 'static,
) -> Round {
    let mut asked = tokio::task::JoinSet::new();
    for (server_id, peer) in &peers {
        let (server_id, peer) = (server_id.clone(), peer.clone());
        asked.spawn(async move { (server_id, marks_of(&peer).await) });
    }
    let mut marks = HashMap::new();
    let mut failed = HashMap::new();
    while let Some(Ok((server_id, answer))) = asked.join_next().await {
        match answer {
            Ok(found) => drop(marks.insert(server_id, found)),
            Err(error) => {
                tracing::debug!(server_id, "couldn't read how far a server's logs go: {error:#}");
                failed.insert(server_id, Failure { origin: None, error: motile_protocol::error_text(&error) });
            }
        }
    }
    let mut names: Vec<String> = origins.keys().cloned().collect();
    names.sort();
    let planned = plan(&marks, &names, metered);
    report(Progress::Planned(planned.clone()));
    let mut by_target: HashMap<String, Vec<Transfer>> = HashMap::new();
    for transfer in planned {
        by_target.entry(transfer.target.clone()).or_default().push(transfer);
    }
    let peers = Arc::new(peers);
    let origins = Arc::new(origins);
    let report = Arc::new(report);
    let mut carried = tokio::task::JoinSet::new();
    for (target, transfers) in by_target {
        let (peers, origins, report) = (peers.clone(), origins.clone(), report.clone());
        carried.spawn(async move {
            let mut reached = Vec::new();
            let mut failure = None;
            for transfer in transfers {
                let (Some(source), Some(target), Some(origin)) =
                    (peers.get(&transfer.source), peers.get(&target), origins.get(&transfer.origin))
                else {
                    continue;
                };
                match carry(source, target, &transfer, origin, report.as_ref()).await {
                    Ok(Some(mark)) => reached.push((transfer.origin.clone(), mark)),
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(origin = transfer.origin, "couldn't copy a server's threads: {error:#}");
                        let error = motile_protocol::error_text(&error);
                        failure = Some(Failure { origin: Some(transfer.origin.clone()), error });
                    }
                }
            }
            (target, reached, failure)
        });
    }
    let mut changed = HashSet::new();
    while let Some(Ok((target, reached, failure))) = carried.join_next().await {
        for (origin, mark) in reached {
            if let Some(marks) = marks.get_mut(&target) {
                marks.copies.insert(origin.clone(), mark);
            }
            changed.insert(origin);
        }
        if let Some(failure) = failure {
            failed.insert(target, failure);
        }
    }
    Round { marks, changed, failed }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(log: &str, change: u64) -> Watermark {
        Watermark { log_id: log.to_string(), change }
    }

    fn server(own: Watermark, copies: &[(&str, Watermark)]) -> Marks {
        Marks { own, copies: copies.iter().map(|(origin, mark)| (origin.to_string(), mark.clone())).collect() }
    }

    fn transfers(plan: &[Transfer]) -> Vec<(&str, &str, &str, u64, bool)> {
        let mut found: Vec<_> = plan
            .iter()
            .map(|transfer| {
                (
                    transfer.origin.as_str(),
                    transfer.source.as_str(),
                    transfer.target.as_str(),
                    transfer.after,
                    transfer.reset,
                )
            })
            .collect();
        found.sort();
        found
    }

    fn origins(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn each_server_gets_what_it_lacks_of_the_others_from_the_others_themselves() {
        let marks = HashMap::from([
            ("a".to_string(), server(mark("la", 10), &[("b", mark("lb", 4))])),
            ("b".to_string(), server(mark("lb", 7), &[("a", mark("la", 10))])),
        ]);

        let plan = plan(&marks, &origins(&["a", "b"]), false);

        assert_eq!(transfers(&plan), vec![("b", "b", "a", 4, false)]);
    }

    #[test]
    fn a_removed_servers_copy_comes_from_the_server_whose_copy_goes_furthest() {
        let marks = HashMap::from([
            ("a".to_string(), server(mark("la", 1), &[("gone", mark("lg", 50))])),
            ("b".to_string(), server(mark("lb", 1), &[("a", mark("la", 1)), ("gone", mark("lg", 20))])),
            ("c".to_string(), server(mark("lc", 1), &[("a", mark("la", 1)), ("b", mark("lb", 1))])),
        ]);

        let plan = plan(&marks, &origins(&["a", "b", "c", "gone"]), false);

        assert_eq!(
            transfers(&plan),
            vec![
                ("b", "b", "a", 0, false),
                ("c", "c", "a", 0, false),
                ("c", "c", "b", 0, false),
                ("gone", "a", "b", 20, false),
                ("gone", "a", "c", 0, false),
            ]
        );
    }

    #[test]
    fn a_copy_of_another_log_is_made_again_and_one_ahead_of_its_origin_is_reset() {
        let marks = HashMap::from([
            ("a".to_string(), server(mark("new", 5), &[])),
            ("b".to_string(), server(mark("lb", 1), &[("a", mark("old", 9))])),
            ("c".to_string(), server(mark("lc", 1), &[("a", mark("new", 8))])),
        ]);

        let plan = plan(&marks, &origins(&["a"]), false);

        assert_eq!(transfers(&plan), vec![("a", "a", "b", 0, false), ("a", "a", "c", 0, true)]);
    }

    #[test]
    fn a_holder_with_another_log_than_the_best_copy_keeps_its_own() {
        let marks = HashMap::from([
            ("a".to_string(), server(mark("la", 1), &[("gone", mark("one", 9))])),
            ("b".to_string(), server(mark("lb", 1), &[("gone", mark("two", 3))])),
        ]);

        let plan = plan(&marks, &origins(&["gone"]), false);

        assert!(plan.is_empty());
    }

    #[test]
    fn on_a_metered_network_only_what_follows_a_copy_is_carried() {
        let marks = HashMap::from([
            ("a".to_string(), server(mark("la", 10), &[])),
            ("b".to_string(), server(mark("lb", 7), &[("a", mark("la", 4))])),
        ]);

        let plan = plan(&marks, &origins(&["a", "b"]), true);

        assert_eq!(transfers(&plan), vec![("a", "a", "b", 4, false)]);
    }

    #[test]
    fn an_origin_nobody_has_or_that_is_up_to_date_is_left_alone() {
        let marks = HashMap::from([
            ("a".to_string(), server(mark("la", 3), &[("b", mark("lb", 2))])),
            ("b".to_string(), server(mark("lb", 2), &[("a", mark("la", 3))])),
        ]);

        assert!(plan(&marks, &origins(&["a", "b", "elsewhere"]), false).is_empty());
    }
}
