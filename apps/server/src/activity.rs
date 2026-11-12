//! What the user and the agents did, kept for the usage view's Activity tab: the clients that
//! connected, each turn once it ended, and each feature used. The rows go into the change log, so
//! every server ends up with the account's whole activity, as it does with the usage.

use std::collections::HashSet;

use motile_protocol::activity::{self as features, Use};
use motile_protocol::wire::{
    Access, ActionBucket, ActionRecord, ActivityReport, Actor, ActorKind, Agent, CONTINUE_PROMPT, ClientForm,
    ClientInfo, ClientKind, ClientRecord, ConnectionPath, EngagedRecord, EngagedSpans, HourBucket, Item, ItemKind,
    Thread, TurnBucket, TurnOutcome, TurnRecord, TurnStarter,
};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};

use crate::hub::Hub;
use crate::store::{Store, StoredThread, as_text, from_text};

pub(crate) const CLIENT_COLUMNS: &str = "device, kind, system, form, version, seen_at";
pub(crate) const TURN_COLUMNS: &str = "id, thread_id, project_id, agent, agent_account, model, effort, access, starter, \
    device, started_at, ended_at, outcome, waited_secs, asks, steers, tools, agents, subagents, output_tokens, \
    reasoning_tokens, fast";
pub(crate) const ACTION_COLUMNS: &str = "at, actor, device, feature, variant, thread_id, project_id, path, rtt_ms";
pub(crate) const ENGAGED_COLUMNS: &str = "device, project_id, started_at, ended_at";
/// The longest stretch a client is believed, and how far ahead of this server's clock one may end.
const LONGEST_STRETCH: f64 = 86_400.0;
const CLOCK_SKEW: f64 = 300.0;

/// Claude Code's tool that runs a subagent inside its own session, by its old name and its new.
const SUBAGENT_TOOLS: [&str; 2] = ["Task", "Agent"];
const CHILD_TOOLS: [&str; 2] = ["mcp__motile__start_agent", "mcp__motile__create_thread"];

/// What a turn did while it ran, until it ends and is kept as a `TurnRecord`.
#[derive(Clone, Debug)]
pub struct Tally {
    starter: TurnStarter,
    device: Option<String>,
    started_at: f64,
    waiting_since: Option<f64>,
    waited_secs: f64,
    asks: u32,
    steers: u32,
    tools: HashSet<String>,
    subagents: HashSet<String>,
    agents: u32,
    output_tokens: u64,
    reasoning_tokens: u64,
    fast: Option<bool>,
}

impl Tally {
    pub fn new(starter: TurnStarter, device: Option<String>, started_at: f64) -> Self {
        Self {
            starter,
            device,
            started_at,
            waiting_since: None,
            waited_secs: 0.0,
            asks: 0,
            steers: 0,
            tools: HashSet::new(),
            subagents: HashSet::new(),
            agents: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
            fast: None,
        }
    }

    /// Counts a tool call of the turn's agent once, however often it is told of. `true` the first time.
    pub fn tool(&mut self, id: &str, name: &str) -> bool {
        if !self.tools.insert(id.to_string()) {
            return false;
        }
        if SUBAGENT_TOOLS.contains(&name) {
            self.subagents.insert(id.to_string());
        }
        if CHILD_TOOLS.contains(&name) {
            self.agents += 1;
        }
        true
    }

    pub fn subagent(&mut self, tool_id: &str) {
        self.subagents.insert(tool_id.to_string());
    }

    pub fn asked(&mut self) {
        self.asks += 1;
    }

    pub fn steered(&mut self) {
        self.steers += 1;
    }

    pub fn spent(&mut self, output: u64) {
        self.output_tokens += output;
    }

    pub fn reasoned(&mut self, tokens: u64) {
        self.reasoning_tokens += tokens;
    }

    pub fn fast(&mut self, fast: bool) {
        self.fast = Some(fast);
    }

    /// Notes whether the turn waits for the user now.
    pub fn waits(&mut self, waiting: bool, at: f64) {
        match (waiting, self.waiting_since) {
            (true, None) => self.waiting_since = Some(at),
            (false, Some(since)) => {
                self.waited_secs += (at - since).max(0.0);
                self.waiting_since = None;
            }
            _ => {}
        }
    }

    pub fn record(
        mut self,
        id: String,
        thread: &Thread,
        model: &str,
        outcome: TurnOutcome,
        ended_at: f64,
    ) -> TurnRecord {
        self.waits(false, ended_at);
        TurnRecord {
            id,
            thread_id: thread.id.clone(),
            project_id: thread.project_id.clone(),
            agent: thread.agent,
            agent_account: thread.agent_account.clone(),
            model: model.to_string(),
            effort: thread.effort.clone().unwrap_or_default(),
            access: thread.access,
            starter: self.starter,
            device: self.device,
            started_at: self.started_at.min(ended_at),
            ended_at,
            outcome,
            waited_secs: self.waited_secs,
            asks: self.asks,
            steers: self.steers,
            tools: self.tools.len() as u32,
            agents: self.agents,
            subagents: self.subagents.len() as u32,
            output_tokens: self.output_tokens,
            reasoning_tokens: self.reasoning_tokens,
            fast: self.fast,
        }
    }
}

/// Who used a feature from a client: its device, and how it was connected.
#[derive(Clone, Debug)]
pub struct Sender {
    pub device: String,
    pub path: Option<ConnectionPath>,
    pub rtt_ms: Option<u32>,
}

