//! What the user and the agents did on the account's servers, for the usage view's Activity tab:
//! the headline numbers, the work over time by where it came from, the clients, the hours of the
//! week, how turns ended, and the features used, all ready to draw.

use std::collections::HashMap;

use motile_protocol::activity as features;
use motile_protocol::wire::{
    ActionBucket, ActivityReport, Actor, Agent, ClientForm, ClientKind, ClientRecord, TurnBucket, TurnOutcome,
    TurnStarter,
};
use serde::Serialize;

use crate::usage::{Window, agent_name};

const WEEKDAYS: [&str; 7] = ["Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays", "Sundays"];

/// What a server said was done: its own, or its copy of a removed server's.
pub struct Done {
    pub server_id: String,
    pub report: ActivityReport,
}

/// What the view names things by.
#[derive(Default)]
pub struct Names {
    /// The projects' names by server and project.
    pub projects: HashMap<(String, String), String>,
    pub servers: HashMap<String, String>,
    /// The account's devices' names and platforms by key.
    pub devices: HashMap<String, (String, String)>,
    /// This client's own device key.
    pub this_device: String,
}

#[derive(Serialize, Debug, Default, PartialEq)]
pub struct View {
    /// Nothing was done in the time asked about.
    pub empty: bool,
    pub starts: Vec<f64>,
    pub tiles: Vec<Tile>,
    /// Each with a point for each of `starts`: the user's messages by the kind of client they came
    /// from, then the turns of the agents and of the subagents, the agents that agents started.
    pub sources: Vec<Source>,
    pub clients: Vec<ClientRow>,
    /// The 168 hours of the week, Monday's first.
    pub hours: Vec<HourCell>,
    /// When the user sends the most messages, as a sentence.
    pub busiest: Option<String>,
    pub outcomes: Vec<Line>,
    pub agents: Vec<Line>,
    pub models: Vec<Line>,
    pub projects: Vec<Line>,
    pub facts: Vec<Fact>,
    /// The features the user used, by group.
    pub features: Vec<FeatureGroup>,
    /// The motile tools the agents called.
    pub agent_features: Vec<FeatureLine>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Tile {
    pub label: String,
    pub value: String,
    pub detail: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Source {
    pub id: String,
    pub label: String,
    /// One of the clients' symbols, by its name in `Symbol.swift`.
    pub symbol: &'static str,
    /// Which of the six chart colours, from 1, always the same for a kind of device; 0 is the muted
    /// colour of clients that didn't say which they are.
    pub color: u8,
    pub messages: u32,
    pub turns: u32,
    pub seconds: f64,
    pub message_points: Vec<u32>,
    pub turn_points: Vec<u32>,
    pub second_points: Vec<f64>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct ClientRow {
    pub device: String,
    pub name: String,
    /// The client and where it runs: "Mac app · macOS".
    pub client: String,
    /// Its version and how it reaches the servers.
    pub detail: String,
    pub symbol: &'static str,
    pub color: u8,
    pub messages: u32,
    pub turns: u32,
    /// Its part of the messages the clients listed sent, from 0 to 1.
    pub share: f64,
    pub last_seen: Option<f64>,
    pub this_device: bool,
    /// The device is no longer the account's.
    pub removed: bool,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct HourCell {
    pub weekday: u8,
    pub hour: u8,
    pub messages: u32,
    pub turns: u32,
    /// How busy it was next to the busiest hour, from 0, nothing, to 4.
    pub level: u8,
}

/// An agent, a model, a project or an outcome of turns and its part of them.
#[derive(Serialize, Debug, PartialEq)]
pub struct Line {
    pub name: String,
    pub agent: Option<Agent>,
    pub server: Option<String>,
    pub turns: u32,
    /// How long they worked, on average.
    pub detail: String,
    pub share: f64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Fact {
    pub label: String,
    pub value: String,
    pub detail: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct FeatureGroup {
    pub name: String,
    pub count: u32,
    pub features: Vec<FeatureLine>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct FeatureLine {
    pub id: String,
    pub label: String,
    pub count: u32,
    /// Its count next to the most used of its list, from 0 to 1.
    pub share: f64,
    /// How it was used, when it was used in more than one way.
    pub detail: String,
}

/// What a line of turns is named by, from its server and its turns: a name, an agent and a server.
type LineKey<'a> = dyn Fn(&str, &TurnBucket) -> (String, Option<Agent>, Option<String>) + 'a;
/// How a feature was used, each way with how many times.
type Variants = Vec<(String, u32)>;

#[derive(Default)]
struct Sums {
    turns: u32,
    seconds: f64,
}

impl Sums {
    fn add(&mut self, bucket: &TurnBucket) {
        self.turns += bucket.turns;
        self.seconds += bucket.seconds;
    }
}

/// Where a message came from or whose turn it was, as the chart groups them.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum From {
    Client(ClientKind, ClientForm, String),
    UnknownClient,
    Agents,
    Subagents,
}

impl From {
    fn of_device(device: Option<&str>, clients: &HashMap<String, ClientRecord>) -> Self {
        match device.and_then(|device| clients.get(device)) {
            Some(record) => From::Client(record.client.kind, record.client.form, system_of(record)),
            None => From::UnknownClient,
        }
    }

    fn of_turn(bucket: &TurnBucket) -> Self {
        if bucket.agent_thread { From::Subagents } else { From::Agents }
    }

    fn id(&self) -> String {
        match self {
            From::Client(kind, form, system) => format!("{kind:?}-{form:?}-{system}").to_lowercase(),
            From::UnknownClient => "unknown".into(),
            From::Agents => "agents".into(),
            From::Subagents => "subagents".into(),
        }
    }

    fn label(&self) -> String {
        match self {
            From::Client(kind, form, system) => device_label(*kind, *form, system).to_string(),
            From::UnknownClient => "Unknown client".into(),
            From::Agents => "Agents".into(),
            From::Subagents => "Subagents".into(),
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            From::Client(_, form, system) => form_symbol(*form, system),
            From::UnknownClient => "monitorSmartphone",
            From::Agents => "sparkles",
            From::Subagents => "bot",
        }
    }

    /// The chart colour, the same in every period: desktops blue, or pink off a Mac, phones green,
    /// tablets sky; the agents purple and the subagents orange.
    fn color(&self) -> u8 {
        match self {
            From::Client(ClientKind::Mac, _, _) => 1,
            From::Client(_, ClientForm::Phone, _) => 2,
            From::Client(_, ClientForm::Tablet, _) => 5,
            From::Client(ClientKind::Gpui, _, system) if system == "macos" => 1,
            From::Client(ClientKind::Other, _, _) | From::UnknownClient => 0,
            From::Client(..) => 6,
            From::Agents => 4,
            From::Subagents => 3,
        }
    }

    fn rank(&self) -> u8 {
        match self {
            From::Client(..) => 0,
            From::UnknownClient => 1,
            From::Agents => 2,
            From::Subagents => 3,
        }
    }
}

fn system_of(record: &ClientRecord) -> String {
    record.client.system.clone()
}

fn device_label(kind: ClientKind, form: ClientForm, system: &str) -> &'static str {
    match (kind, form, system) {
        (ClientKind::Mac, _, _) => "Mac",
        (ClientKind::Ios, ClientForm::Tablet, _) => "iPad",
        (ClientKind::Ios, _, _) => "iPhone",
        (ClientKind::Android, ClientForm::Tablet, _) => "Android tablet",
        (ClientKind::Android, _, _) => "Android phone",
        (ClientKind::Gpui, _, "windows") => "Windows",
        (ClientKind::Gpui, _, "macos") => "Mac (GPUI)",
        (ClientKind::Gpui, _, _) => "Linux",
        (ClientKind::Other, _, _) => "Other clients",
    }
}

fn client_label(kind: ClientKind) -> &'static str {
    match kind {
        ClientKind::Mac => "Mac app",
        ClientKind::Ios => "iOS app",
        ClientKind::Android => "Android app",
        ClientKind::Gpui => "GPUI app",
        ClientKind::Other => "Client",
    }
}

fn system_label(system: &str) -> &str {
    match system {
        "macos" => "macOS",
        "ios" => "iOS",
        "ipados" => "iPadOS",
        "android" => "Android",
        "linux" => "Linux",
        "windows" => "Windows",
        other => other,
    }
}

fn form_symbol(form: ClientForm, system: &str) -> &'static str {
    match (form, system) {
        (ClientForm::Phone, _) => "smartphone",
        (ClientForm::Tablet, _) => "tablet",
        (_, "macos") => "laptop",
        _ => "monitor",
    }
}

pub fn view(done: &[Done], window: Window, names: &Names) -> View {
    let starts = window.starts();
    let point = |start: f64| {
        let index = ((start - window.since) / f64::from(window.bucket_secs)).round();
        (index >= 0.0 && (index as usize) < starts.len()).then_some(index as usize)
    };
    let mut clients: HashMap<String, ClientRecord> = HashMap::new();
    for record in done.iter().flat_map(|done| &done.report.clients) {
        let newer = clients.get(&record.device).is_none_or(|known| known.seen_at < record.seen_at);
        if newer {
            clients.insert(record.device.clone(), record.clone());
        }
    }

    let turns: Vec<(&str, &TurnBucket)> = done
        .iter()
        .flat_map(|done| done.report.turns.iter().map(move |bucket| (done.server_id.as_str(), bucket)))
        .collect();
    let actions: Vec<&ActionBucket> = done.iter().flat_map(|done| &done.report.actions).collect();
    let messages: Vec<&&ActionBucket> =
        actions.iter().filter(|bucket| bucket.actor == Actor::User && bucket.feature == features::MESSAGE).collect();
    let message_count: u32 = messages.iter().map(|bucket| bucket.count).sum();
    let turn_count: u32 = turns.iter().map(|(_, bucket)| bucket.turns).sum();
    if message_count == 0 && turn_count == 0 && actions.is_empty() {
        return View { empty: true, starts, ..View::default() };
    }

    let mut sources: Vec<(From, Source)> = Vec::new();
    for bucket in &messages {
        let index = source_of(&mut sources, From::of_device(bucket.device.as_deref(), &clients), starts.len());
        let entry = &mut sources[index].1;
        entry.messages += bucket.count;
        if let Some(point) = point(bucket.start) {
            entry.message_points[point] += bucket.count;
        }
    }
    for (_, bucket) in &turns {
        let index = source_of(&mut sources, From::of_turn(bucket), starts.len());
        let entry = &mut sources[index].1;
        entry.turns += bucket.turns;
        entry.seconds += bucket.seconds;
        if let Some(point) = point(bucket.start) {
            entry.turn_points[point] += bucket.turns;
            entry.second_points[point] += bucket.seconds;
        }
    }
    sources.sort_by(|(a, x), (b, y)| {
        a.rank().cmp(&b.rank()).then((y.messages + y.turns).cmp(&(x.messages + x.turns))).then(x.label.cmp(&y.label))
    });

    let seconds: f64 = turns.iter().map(|(_, bucket)| bucket.seconds).sum();
    let waited: f64 = turns.iter().map(|(_, bucket)| bucket.waited_secs).sum();
    let asks: u32 = turns.iter().map(|(_, bucket)| bucket.asks).sum();
    let count_of = |outcome: TurnOutcome| -> u32 {
        turns.iter().filter(|(_, bucket)| bucket.outcome == outcome).map(|(_, bucket)| bucket.turns).sum()
    };
    let (finished, stopped, failed) =
        (count_of(TurnOutcome::Done), count_of(TurnOutcome::Stopped), count_of(TurnOutcome::Failed));
    let threads: u32 = actions.iter().filter(|bucket| bucket.feature == "thread.new").map(|bucket| bucket.count).sum();
    let tiles = vec![
        Tile {
            label: "Messages".into(),
            value: number(message_count),
            detail: plural(threads, "thread started", "threads started"),
        },
        Tile {
            label: "Agent time".into(),
            value: duration(seconds),
            detail: match turn_count {
                0 => "No turns".into(),
                count => {
                    format!("{} · {} on average", plural(count, "turn", "turns"), duration(seconds / f64::from(count)))
                }
            },
        },
        Tile {
            label: "Waiting on you".into(),
            value: duration(waited),
            detail: plural(asks, "approval or question", "approvals and questions"),
        },
        Tile {
            label: "Finished".into(),
            value: percent(f64::from(finished), f64::from(turn_count)),
            detail: format!("{} stopped · {} failed", number(stopped), number(failed)),
        },
    ];

    let client_rows = client_rows(&messages, &turns, &clients, names);
    let (hours, busiest) = hours(done);

    let lines = |key: &LineKey<'_>| {
        let mut found: Vec<(String, Option<Agent>, Option<String>, Sums)> = Vec::new();
        for (server_id, bucket) in &turns {
            let (name, agent, server) = key(server_id, bucket);
            let index = found.iter().position(|(known, known_agent, known_server, _)| {
                *known == name && *known_agent == agent && *known_server == server
            });
            let index = index.unwrap_or_else(|| {
                found.push((name, agent, server, Sums::default()));
                found.len() - 1
            });
            found[index].3.add(bucket);
        }
        let mut lines: Vec<Line> = found
            .into_iter()
            .map(|(name, agent, server, sums)| Line {
                name,
                agent,
                server,
                turns: sums.turns,
                detail: format!("{} on average", duration(sums.seconds / f64::from(sums.turns.max(1)))),
                share: f64::from(sums.turns) / f64::from(turn_count.max(1)),
            })
            .collect();
        lines.sort_by(|a, b| b.turns.cmp(&a.turns).then(a.name.cmp(&b.name)));
        lines
    };
    let several_servers = turns.iter().any(|(server, _)| *server != turns[0].0);
    let server_name =
        |server_id: &str| names.servers.get(server_id).cloned().unwrap_or_else(|| "Unknown server".into());
    let outcomes = lines(&|_, bucket| (outcome_label(bucket.outcome).into(), None, None));
    let agents = lines(&|_, bucket| (agent_name(bucket.agent).into(), Some(bucket.agent), None));
    let models = lines(&|_, bucket| {
        let model = if bucket.model.is_empty() { "Default model" } else { &bucket.model };
        (model.to_string(), Some(bucket.agent), None)
    });
    let projects = lines(&|server_id, bucket| {
        let project = names.projects.get(&(server_id.to_string(), bucket.project_id.clone()));
        let server = several_servers.then(|| server_name(server_id));
        (project.cloned().unwrap_or_else(|| "Removed project".into()), None, server)
    });

    View {
        empty: false,
        starts,
        tiles,
        sources: sources.into_iter().map(|(_, source)| source).collect(),
        clients: client_rows,
        hours,
        busiest,
        outcomes,
        agents,
        models,
        projects,
        facts: facts(&turns, &messages, turn_count),
        features: feature_groups(&actions),
        agent_features: feature_lines(actions.iter().copied().filter(|bucket| bucket.actor == Actor::Agent)),
    }
}

/// The index of the source `from` among `sources`, which it joins with `points` empty points if new.
fn source_of(sources: &mut Vec<(From, Source)>, from: From, points: usize) -> usize {
    if let Some(index) = sources.iter().position(|(known, _)| *known == from) {
        return index;
    }
    let source = Source {
        id: from.id(),
        label: from.label(),
        symbol: from.symbol(),
        color: from.color(),
        messages: 0,
        turns: 0,
        seconds: 0.0,
        message_points: vec![0; points],
        turn_points: vec![0; points],
        second_points: vec![0.0; points],
    };
    sources.push((from, source));
    sources.len() - 1
}

fn client_rows(
    messages: &[&&ActionBucket],
    turns: &[(&str, &TurnBucket)],
    clients: &HashMap<String, ClientRecord>,
    names: &Names,
) -> Vec<ClientRow> {
    #[derive(Default)]
    struct Seen {
        messages: u32,
        turns: u32,
        direct: u32,
        relay: u32,
        rtt_ms: u64,
        timed: u32,
    }
    let mut seen: HashMap<String, Seen> = HashMap::new();
    for bucket in messages {
        let Some(device) = &bucket.device else { continue };
        let entry = seen.entry(device.clone()).or_default();
        entry.messages += bucket.count;
        entry.direct += bucket.direct;
        entry.relay += bucket.relay;
        entry.rtt_ms += bucket.rtt_ms;
        entry.timed += bucket.timed;
    }
    for (_, bucket) in turns.iter().filter(|(_, bucket)| bucket.starter == TurnStarter::User) {
        let Some(device) = &bucket.device else { continue };
        seen.entry(device.clone()).or_default().turns += bucket.turns;
    }
    let listed: u32 = seen.values().map(|seen| seen.messages).sum();
    let mut rows: Vec<ClientRow> = seen
        .into_iter()
        .map(|(device, seen)| {
            let record = clients.get(&device);
            let account = names.devices.get(&device);
            let from = From::of_device(Some(&device), clients);
            let client = match record {
                Some(record) => {
                    format!("{} · {}", client_label(record.client.kind), system_label(&record.client.system))
                }
                None => {
                    account.map(|(_, platform)| system_label(platform).to_string()).unwrap_or_else(|| "Client".into())
                }
            };
            let mut detail = Vec::new();
            if let Some(record) = record {
                detail.push(format!("Version {}", record.client.version));
            }
            let reached = seen.direct + seen.relay;
            if reached > 0 {
                detail.push(match seen.relay {
                    0 => "Direct".to_string(),
                    relay if relay == reached => "Through a relay".to_string(),
                    relay => format!("{} through a relay", percent(f64::from(relay), f64::from(reached))),
                });
            }
            if seen.timed > 0 {
                detail.push(format!("{} ms", seen.rtt_ms / u64::from(seen.timed)));
            }
            ClientRow {
                name: account.map(|(name, _)| name.clone()).unwrap_or_else(|| from.label()),
                client,
                detail: detail.join(" · "),
                symbol: from.symbol(),
                color: from.color(),
                messages: seen.messages,
                turns: seen.turns,
                share: f64::from(seen.messages) / f64::from(listed.max(1)),
                last_seen: record.map(|record| record.seen_at),
                this_device: device == names.this_device,
                removed: account.is_none(),
                device,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.messages.cmp(&a.messages).then(b.turns.cmp(&a.turns)).then(a.name.cmp(&b.name)));
    rows
}

fn hours(done: &[Done]) -> (Vec<HourCell>, Option<String>) {
    let mut cells: Vec<HourCell> = (0..168)
        .map(|index| HourCell {
            weekday: (index / 24) as u8,
            hour: (index % 24) as u8,
            messages: 0,
            turns: 0,
            level: 0,
        })
        .collect();
    for hour in done.iter().flat_map(|done| &done.report.hours) {
        let Some(cell) = cells.get_mut(usize::from(hour.weekday) * 24 + usize::from(hour.hour)) else { continue };
        cell.messages += hour.messages;
        cell.turns += hour.turns;
    }
    let by_messages = cells.iter().any(|cell| cell.messages > 0);
    let weight = |cell: &HourCell| if by_messages { cell.messages } else { cell.turns };
    let most = cells.iter().map(weight).max().unwrap_or(0);
    for cell in &mut cells {
        let value = weight(cell);
        cell.level = match most {
            0 => 0,
            _ if value == 0 => 0,
            _ => (f64::from(value) / f64::from(most) * 4.0).ceil().clamp(1.0, 4.0) as u8,
        };
    }
    let busiest = cells
        .iter()
        .filter(|cell| weight(cell) > 0)
        .max_by_key(|cell| (weight(cell), std::cmp::Reverse(cell.weekday * 24 + cell.hour)));
    let sentence = busiest.map(|cell| {
        let what = if by_messages { "You send the most messages" } else { "Your agents work the most" };
        format!("{what} on {} around {}", WEEKDAYS[usize::from(cell.weekday)], clock(cell.hour))
    });
    (cells, sentence)
}

fn facts(turns: &[(&str, &TurnBucket)], messages: &[&&ActionBucket], turn_count: u32) -> Vec<Fact> {
    let sum = |value: fn(&TurnBucket) -> u64| -> u64 { turns.iter().map(|(_, bucket)| value(bucket)).sum() };
    let tools = sum(|bucket| u64::from(bucket.tools));
    let steers = sum(|bucket| u64::from(bucket.steers));
    let agents = sum(|bucket| u64::from(bucket.agents));
    let subagents = sum(|bucket| u64::from(bucket.subagents));
    let output = sum(|bucket| bucket.output_tokens);
    let reasoning = sum(|bucket| bucket.reasoning_tokens);
    let fast: u32 = turns.iter().filter(|(_, bucket)| bucket.fast).map(|(_, bucket)| bucket.turns).sum();
    let reached: u32 = messages.iter().map(|bucket| bucket.direct + bucket.relay).sum();
    let direct: u32 = messages.iter().map(|bucket| bucket.direct).sum();
    let timed: u32 = messages.iter().map(|bucket| bucket.timed).sum();
    let rtt: u64 = messages.iter().map(|bucket| bucket.rtt_ms).sum();
    let per_turn = |value: u64| format!("{:.1} a turn", value as f64 / f64::from(turn_count.max(1)));
    let mut facts = vec![Fact { label: "Tool calls".into(), value: number(tools as u32), detail: per_turn(tools) }];
    facts.push(Fact {
        label: "Steered".into(),
        value: number(steers as u32),
        detail: "Messages taken mid-turn".into(),
    });
    if agents + subagents > 0 {
        let detail = match (agents, subagents) {
            (0, _) => "Inside an agent's session".to_string(),
            (_, 0) => "In threads of their own".to_string(),
            _ => format!("{} in threads · {} inside a session", number(agents as u32), number(subagents as u32)),
        };
        facts.push(Fact { label: "Subagents".into(), value: number((agents + subagents) as u32), detail });
    }
    if fast > 0 {
        facts.push(Fact {
            label: "Fast mode".into(),
            value: plural(fast, "turn", "turns"),
            detail: format!("{} of turns", percent(f64::from(fast), f64::from(turn_count))),
        });
    }
    if reasoning > 0 {
        facts.push(Fact {
            label: "Reasoning".into(),
            value: percent(reasoning as f64, output.max(reasoning) as f64),
            detail: format!("{} of {} output tokens", abbreviated(reasoning), abbreviated(output.max(reasoning))),
        });
    }
    if reached > 0 {
        let detail = match timed {
            0 => "Of messages".to_string(),
            timed => format!("{} ms round trip on average", rtt / u64::from(timed)),
        };
        facts.push(Fact { label: "Direct".into(), value: percent(f64::from(direct), f64::from(reached)), detail });
    }
    facts
}

fn feature_groups(actions: &[&ActionBucket]) -> Vec<FeatureGroup> {
    let users: Vec<&ActionBucket> = actions.iter().copied().filter(|bucket| bucket.actor == Actor::User).collect();
    let lines = feature_lines(users.iter().copied());
    let mut groups: Vec<FeatureGroup> = Vec::new();
    for line in lines {
        let Some(feature) = features::find(&line.id) else { continue };
        let name = feature.group.label().to_string();
        let index = groups.iter().position(|group| group.name == name).unwrap_or_else(|| {
            groups.push(FeatureGroup { name, count: 0, features: Vec::new() });
            groups.len() - 1
        });
        groups[index].count += line.count;
        groups[index].features.push(line);
    }
    for group in &mut groups {
        let most = group.features.iter().map(|line| line.count).max().unwrap_or(1).max(1);
        group.features.iter_mut().for_each(|line| line.share = f64::from(line.count) / f64::from(most));
    }
    groups.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    groups
}

fn feature_lines<'a>(buckets: impl Iterator<Item = &'a ActionBucket>) -> Vec<FeatureLine> {
    let mut counted: Vec<(&'static features::Feature, u32, Variants)> = Vec::new();
    for bucket in buckets {
        let Some(feature) = features::find(&bucket.feature) else { continue };
        let index = counted.iter().position(|(known, _, _)| known.id == feature.id).unwrap_or_else(|| {
            counted.push((feature, 0, Vec::new()));
            counted.len() - 1
        });
        let (_, count, variants) = &mut counted[index];
        *count += bucket.count;
        if !bucket.variant.is_empty() {
            match variants.iter_mut().find(|(variant, _)| *variant == bucket.variant) {
                Some((_, times)) => *times += bucket.count,
                None => variants.push((bucket.variant.clone(), bucket.count)),
            }
        }
    }
    let most = counted.iter().map(|(_, count, _)| *count).max().unwrap_or(1).max(1);
    let mut lines: Vec<FeatureLine> = counted
        .into_iter()
        .map(|(feature, count, mut variants)| {
            variants.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let detail = match variants.len() {
                0 | 1 if variants.first().is_none_or(|(_, times)| *times == count) => String::new(),
                _ => variants
                    .iter()
                    .take(3)
                    .map(|(variant, times)| format!("{} {}", number(*times), variant_label(feature.id, variant)))
                    .collect::<Vec<_>>()
                    .join(" · "),
            };
            FeatureLine {
                id: feature.id.into(),
                label: feature.label.into(),
                count,
                share: f64::from(count) / f64::from(most),
                detail,
            }
        })
        .collect();
    lines.sort_by(|a, b| b.count.cmp(&a.count).then(a.label.cmp(&b.label)));
    lines
}

fn variant_label<'a>(feature: &str, variant: &'a str) -> &'a str {
    match (feature, variant) {
        (features::MESSAGE, "new") => "new threads",
        (features::MESSAGE, "reply") => "replies",
        (features::MESSAGE, "now") => "sent now",
        (features::MESSAGE, "program") => "from programs",
        ("approval", "allow") => "allowed",
        ("approval", "refuse") => "refused",
        ("approval", "answer") => "answered",
        ("thread.new", "local") => "in the project",
        ("thread.new", "worktree") => "in worktrees",
        ("thread.new", "copy") => "from copies",
        ("project.add", "folder") => "folders",
        ("project.add", "new") => "new",
        ("project.add", "clone") => "cloned",
        ("account.save", "new") => "added",
        ("account.save", "change") => "changed",
        (_, "on") => "on",
        (_, "off") => "off",
        (_, other) => other,
    }
}

fn outcome_label(outcome: TurnOutcome) -> &'static str {
    match outcome {
        TurnOutcome::Done => "Finished",
        TurnOutcome::Stopped => "Stopped",
        TurnOutcome::Failed => "Failed",
        TurnOutcome::Limit => "Hit the usage limit",
        TurnOutcome::Restart => "Cut off by a restart",
        TurnOutcome::Other => "Other",
    }
}

/// A whole number with its thousands set apart.
pub fn number(value: u32) -> String {
    let digits = value.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

fn abbreviated(value: u64) -> String {
    match value {
        0..1_000 => value.to_string(),
        1_000..1_000_000 => format!("{:.1}K", value as f64 / 1e3).replace(".0K", "K"),
        1_000_000..1_000_000_000 => format!("{:.1}M", value as f64 / 1e6).replace(".0M", "M"),
        _ => format!("{:.1}B", value as f64 / 1e9).replace(".0B", "B"),
    }
}

fn plural(count: u32, one: &str, many: &str) -> String {
    format!("{} {}", number(count), if count == 1 { one } else { many })
}

fn percent(part: f64, whole: f64) -> String {
    if whole <= 0.0 {
        return "0%".into();
    }
    format!("{}%", (part / whole * 100.0).round() as u32)
}

/// How long something took, to the minute once it took an hour.
pub fn duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0).round() as u64;
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        3600..360_000 if seconds % 3600 < 60 => format!("{}h", seconds / 3600),
        3600..360_000 => format!("{}h {}m", seconds / 3600, seconds % 3600 / 60),
        _ => format!("{}h", number((seconds / 3600) as u32)),
    }
}