/// Keeps the features a client used. A use outside a thread names the thread given, if any.
pub async fn keep_uses(hub: &Hub, sender: &Sender, uses: Vec<Use>, thread_id: Option<&str>) {
    for used in uses {
        let thread_id = used.thread_id.or_else(|| thread_id.map(str::to_string)).unwrap_or_default();
        let project_id = match used.project_id {
            Some(project_id) => project_id,
            None if thread_id.is_empty() => String::new(),
            None => hub.thread(&thread_id).await.map(|thread| thread.project_id).unwrap_or_default(),
        };
        let action = ActionRecord {
            at: motile_protocol::now(),
            actor: ActorKind::User,
            device: Some(sender.device.clone()),
            feature: used.feature.to_string(),
            variant: used.variant,
            thread_id,
            project_id,
            path: sender.path,
            rtt_ms: sender.rtt_ms,
        };
        if let Err(error) = hub.store().save_action(&action) {
            tracing::warn!(feature = action.feature, "couldn't keep a feature's use: {error:#}");
        }
    }
}

/// Keeps the features a client used with a message to `thread_id`, in `project_id`.
pub fn keep_used(store: &Store, sender: &Sender, uses: Vec<Use>, thread_id: &str, project_id: &str) {
    for used in uses {
        let action = ActionRecord {
            at: motile_protocol::now(),
            actor: ActorKind::User,
            device: Some(sender.device.clone()),
            feature: used.feature.to_string(),
            variant: used.variant,
            thread_id: used.thread_id.unwrap_or_else(|| thread_id.to_string()),
            project_id: used.project_id.unwrap_or_else(|| project_id.to_string()),
            path: sender.path,
            rtt_ms: sender.rtt_ms,
        };
        if let Err(error) = store.save_action(&action) {
            tracing::warn!(feature = action.feature, "couldn't keep a feature's use: {error:#}");
        }
    }
}

pub(crate) fn client_values(record: &ClientRecord) -> Vec<Value> {
    let client = &record.client;
    vec![
        record.device.clone().into(),
        as_text(&client.kind).into(),
        client.system.clone().into(),
        as_text(&client.form).into(),
        client.version.clone().into(),
        record.seen_at.into(),
    ]
}

pub(crate) fn client_record(row: &Row, first: usize) -> rusqlite::Result<ClientRecord> {
    Ok(ClientRecord {
        device: row.get(first)?,
        client: ClientInfo {
            kind: from_text(&row.get::<_, String>(first + 1)?).unwrap_or(ClientKind::Other),
            system: row.get(first + 2)?,
            form: from_text(&row.get::<_, String>(first + 3)?).unwrap_or(ClientForm::Other),
            version: row.get(first + 4)?,
        },
        seen_at: row.get(first + 5)?,
        connected: false,
    })
}

pub(crate) fn turn_values(turn: &TurnRecord) -> Vec<Value> {
    vec![
        turn.id.clone().into(),
        turn.thread_id.clone().into(),
        turn.project_id.clone().into(),
        as_text(&turn.agent).into(),
        turn.agent_account.clone().into(),
        turn.model.clone().into(),
        turn.effort.clone().into(),
        as_text(&turn.access).into(),
        as_text(&turn.starter).into(),
        turn.device.clone().into(),
        turn.started_at.into(),
        turn.ended_at.into(),
        as_text(&turn.outcome).into(),
        turn.waited_secs.into(),
        i64::from(turn.asks).into(),
        i64::from(turn.steers).into(),
        i64::from(turn.tools).into(),
        i64::from(turn.agents).into(),
        i64::from(turn.subagents).into(),
        (turn.output_tokens as i64).into(),
        (turn.reasoning_tokens as i64).into(),
        turn.fast.map(i64::from).into(),
    ]
}

pub(crate) fn turn_record(row: &Row, first: usize) -> rusqlite::Result<TurnRecord> {
    let text = |index: usize| row.get::<_, String>(first + index);
    Ok(TurnRecord {
        id: text(0)?,
        thread_id: text(1)?,
        project_id: text(2)?,
        agent: from_text(&text(3)?).unwrap_or(Agent::Claude),
        agent_account: text(4)?,
        model: text(5)?,
        effort: text(6)?,
        access: from_text(&text(7)?).unwrap_or(Access::Full),
        starter: from_text(&text(8)?).unwrap_or(TurnStarter::Other),
        device: row.get(first + 9)?,
        started_at: row.get(first + 10)?,
        ended_at: row.get(first + 11)?,
        outcome: from_text(&text(12)?).unwrap_or(TurnOutcome::Other),
        waited_secs: row.get(first + 13)?,
        asks: row.get(first + 14)?,
        steers: row.get(first + 15)?,
        tools: row.get(first + 16)?,
        agents: row.get(first + 17)?,
        subagents: row.get(first + 18)?,
        output_tokens: row.get::<_, i64>(first + 19)? as u64,
        reasoning_tokens: row.get::<_, i64>(first + 20)? as u64,
        fast: row.get::<_, Option<i64>>(first + 21)?.map(|fast| fast != 0),
    })
}

pub(crate) fn action_values(action: &ActionRecord) -> Vec<Value> {
    vec![
        action.at.into(),
        as_text(&action.actor).into(),
        action.device.clone().into(),
        action.feature.clone().into(),
        action.variant.clone().into(),
        action.thread_id.clone().into(),
        action.project_id.clone().into(),
        action.path.as_ref().map(as_text).into(),
        action.rtt_ms.map(i64::from).into(),
    ]
}

pub(crate) fn action_record(row: &Row, first: usize) -> rusqlite::Result<ActionRecord> {
    Ok(ActionRecord {
        at: row.get(first)?,
        actor: from_text(&row.get::<_, String>(first + 1)?).unwrap_or(ActorKind::Other),
        device: row.get(first + 2)?,
        feature: row.get(first + 3)?,
        variant: row.get(first + 4)?,
        thread_id: row.get(first + 5)?,
        project_id: row.get(first + 6)?,
        path: row.get::<_, Option<String>>(first + 7)?.and_then(|path| from_text::<ConnectionPath>(&path)),
        rtt_ms: row.get(first + 8)?,
    })
}

pub(crate) fn engaged_values(stretch: &EngagedRecord) -> Vec<Value> {
    vec![
        stretch.device.clone().into(),
        stretch.project_id.clone().into(),
        stretch.started_at.into(),
        stretch.ended_at.into(),
    ]
}

pub(crate) fn engaged_record(row: &Row, first: usize) -> rusqlite::Result<EngagedRecord> {
    Ok(EngagedRecord {
        device: row.get(first)?,
        project_id: row.get(first + 1)?,
        started_at: row.get(first + 2)?,
        ended_at: row.get(first + 3)?,
    })
}

/// `?, ?, …` for `count` values after `skip` that came before.
pub(crate) fn placeholders(skip: usize, count: usize) -> String {
    (skip + 1..=skip + count).map(|index| format!("?{index}")).collect::<Vec<_>>().join(", ")
}

impl Store {
    /// Keeps which client the device is, as it just said.
    pub fn save_client(&self, record: &ClientRecord) -> rusqlite::Result<()> {
        let connection = self.connection();
        let known: Option<ClientRecord> = connection
            .query_row(&format!("SELECT {CLIENT_COLUMNS} FROM clients WHERE device = ?1"), [&record.device], |row| {
                client_record(row, 0)
            })
            .ok();
        // A client that is as it was is only logged again once a day, so that every connection isn't a change.
        if known.is_some_and(|known| known.client == record.client && record.seen_at - known.seen_at < 86_400.0) {
            return Ok(());
        }
        connection.execute(
            &format!(
                "INSERT INTO clients ({CLIENT_COLUMNS}) VALUES ({}) ON CONFLICT (device) DO UPDATE SET
                     kind = excluded.kind, system = excluded.system, form = excluded.form,
                     version = excluded.version, seen_at = excluded.seen_at",
                placeholders(0, 6)
            ),
            params_from_iter(client_values(record)),
        )?;
        Ok(())
    }

    /// Keeps when the device left, unless it was seen in the last minute.
    pub fn client_left(&self, device: &str, at: f64) -> rusqlite::Result<()> {
        self.connection()
            .execute("UPDATE clients SET seen_at = ?2 WHERE device = ?1 AND seen_at < ?2 - 60", params![device, at])?;
        Ok(())
    }

    pub fn save_turn(&self, turn: &TurnRecord) -> rusqlite::Result<()> {
        self.connection().execute(
            &format!("INSERT OR IGNORE INTO turns ({TURN_COLUMNS}) VALUES ({})", placeholders(0, 22)),
            params_from_iter(turn_values(turn)),
        )?;
        Ok(())
    }

    pub fn save_action(&self, action: &ActionRecord) -> rusqlite::Result<()> {
        self.connection().execute(
            &format!("INSERT INTO actions ({ACTION_COLUMNS}) VALUES ({})", placeholders(0, 9)),
            params_from_iter(action_values(action)),
        )?;
        Ok(())
    }

    /// Keeps the stretches a client spent as `device`'s, skipping those it has and those no client
    /// could have spent: empty, longer than a day, or ending in the future.
    pub fn save_engaged(&self, device: &str, stretches: &[EngagedRecord]) -> rusqlite::Result<()> {
        let latest = motile_protocol::now() + CLOCK_SKEW;
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        for stretch in stretches {
            let length = stretch.ended_at - stretch.started_at;
            if !(length > 0.0 && length <= LONGEST_STRETCH && stretch.ended_at <= latest) {
                continue;
            }
            let stretch = EngagedRecord { device: device.to_string(), ..stretch.clone() };
            transaction.execute(
                &format!("INSERT OR IGNORE INTO engaged ({ENGAGED_COLUMNS}) VALUES ({})", placeholders(0, 4)),
                params_from_iter(engaged_values(&stretch)),
            )?;
        }
        transaction.commit()
    }