fn clock(hour: u8) -> String {
    match hour {
        0 => "midnight".into(),
        12 => "noon".into(),
        1..12 => format!("{hour} am"),
        _ => format!("{} pm", hour - 12),
    }
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::{ClientInfo, HourBucket};

    use super::*;

    const DAY: f64 = 86_400.0;

    fn client(device: &str, kind: ClientKind, system: &str, form: ClientForm) -> ClientRecord {
        ClientRecord {
            device: device.into(),
            client: ClientInfo { kind, system: system.into(), form, version: "0.1.9".into() },
            seen_at: 50.0,
        }
    }

    fn turn(
        start: f64,
        starter: TurnStarter,
        device: Option<&str>,
        outcome: TurnOutcome,
        turns: u32,
        seconds: f64,
    ) -> TurnBucket {
        TurnBucket {
            start,
            device: device.map(str::to_string),
            starter,
            agent: Agent::Claude,
            agent_account: String::new(),
            model: "claude-opus".into(),
            project_id: "p".into(),
            outcome,
            fast: false,
            turns,
            seconds,
            waited_secs: 30.0,
            asks: 1,
            steers: 0,
            tools: 4 * turns,
            agents: 0,
            subagents: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
            agent_thread: false,
        }
    }

    fn action(
        start: f64,
        actor: Actor,
        device: Option<&str>,
        feature: &str,
        variant: &str,
        count: u32,
    ) -> ActionBucket {
        ActionBucket {
            start,
            actor,
            device: device.map(str::to_string),
            feature: feature.into(),
            variant: variant.into(),
            count,
            direct: if device.is_some() { count } else { 0 },
            relay: 0,
            rtt_ms: if device.is_some() { 20 * u64::from(count) } else { 0 },
            timed: if device.is_some() { count } else { 0 },
        }
    }

    fn names() -> Names {
        Names {
            projects: HashMap::from([(("studio".into(), "p".into()), "Motile".into())]),
            servers: HashMap::from([("studio".into(), "Studio".into())]),
            devices: HashMap::from([
                ("mac".into(), ("Studio Mac".into(), "macos".into())),
                ("phone".into(), ("Pocket".into(), "ios".into())),
            ]),
            this_device: "mac".into(),
        }
    }

    #[test]
    fn work_is_told_apart_by_the_client_it_came_from() {
        let window = Window { since: 0.0, until: 2.0 * DAY, bucket_secs: 86_400, utc_offset_secs: 0 };
        let report = ActivityReport {
            turns: vec![
                turn(0.0, TurnStarter::User, Some("mac"), TurnOutcome::Done, 3, 600.0),
                turn(DAY, TurnStarter::User, Some("phone"), TurnOutcome::Stopped, 1, 60.0),
                turn(DAY, TurnStarter::Server, None, TurnOutcome::Done, 2, 120.0),
            ],
            actions: vec![
                action(0.0, Actor::User, Some("mac"), "message", "reply", 3),
                action(DAY, Actor::User, Some("phone"), "message", "new", 1),
                action(DAY, Actor::User, Some("phone"), "thread.new", "local", 1),
                action(DAY, Actor::Agent, None, "agent.start_agent", "", 2),
            ],
            hours: vec![HourBucket { weekday: 1, hour: 15, messages: 4, turns: 6, seconds: 780.0 }],
            clients: vec![
                client("mac", ClientKind::Mac, "macos", ClientForm::Desktop),
                client("phone", ClientKind::Ios, "ios", ClientForm::Phone),
            ],
        };
        let view = view(&[Done { server_id: "studio".into(), report }], window, &names());

        let tiles: Vec<_> =
            view.tiles.iter().map(|tile| (tile.label.as_str(), tile.value.as_str(), tile.detail.as_str())).collect();
        assert_eq!(
            tiles,
            [
                ("Messages", "4", "1 thread started"),
                ("Agent time", "13m", "6 turns · 2m on average"),
                ("Waiting on you", "1m", "3 approvals and questions"),
                ("Finished", "83%", "1 stopped · 0 failed"),
            ]
        );
        let sources: Vec<_> = view
            .sources
            .iter()
            .map(|source| {
                (
                    source.label.as_str(),
                    source.symbol,
                    source.color,
                    &source.message_points[..],
                    &source.turn_points[..],
                )
            })
            .collect();
        assert_eq!(
            sources,
            [
                ("Mac", "laptop", 1, &[3, 0][..], &[0, 0][..]),
                ("iPhone", "smartphone", 2, &[0, 1][..], &[0, 0][..]),
                ("Agents", "sparkles", 4, &[0, 0][..], &[3, 3][..]),
            ]
        );
        let clients: Vec<_> = view
            .clients
            .iter()
            .map(|row| (row.name.as_str(), row.client.as_str(), row.messages, row.this_device))
            .collect();
        assert_eq!(clients, [("Studio Mac", "Mac app · macOS", 3, true), ("Pocket", "iOS app · iOS", 1, false)]);
        assert_eq!(view.clients[0].detail, "Version 0.1.9 · Direct · 20 ms");
        assert_eq!(view.busiest.as_deref(), Some("You send the most messages on Tuesdays around 3 pm"));
        assert_eq!(view.hours.iter().filter(|cell| cell.level == 4).count(), 1);
        assert_eq!(view.projects[0].name, "Motile");
        assert_eq!(view.features[0].features[0].label, "Messages sent");
        assert_eq!(view.features[0].features[0].detail, "3 replies · 1 new threads");
        assert_eq!(view.agent_features[0].label, "Agents started");
    }

    #[test]
    fn a_kind_of_device_keeps_its_colour_whichever_did_the_most() {
        let window = Window { since: 0.0, until: DAY, bucket_secs: 86_400, utc_offset_secs: 0 };
        let report = |mac: u32, phone: u32| ActivityReport {
            actions: vec![
                action(0.0, Actor::User, Some("mac"), "message", "reply", mac),
                action(0.0, Actor::User, Some("phone"), "message", "reply", phone),
                action(0.0, Actor::User, Some("old"), "message", "reply", 1),
            ],
            clients: vec![
                client("mac", ClientKind::Mac, "macos", ClientForm::Desktop),
                client("phone", ClientKind::Ios, "ios", ClientForm::Phone),
            ],
            ..ActivityReport::default()
        };
        let colors = |report: ActivityReport| -> Vec<(String, u8)> {
            let view = view(&[Done { server_id: "studio".into(), report }], window, &names());
            let mut colors: Vec<_> = view.sources.into_iter().map(|source| (source.label, source.color)).collect();
            colors.sort();
            colors
        };
        let expected = [("Mac".to_string(), 1), ("Unknown client".to_string(), 0), ("iPhone".to_string(), 2)];
        assert_eq!(colors(report(9, 1)), expected);
        assert_eq!(colors(report(1, 9)), expected);
    }

    #[test]
    fn a_thread_an_agent_started_is_a_subagent_s_work() {
        let window = Window { since: 0.0, until: DAY, bucket_secs: 86_400, utc_offset_secs: 0 };
        let in_agent_thread = |starter, device, seconds| TurnBucket {
            agent_thread: true,
            ..turn(0.0, starter, device, TurnOutcome::Done, 1, seconds)
        };
        let report = ActivityReport {
            turns: vec![
                in_agent_thread(TurnStarter::Agent, None, 60.0),
                in_agent_thread(TurnStarter::Server, None, 120.0),
                in_agent_thread(TurnStarter::User, None, 240.0),
                in_agent_thread(TurnStarter::User, Some("mac"), 30.0),
                turn(0.0, TurnStarter::User, None, TurnOutcome::Done, 1, 10.0),
                turn(0.0, TurnStarter::Notice, None, TurnOutcome::Done, 1, 5.0),
            ],
            clients: vec![client("mac", ClientKind::Mac, "macos", ClientForm::Desktop)],
            ..ActivityReport::default()
        };
        let view = view(&[Done { server_id: "studio".into(), report }], window, &names());
        let sources: Vec<_> =
            view.sources.iter().map(|source| (source.label.as_str(), source.turns, source.seconds)).collect();
        assert_eq!(sources, [("Agents", 2, 15.0), ("Subagents", 4, 450.0)]);
    }

    #[test]
    fn a_clients_share_is_of_the_messages_the_clients_listed_sent() {
        let window = Window { since: 0.0, until: DAY, bucket_secs: 86_400, utc_offset_secs: 0 };
        let report = ActivityReport {
            actions: vec![
                action(0.0, Actor::User, Some("mac"), "message", "reply", 3),
                action(0.0, Actor::User, Some("phone"), "message", "reply", 1),
                action(0.0, Actor::User, None, "message", "reply", 1000),
            ],
            clients: vec![
                client("mac", ClientKind::Mac, "macos", ClientForm::Desktop),
                client("phone", ClientKind::Ios, "ios", ClientForm::Phone),
            ],
            ..ActivityReport::default()
        };
        let view = view(&[Done { server_id: "studio".into(), report }], window, &names());
        let shares: Vec<_> = view.clients.iter().map(|row| (row.name.as_str(), row.share)).collect();
        assert_eq!(shares, [("Studio Mac", 0.75), ("Pocket", 0.25)]);
    }

    #[test]
    fn nothing_done_is_an_empty_view() {
        let window = Window { since: 0.0, until: DAY, bucket_secs: 3600, utc_offset_secs: 0 };
        let view = view(&[Done { server_id: "studio".into(), report: ActivityReport::default() }], window, &names());
        assert!(view.empty);
        assert_eq!(view.starts.len(), 24);
    }

    #[test]
    fn durations_and_numbers_read_as_people_say_them() {
        assert_eq!(
            [duration(42.0), duration(600.0), duration(3600.0), duration(5400.0), duration(400_000.0)],
            ["42s", "10m", "1h", "1h 30m", "111h"]
        );
        assert_eq!(number(1_234_567), "1,234,567");
        assert_eq!(clock(15), "3 pm");
    }
}