    /// What was done from `since` until `until`, in buckets of `bucket_secs` that start where a
    /// clock `utc_offset_secs` ahead of UTC starts them: this server's own, or with `origin` its copy
    /// of that server's. The clients are every one the server knows of either way.
    pub fn activity_of(
        &self,
        origin: Option<&str>,
        since: f64,
        until: f64,
        bucket_secs: u32,
        utc_offset_secs: i32,
    ) -> rusqlite::Result<ActivityReport> {
        let connection = self.reader();
        let (turns, actions, agent_threads) = match origin {
            Some(_) => (
                "SELECT * FROM copied_turns WHERE origin = ?5",
                "SELECT * FROM copied_actions WHERE origin = ?5",
                "SELECT id FROM copied_threads WHERE origin = ?5 AND json_extract(payload, '$.parent') IS NOT NULL",
            ),
            None => (
                "SELECT *, ?5 AS origin FROM turns",
                "SELECT *, ?5 AS origin FROM actions",
                "SELECT id FROM threads WHERE parent_thread IS NOT NULL OR started_by IS NOT NULL",
            ),
        };
        let turns = format!("(SELECT *, thread_id IN ({agent_threads}) AS agent_thread FROM ({turns}))");
        // A message without a client in a thread an agent started was read from its transcript, and the agent sent it.
        let actions = format!(
            "(SELECT * FROM ({actions}) WHERE NOT (actor = 'user' AND device IS NULL AND thread_id IN ({agent_threads})))"
        );
        let (size, offset) = (f64::from(bucket_secs.max(60)), f64::from(utc_offset_secs));
        let origin = origin.unwrap_or_default();
        let start = |bucket: i64| bucket as f64 * size - offset;
        let turn_buckets = {
            let mut statement = connection.prepare(&format!(
                "SELECT CAST((ended_at + ?3) / ?4 AS INTEGER) AS bucket, device, starter, agent, agent_account, model,
                        project_id, outcome, COALESCE(fast, 0) AS fast, COUNT(*), SUM(ended_at - started_at),
                        SUM(waited_secs), SUM(asks), SUM(steers), SUM(tools), SUM(agents), SUM(subagents),
                        SUM(output_tokens), SUM(reasoning_tokens), agent_thread
                 FROM {turns} WHERE ended_at >= ?1 AND ended_at < ?2
                 GROUP BY bucket, device, starter, agent, agent_account, model, project_id, outcome, fast,
                          agent_thread
                 ORDER BY bucket"
            ))?;
            let rows = statement.query_map(params![since, until, offset, size, origin], |row| {
                let text = |index: usize| row.get::<_, String>(index);
                Ok(TurnBucket {
                    start: start(row.get(0)?),
                    device: row.get(1)?,
                    starter: from_text(&text(2)?).unwrap_or(TurnStarter::Other),
                    agent: from_text(&text(3)?).unwrap_or(Agent::Claude),
                    agent_account: text(4)?,
                    model: text(5)?,
                    project_id: text(6)?,
                    outcome: from_text(&text(7)?).unwrap_or(TurnOutcome::Other),
                    fast: row.get::<_, i64>(8)? != 0,
                    turns: row.get(9)?,
                    seconds: row.get(10)?,
                    waited_secs: row.get(11)?,
                    asks: row.get(12)?,
                    steers: row.get(13)?,
                    tools: row.get(14)?,
                    agents: row.get(15)?,
                    subagents: row.get(16)?,
                    output_tokens: row.get::<_, i64>(17)? as u64,
                    reasoning_tokens: row.get::<_, i64>(18)? as u64,
                    agent_thread: row.get(19)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let action_buckets = {
            let mut statement = connection.prepare(&format!(
                "SELECT CAST((at + ?3) / ?4 AS INTEGER) AS bucket, actor, device, feature, variant, COUNT(*),
                        SUM(path = 'direct'), SUM(path = 'relay'), COALESCE(SUM(rtt_ms), 0), COUNT(rtt_ms)
                 FROM {actions} WHERE at >= ?1 AND at < ?2
                 GROUP BY bucket, actor, device, feature, variant ORDER BY bucket"
            ))?;
            let rows = statement.query_map(params![since, until, offset, size, origin], |row| {
                Ok(ActionBucket {
                    start: start(row.get(0)?),
                    actor: from_text(&row.get::<_, String>(1)?).unwrap_or(ActorKind::Other),
                    device: row.get(2)?,
                    feature: row.get(3)?,
                    variant: row.get(4)?,
                    count: row.get(5)?,
                    direct: row.get::<_, Option<u32>>(6)?.unwrap_or_default(),
                    relay: row.get::<_, Option<u32>>(7)?.unwrap_or_default(),
                    rtt_ms: row.get::<_, i64>(8)? as u64,
                    timed: row.get(9)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let hours = {
            let mut statement = connection.prepare(&format!(
                "SELECT hour, SUM(messages), SUM(turns), SUM(seconds) FROM (
                     SELECT CAST((at + ?3) / 3600 AS INTEGER) AS hour, 1 AS messages, 0 AS turns, 0.0 AS seconds
                     FROM {actions} WHERE feature = 'message' AND actor = 'user' AND at >= ?1 AND at < ?2
                     UNION ALL
                     SELECT CAST((started_at + ?3) / 3600 AS INTEGER), 0, 1, ended_at - started_at
                     FROM {turns} WHERE ended_at >= ?1 AND ended_at < ?2
                 ) WHERE ?4 > 0 GROUP BY hour ORDER BY hour"
            ))?;
            let rows = statement.query_map(params![since, until, offset, size, origin], |row| {
                let hour: i64 = row.get(0)?;
                Ok(HourBucket {
                    start: Some(hour as f64 * 3600.0 - offset),
                    // The first of January 1970 was a Thursday.
                    weekday: (hour.div_euclid(24) + 3).rem_euclid(7) as u8,
                    hour: hour.rem_euclid(24) as u8,
                    messages: row.get(1)?,
                    turns: row.get(2)?,
                    seconds: row.get(3)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let engaged_table = match origin.is_empty() {
            true => "(SELECT *, ?3 AS origin FROM engaged)",
            false => "(SELECT * FROM copied_engaged WHERE origin = ?3)",
        };
        let mut engaged: Vec<EngagedSpans> = Vec::new();
        {
            let mut statement = connection.prepare(&format!(
                "SELECT device, project_id, started_at, ended_at FROM {engaged_table}
                 WHERE started_at >= ?1 - {LONGEST_STRETCH} AND started_at < ?2 AND ended_at > ?1
                 ORDER BY device, project_id, started_at"
            ))?;
            let rows = statement.query_map(params![since, until, origin], |row| engaged_record(row, 0))?;
            for stretch in rows {
                let stretch = stretch?;
                let offset = |at: f64| (at.clamp(since, until) - since).round() as u32;
                let span = [offset(stretch.started_at), offset(stretch.ended_at)];
                if span[1] <= span[0] {
                    continue;
                }
                match engaged.last_mut() {
                    Some(last) if last.device == stretch.device && last.project_id == stretch.project_id => {
                        last.spans.push(span)
                    }
                    _ => engaged.push(EngagedSpans {
                        device: stretch.device,
                        project_id: stretch.project_id,
                        spans: vec![span],
                    }),
                }
            }
        }
        let engaged_since: Option<f64> = connection.query_row(
            &format!("SELECT MIN(started_at) FROM {engaged_table}"),
            params![since, until, origin],
            |row| row.get(0),
        )?;
        Ok(ActivityReport {
            turns: turn_buckets,
            actions: action_buckets,
            hours,
            clients: known_clients(&connection)?,
            engaged,
            engaged_since,
        })
    }
}

/// The setting that says the turns and messages from before activity was kept have been read from
/// the transcripts.
const BACKFILLED: &str = "activity_backfilled";

/// Reads the turns and messages from before this server kept its activity out of its threads'
/// transcripts, once, in the background. Which client sent a message, how long a turn waited and
/// which features were used were never kept, so those start with this version.
pub fn backfill_soon(hub: &std::sync::Arc<Hub>) {
    if hub.store().setting(BACKFILLED).is_some() {
        return;
    }
    let hub = hub.clone();
    tokio::task::spawn_blocking(move || match hub.store().backfill_activity() {
        Ok((turns, messages)) => tracing::info!(turns, messages, "read the activity from before it was kept"),
        Err(error) => tracing::error!("couldn't read the activity from before it was kept: {error:#}"),
    });
}

/// The turns and the user's messages a thread's transcript shows. A turn is kept by the id of the
/// item that ended it, as one that is kept live is, so reading it again changes nothing.
pub fn from_transcript(stored: &StoredThread, items: &[Item]) -> (Vec<TurnRecord>, Vec<ActionRecord>) {
    let thread = &stored.thread;
    let own: Vec<&Item> = items.iter().filter(|item| item.parent.is_none()).collect();
    let mut agent = own
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Handoff { from, .. } => Some(from.agent),
            _ => None,
        })
        .unwrap_or(thread.agent);
    let mut model: Option<String> = None;
    let (mut turns, mut messages) = (Vec::new(), Vec::new());
    let mut open: Option<Derived> = None;
    let mut first_message = true;
    let agent_thread = stored.started_by.is_some() || thread.parent.is_some();
    let action = |item: &Item, device: &Option<String>, feature: &str, variant: &str| ActionRecord {
        at: item.created_at,
        actor: ActorKind::User,
        device: device.clone(),
        feature: feature.to_string(),
        variant: variant.to_string(),
        thread_id: thread.id.clone(),
        project_id: thread.project_id.clone(),
        path: None,
        rtt_ms: None,
    };
    for item in own {
        match &item.kind {
            ItemKind::User { text, context, by, .. } => {
                let continued = text == CONTINUE_PROMPT;
                // Messages from before senders were kept are told apart as well as they can be.
                let starter = match by {
                    Actor::User { .. } => TurnStarter::User,
                    Actor::Agent { .. } => TurnStarter::Agent,
                    Actor::Server { .. } => TurnStarter::Server,
                    Actor::Unknown if agent_thread => TurnStarter::Agent,
                    Actor::Unknown if continued => TurnStarter::Server,
                    Actor::Unknown => TurnStarter::User,
                };
                let device = by.device_key();
                match &mut open {
                    Some(turn) => turn.steers += 1,
                    None => open = Some(Derived { device: device.clone(), ..Derived::new(starter, item.created_at) }),
                }
                if starter == TurnStarter::User && !continued {
                    let variant = match (first_message, context.is_some()) {
                        (true, _) => "new",
                        (false, true) => "program",
                        (false, false) => "reply",
                    };
                    messages.push(action(item, &device, features::MESSAGE, variant));
                    if first_message {
                        let kind = if stored.worktree.is_some() { "worktree" } else { "local" };
                        messages.push(action(item, &device, "thread.new", kind));
                    }
                    if context.is_some() {
                        messages.push(action(item, &device, "program.message", ""));
                    }
                }
                first_message = false;
            }
            ItemKind::Notice { .. } => {
                if open.is_none() {
                    open = Some(Derived::new(TurnStarter::Notice, item.created_at));
                }
            }
            ItemKind::Tool { call } => {
                let turn = open.get_or_insert_with(|| Derived::new(TurnStarter::Server, item.created_at));
                turn.tools.insert(call.id.clone());
                if SUBAGENT_TOOLS.contains(&call.name.as_str()) {
                    turn.subagents += 1;
                }
                if CHILD_TOOLS.contains(&call.name.as_str()) {
                    turn.agents += 1;
                }
            }
            ItemKind::Error { message } => {
                if let Some(turn) = &mut open {
                    turn.errors.push(message.to_lowercase());
                }
            }
            ItemKind::Handoff { to, .. } => {
                agent = to.agent;
                model = to.model.clone();
            }
            ItemKind::TurnEnd { summary } => {
                let Some(turn) = open.take() else { continue };
                let ended_at = item.created_at;
                let started_at = summary
                    .duration_ms
                    .map(|ms| ended_at - ms as f64 / 1000.0)
                    .unwrap_or(turn.started_at)
                    .min(ended_at);
                let outcome = match () {
                    _ if summary.stopped => TurnOutcome::Stopped,
                    _ if turn.errors.iter().any(|error| error.contains("restarted")) => TurnOutcome::Restart,
                    _ if turn.errors.iter().any(|error| error.contains("limit")) => TurnOutcome::Limit,
                    _ if summary.is_error => TurnOutcome::Failed,
                    _ => TurnOutcome::Done,
                };
                turns.push(TurnRecord {
                    id: item.id.clone(),
                    thread_id: thread.id.clone(),
                    project_id: thread.project_id.clone(),
                    agent,
                    agent_account: if agent == thread.agent { thread.agent_account.clone() } else { String::new() },
                    model: model.clone().or_else(|| thread.model.clone()).unwrap_or_default(),
                    effort: thread.effort.clone().unwrap_or_default(),
                    access: thread.access,
                    starter: turn.starter,
                    device: turn.device,
                    started_at,
                    ended_at,
                    outcome,
                    waited_secs: 0.0,
                    asks: 0,
                    steers: turn.steers,
                    tools: turn.tools.len() as u32,
                    agents: turn.agents,
                    subagents: turn.subagents,
                    output_tokens: 0,
                    reasoning_tokens: 0,
                    fast: None,
                });
            }
            _ => {}
        }
    }
    (turns, messages)
}

/// A turn as its transcript shows it, until the item that ends it.
struct Derived {
    starter: TurnStarter,
    device: Option<String>,
    started_at: f64,
    steers: u32,
    tools: HashSet<String>,
    agents: u32,
    subagents: u32,
    errors: Vec<String>,
}

impl Derived {
    fn new(starter: TurnStarter, started_at: f64) -> Self {
        Self {
            starter,
            device: None,
            started_at,
            steers: 0,
            tools: HashSet::new(),
            agents: 0,
            subagents: 0,
            errors: Vec::new(),
        }
    }
}

impl Store {
    /// Reads every thread's turns and the user's messages from its transcript into the activity,
    /// with the model and the output tokens the usage kept for each turn. Messages from when the
    /// activity was already kept are left out, so none is counted twice.
    pub fn backfill_activity(&self) -> rusqlite::Result<(usize, usize)> {
        let kept_since: f64 = self
            .connection()
            .query_row("SELECT MIN(at) FROM actions WHERE feature = ?1", [features::MESSAGE], |row| {
                row.get::<_, Option<f64>>(0)
            })?
            .unwrap_or_else(motile_protocol::now);
        let (mut turns, mut messages) = (0, 0);
        for stored in self.load_threads()? {
            let items = self.items_since(&stored.thread.id, 0)?;
            let (derived, sent) = from_transcript(&stored, &items);
            let mut connection = self.connection();
            let transaction = connection.transaction()?;
            for mut turn in derived {
                let spent: Option<(String, i64)> = transaction
                    .query_row(
                        "SELECT model, SUM(output) FROM usage
                         WHERE thread_id = ?1 AND purpose = 'turn' AND at >= ?2 AND at <= ?3 + 5
                         GROUP BY model ORDER BY SUM(output) DESC LIMIT 1",
                        params![turn.thread_id, turn.started_at, turn.ended_at],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;
                if let Some((model, output)) = spent {
                    turn.model = model;
                    turn.output_tokens = output.max(0) as u64;
                }
                turns += transaction.execute(
                    &format!("INSERT OR IGNORE INTO turns ({TURN_COLUMNS}) VALUES ({})", placeholders(0, 22)),
                    params_from_iter(turn_values(&turn)),
                )?;
            }
            for action in sent.iter().filter(|action| action.at < kept_since) {
                messages += transaction.execute(
                    &format!("INSERT INTO actions ({ACTION_COLUMNS}) VALUES ({})", placeholders(0, 9)),
                    params_from_iter(action_values(action)),
                )?;
            }
            transaction.commit()?;
        }
        self.set_setting(BACKFILLED, Some("1"))?;
        Ok((turns, messages))
    }
}

/// The clients this server and its copies know of, each once, as it was seen last.
fn known_clients(connection: &Connection) -> rusqlite::Result<Vec<ClientRecord>> {
    let mut statement = connection.prepare(&format!(
        "SELECT {CLIENT_COLUMNS} FROM (
             SELECT {CLIENT_COLUMNS} FROM clients UNION ALL SELECT {CLIENT_COLUMNS} FROM copied_clients
         ) GROUP BY device HAVING seen_at = MAX(seen_at)"
    ))?;
    let rows = statement.query_map([], |row| client_record(row, 0))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::Access;

    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let folder = tempfile::tempdir().unwrap();
        let store = Store::open(&folder.path().join("motile.db")).unwrap();
        (folder, store)
    }

    fn thread() -> Thread {
        serde_json::from_value(serde_json::json!({
            "id": "t", "title": "T", "project_id": "p", "cwd": "/tmp", "agent": "claude", "agent_account": "a",
            "model": null, "effort": "high", "access": "full", "created_at": 0.0, "updated_at": 0.0, "done_at": null,
            "running": false, "needs_approval": false, "turn_ended_at": null, "rev": 0
        }))
        .unwrap()
    }

    fn action(
        at: f64,
        feature: &str,
        device: Option<&str>,
        path: Option<ConnectionPath>,
        rtt_ms: Option<u32>,
    ) -> ActionRecord {
        ActionRecord {
            at,
            actor: if device.is_some() { ActorKind::User } else { ActorKind::Agent },
            device: device.map(str::to_string),
            feature: feature.into(),
            variant: String::new(),
            thread_id: "t".into(),
            project_id: "p".into(),
            path,
            rtt_ms,
        }
    }

    #[test]
    fn a_tally_counts_each_tool_once_and_the_time_spent_waiting() {
        let mut tally = Tally::new(TurnStarter::User, Some("mac".into()), 100.0);
        assert!(tally.tool("1", "Bash"));
        assert!(!tally.tool("1", "Bash"));
        tally.tool("2", "Task");
        tally.tool("3", "mcp__motile__start_agent");
        tally.asked();
        tally.waits(true, 110.0);
        tally.waits(true, 115.0);
        tally.waits(false, 130.0);
        tally.waits(true, 140.0);

        let turn = tally.record("end".into(), &thread(), "opus", TurnOutcome::Done, 150.0);

        assert_eq!((turn.tools, turn.subagents, turn.agents, turn.asks), (3, 1, 1, 1));
        assert_eq!(turn.waited_secs, 30.0);
        assert_eq!((turn.started_at, turn.ended_at, turn.effort.as_str()), (100.0, 150.0, "high"));
        assert_eq!(turn.access, Access::Full);
    }

    #[test]
    fn activity_is_added_up_by_bucket_and_hour_on_the_asking_clock() {
        let (_folder, store) = store();
        let day = 86_400.0;
        let mut first = Tally::new(TurnStarter::User, Some("mac".into()), 3600.0);
        first.tool("1", "Bash");
        store.save_turn(&first.record("a".into(), &thread(), "opus", TurnOutcome::Done, 3660.0)).unwrap();
        let second = Tally::new(TurnStarter::User, Some("mac".into()), 3700.0);
        store.save_turn(&second.record("b".into(), &thread(), "opus", TurnOutcome::Done, 3760.0)).unwrap();
        let later = Tally::new(TurnStarter::Agent, None, day + 10.0);
        store.save_turn(&later.record("c".into(), &thread(), "opus", TurnOutcome::Stopped, day + 20.0)).unwrap();
        store.save_action(&action(3600.0, "message", Some("mac"), Some(ConnectionPath::Direct), Some(20))).unwrap();
        store.save_action(&action(3610.0, "message", Some("mac"), Some(ConnectionPath::Relay), Some(80))).unwrap();
        store.save_action(&action(3620.0, "agent.start_agent", None, None, None)).unwrap();

        // Two hours ahead of UTC, the first day's hour 1 is hour 3, and 1 January 1970 was a Thursday.
        let report = store.activity_of(None, 0.0, 2.0 * day, 86_400, 7200).unwrap();

        let turns: Vec<_> =
            report.turns.iter().map(|bucket| (bucket.start, bucket.turns, bucket.tools, bucket.seconds)).collect();
        assert_eq!(turns, [(-7200.0, 2, 1, 120.0), (day - 7200.0, 1, 0, 10.0)]);
        let message = report.actions.iter().find(|bucket| bucket.feature == "message").unwrap();
        assert_eq!((message.count, message.direct, message.relay, message.rtt_ms, message.timed), (2, 1, 1, 100, 2));
        let hours: Vec<_> =
            report.hours.iter().map(|hour| (hour.start, hour.weekday, hour.hour, hour.messages, hour.turns)).collect();
        assert_eq!(hours, [(Some(3600.0), 3, 3, 2, 2), (Some(day), 4, 2, 0, 1)]);
    }

    fn item(seq: u64, at: f64, kind: serde_json::Value) -> Item {
        let mut item = serde_json::json!({"id": format!("i{seq}"), "seq": seq, "rev": seq + 1, "created_at": at});
        item.as_object_mut().unwrap().extend(kind.as_object().unwrap().clone());
        serde_json::from_value(item).unwrap()
    }

    fn tool(seq: u64, at: f64, name: &str) -> Item {
        item(
            seq,
            at,
            serde_json::json!({"type": "tool", "call": {"id": format!("c{seq}"), "name": name, "input": "{}",
            "output": null, "status": "succeeded"}}),
        )
    }

    fn turn_end(seq: u64, at: f64, stopped: bool) -> Item {
        item(
            seq,
            at,
            serde_json::json!({"type": "turn_end", "summary": {"duration_ms": 30_000, "cost_usd": null,
            "is_error": false, "stopped": stopped}}),
        )
    }

    fn user(seq: u64, at: f64, text: &str) -> Item {
        item(seq, at, serde_json::json!({"type": "user", "text": text, "attachments": []}))
    }

    fn stored(started_by: Option<&str>) -> StoredThread {
        StoredThread {
            thread: thread(),
            session_id: None,
            title_source: crate::store::TitleSource::User,
            next_seq: 0,
            worktree: None,
            reports: false,
            started_by: started_by.map(str::to_string),
        }
    }

    #[test]
    fn a_transcript_gives_its_turns_and_the_users_messages() {
        let items = [
            user(0, 100.0, "Add a rate limiter"),
            tool(1, 110.0, "Bash"),
            tool(2, 115.0, "mcp__motile__start_agent"),
            user(3, 120.0, "Use Redis"),
            turn_end(4, 140.0, false),
            user(5, 200.0, CONTINUE_PROMPT),
            tool(6, 210.0, "Task"),
            turn_end(7, 240.0, true),
        ];

        let (turns, messages) = from_transcript(&stored(None), &items);

        let shown: Vec<_> = turns
            .iter()
            .map(|turn| {
                (
                    turn.id.as_str(),
                    turn.starter,
                    turn.outcome,
                    turn.started_at,
                    turn.tools,
                    turn.agents,
                    turn.subagents,
                    turn.steers,
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("i4", TurnStarter::User, TurnOutcome::Done, 110.0, 2, 1, 0, 1),
                ("i7", TurnStarter::Server, TurnOutcome::Stopped, 210.0, 1, 0, 1, 0),
            ]
        );
        let sent: Vec<_> =
            messages.iter().map(|action| (action.feature.as_str(), action.variant.as_str(), action.at)).collect();
        assert_eq!(sent, [("message", "new", 100.0), ("thread.new", "local", 100.0), ("message", "reply", 120.0)]);
    }

    #[test]
    fn the_messages_in_an_agents_thread_are_the_agents_not_the_users() {
        let items = [
            user(0, 100.0, "Look over greet.py"),
            turn_end(1, 130.0, false),
            user(2, 200.0, "Now the tests"),
            turn_end(3, 230.0, false),
        ];
        let (turns, messages) = from_transcript(&stored(Some("parent")), &items);
        let starters: Vec<_> = turns.iter().map(|turn| turn.starter).collect();
        assert_eq!(starters, [TurnStarter::Agent, TurnStarter::Agent]);
        assert!(messages.is_empty());
    }

    #[test]
    fn a_report_tells_a_subagents_turns_apart_and_leaves_out_what_its_agent_sent_it() {
        let (_folder, store) = store();
        let connection = store.connection();
        connection
            .execute_batch(
                "INSERT INTO threads (id, title, title_source, project_id, cwd, agent, access, created_at, updated_at)
                 VALUES ('t', 'T', 'user', 'p', '/tmp', 'claude', 'full', 0, 1);
                 INSERT INTO threads (id, title, title_source, project_id, cwd, agent, access, created_at, updated_at,
                                      started_by)
                 VALUES ('child', 'C', 'user', 'p', '/tmp', 'claude', 'full', 0, 1, 't');",
            )
            .unwrap();
        drop(connection);
        let in_thread = |id: &str| Thread { id: id.into(), ..thread() };
        for (id, thread_id) in [("a", "t"), ("b", "child")] {
            let tally = Tally::new(TurnStarter::User, None, 100.0);
            store.save_turn(&tally.record(id.into(), &in_thread(thread_id), "opus", TurnOutcome::Done, 160.0)).unwrap();
        }
        for (thread_id, device) in [("t", None), ("child", None), ("child", Some("mac"))] {
            let action = ActionRecord {
                actor: ActorKind::User,
                thread_id: thread_id.into(),
                ..action(100.0, "message", device, None, None)
            };
            store.save_action(&action).unwrap();
        }

        let report = store.activity_of(None, 0.0, 3600.0, 3600, 0).unwrap();

        let turns: Vec<_> = report.turns.iter().map(|bucket| (bucket.agent_thread, bucket.turns)).collect();
        assert_eq!(turns, [(false, 1), (true, 1)]);
        let messages: u32 = report.actions.iter().map(|bucket| bucket.count).sum();
        assert_eq!(messages, 2);
    }

    #[test]
    fn reading_the_transcripts_again_counts_nothing_twice() {
        let (_folder, store) = store();
        let connection = store.connection();
        connection
            .execute(
                "INSERT INTO threads (id, title, title_source, project_id, cwd, agent, access, created_at, updated_at)
                 VALUES ('t', 'T', 'user', 'p', '/tmp', 'claude', 'full', 0, 1)",
                [],
            )
            .unwrap();
        drop(connection);
        for item in [user(0, 100.0, "Hi"), turn_end(1, 130.0, false)] {
            store.save_item("t", &item).unwrap();
        }

        assert_eq!(store.backfill_activity().unwrap(), (1, 2));
        assert_eq!(store.backfill_activity().unwrap(), (0, 0));
        assert!(store.setting(BACKFILLED).is_some());
    }

    #[test]
    fn a_clients_stretches_are_kept_as_its_own_once_and_reported_cut_to_the_time_asked() {
        let (_folder, store) = store();
        let stretch = |project: &str, started_at: f64, ended_at: f64| EngagedRecord {
            device: "someone else".into(),
            project_id: project.into(),
            started_at,
            ended_at,
        };
        let sent = [
            stretch("p", 3_500.0, 3_700.0),
            stretch("p", 3_800.0, 3_900.0),
            stretch("", 4_000.0, 4_030.4),
            stretch("p", 5_000.0, 5_000.0),
            stretch("p", 6_000.0, 6_000.0 + 2.0 * 86_400.0),
            stretch("p", motile_protocol::now() + 3_600.0, motile_protocol::now() + 3_700.0),
        ];
        store.save_engaged("mac", &sent).unwrap();
        store.save_engaged("mac", &sent[..1]).unwrap();

        let report = store.activity_of(None, 3_600.0, 7_200.0, 3600, 0).unwrap();
        let spans: Vec<_> = report
            .engaged
            .iter()
            .map(|spans| (spans.device.as_str(), spans.project_id.as_str(), &spans.spans[..]))
            .collect();
        assert_eq!(spans, [("mac", "", &[[400, 430]][..]), ("mac", "p", &[[0, 100], [200, 300]][..])]);
        assert_eq!(report.engaged_since, Some(3_500.0));
    }

    #[test]
    fn a_client_is_kept_once_and_logged_again_when_it_changes() {
        let (_folder, store) = store();
        let client = |version: &str, seen_at: f64| ClientRecord {
            device: "mac".into(),
            client: ClientInfo {
                kind: ClientKind::Mac,
                system: "macos".into(),
                form: ClientForm::Desktop,
                version: version.into(),
            },
            seen_at,
            connected: false,
        };
        let change = || store.watermark().unwrap().change;
        store.save_client(&client("0.1.1", 10.0)).unwrap();
        let first = change();
        store.save_client(&client("0.1.1", 20.0)).unwrap();
        assert_eq!(change(), first);
        store.save_client(&client("0.1.2", 30.0)).unwrap();
        assert!(change() > first);

        let report = store.activity_of(None, 0.0, 1.0, 3600, 0).unwrap();
        assert_eq!(report.clients, [client("0.1.2", 30.0)]);

        store.client_left("mac", 60.0).unwrap();
        let report = store.activity_of(None, 0.0, 1.0, 3600, 0).unwrap();
        assert_eq!(report.clients, [client("0.1.2", 30.0)]);
        store.client_left("mac", 200.0).unwrap();
        let report = store.activity_of(None, 0.0, 1.0, 3600, 0).unwrap();
        assert_eq!(report.clients, [client("0.1.2", 200.0)]);
    }
}
