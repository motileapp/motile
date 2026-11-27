//! Pushing to the account's phones what its servers sealed for them: alerts, and what the agents
//! are doing, as a Live Activity on an iPhone and an ongoing notification on Android. The auth
//! server only ever sees the sealed text, the kind of an alert and how many agents work or wait.

mod apns;
mod fcm;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use motile_protocol::auth_api::{PushService, SealedAlert, push_payload};
use serde_json::{Value, json};
use sqlx::PgPool;

use self::apns::{Apns, ApnsPush};
use self::fcm::Fcm;
use crate::config::PushConfig;
use crate::db::{self, Phone, WorkPart};

const TITLE: &str = "Motile";
const MAX_PAYLOAD_BYTES: usize = 4096;
const ALERT_LIFETIME_SECS: i64 = 24 * 60 * 60;
const ENDED_LIVE_STAYS_SECS: i64 = 15 * 60;
const MAX_COLLAPSE_ID_BYTES: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sent {
    Delivered,
    /// The push service no longer takes the token.
    Gone,
    Failed,
}

pub struct Pusher {
    apns: Option<Apns>,
    fcm: Option<Fcm>,
}

impl Pusher {
    pub fn new(config: &PushConfig) -> Self {
        let pusher = Self { apns: Apns::new(config), fcm: Fcm::new(config) };
        if pusher.apns.is_none() {
            tracing::info!("APNs isn't set up: nothing is pushed to iPhones and iPads");
        }
        if pusher.fcm.is_none() {
            tracing::info!("FCM isn't set up: nothing is pushed to Android phones");
        }
        pusher
    }

    pub async fn alert(&self, db: &PgPool, phone: &Phone, alert: &SealedAlert) {
        let registration = &phone.registration;
        let sent = match &registration.service {
            PushService::Apns { topic, sandbox } => {
                let Some(apns) = &self.apns else { return dropped("APNs") };
                let collapse_id = Some(alert.thread_id.as_str())
                    .filter(|id| id.len() <= MAX_COLLAPSE_ID_BYTES && id.bytes().all(|byte| byte.is_ascii_graphic()));
                let push = ApnsPush {
                    token: &registration.token,
                    topic: topic.clone(),
                    push_type: "alert",
                    priority: 10,
                    collapse_id,
                    expiration: Some(chrono::Utc::now().timestamp() + ALERT_LIFETIME_SECS),
                    sandbox: *sandbox,
                    payload: apns_alert(alert, registration.choices.sound),
                };
                apns.send(&push).await
            }
            PushService::Fcm => {
                let Some(fcm) = &self.fcm else { return dropped("FCM") };
                fcm.send(&fcm_alert(&registration.token, alert, registration.choices.sound)).await
            }
        };
        if sent == Sent::Gone {
            forget(db::drop_push(db, &phone.device, &registration.token).await);
        }
    }

    /// Pushes the phone what the account's servers' agents are doing; `before` is how many worked
    /// or waited before the part that came now.
    pub async fn work(&self, db: &PgPool, phone: &Phone, parts: &[WorkPart], before: i64) {
        let registration = &phone.registration;
        let counts = Counts::of(parts);
        let active = counts.working + counts.waiting;
        match &registration.service {
            PushService::Apns { topic, sandbox } => {
                let Some(apns) = &self.apns else { return dropped("APNs") };
                let topic = format!("{topic}.push-type.liveactivity");
                let now = chrono::Utc::now().timestamp();
                let (token, event, priority) = match (&registration.live_token, &registration.live_start_token) {
                    (Some(token), _) if active == 0 => (token, LiveEvent::End, 10),
                    (Some(token), _) => (token, LiveEvent::Update, 5),
                    (None, Some(token)) if active > 0 => {
                        match db::claim_live_start(db, &phone.device).await {
                            Ok(true) => {}
                            Ok(false) => return,
                            Err(error) => return tracing::warn!("couldn't start a Live Activity: {error}"),
                        }
                        (token, LiveEvent::Start, 10)
                    }
                    _ => return,
                };
                let sealed =
                    fitted(parts, |sealed| live(event, sealed, counts, now).to_string().len() <= MAX_PAYLOAD_BYTES);
                let push = ApnsPush {
                    token,
                    topic,
                    push_type: "liveactivity",
                    priority,
                    collapse_id: None,
                    expiration: None,
                    sandbox: *sandbox,
                    payload: live(event, &sealed, counts, now),
                };
                let sent = apns.send(&push).await;
                if sent == Sent::Gone || (sent == Sent::Delivered && event == LiveEvent::End) {
                    forget(db::forget_live_token(db, &phone.device, token).await);
                }
            }
            PushService::Fcm => {
                let Some(fcm) = &self.fcm else { return dropped("FCM") };
                let urgent = (before == 0) != (active == 0);
                let sealed = fitted(parts, |sealed| {
                    fcm_work(&registration.token, sealed, urgent).to_string().len() <= MAX_PAYLOAD_BYTES
                });
                if fcm.send(&fcm_work(&registration.token, &sealed, urgent)).await == Sent::Gone {
                    forget(db::drop_push(db, &phone.device, &registration.token).await);
                }
            }
        }
    }
}

fn dropped(service: &str) {
    tracing::info!("dropped a push: {service} isn't set up");
}

fn forget(result: Result<(), sqlx::Error>) {
    if let Err(error) = result {
        tracing::warn!("couldn't forget a push token: {error}");
    }
}

fn apns_alert(alert: &SealedAlert, sound: bool) -> Value {
    let level = if alert.kind.waits_for_user() { "time-sensitive" } else { "active" };
    let mut aps = json!({
        "alert": { "title": TITLE, "body": alert.kind.placeholder() },
        "mutable-content": 1,
        "thread-id": alert.thread_id,
        "interruption-level": level,
    });
    if sound {
        aps["sound"] = json!("default");
    }
    json!({
        "aps": aps,
        push_payload::TYPE: "alert",
        push_payload::SEALED: alert.sealed,
        push_payload::THREAD: alert.thread_id,
    })
}

fn fcm_alert(token: &str, alert: &SealedAlert, sound: bool) -> Value {
    json!({
        "token": token,
        "data": {
            push_payload::TYPE: "alert",
            push_payload::SEALED: alert.sealed,
            push_payload::THREAD: alert.thread_id,
            "kind": alert.kind,
            "sound": if sound { "1" } else { "0" },
        },
        "android": { "priority": "HIGH", "ttl": "86400s", "collapse_key": alert.thread_id },
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LiveEvent {
    Start,
    Update,
    End,
}

/// How many agents work and wait across the account's servers, which the auth server already sees,
/// so that the Live Activity counts right even when its parts don't open or don't all fit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Counts {
    working: i64,
    waiting: i64,
}

impl Counts {
    fn of(parts: &[WorkPart]) -> Self {
        Self {
            working: parts.iter().map(|part| part.working).sum(),
            waiting: parts.iter().map(|part| part.waiting).sum(),
        }
    }
}

fn live(event: LiveEvent, sealed: &[&str], counts: Counts, now: i64) -> Value {
    let content = json!({ "parts": sealed, "working": counts.working, "waiting": counts.waiting });
    let aps = match event {
        LiveEvent::Start => json!({
            "timestamp": now,
            "event": "start",
            "attributes-type": "AgentsAttributes",
            "attributes": {},
            "content-state": content,
            "alert": { "title": TITLE, "body": "Agents are working" },
        }),
        LiveEvent::Update => json!({ "timestamp": now, "event": "update", "content-state": content }),
        LiveEvent::End => json!({
            "timestamp": now,
            "event": "end",
            "content-state": content,
            "dismissal-date": now + ENDED_LIVE_STAYS_SECS,
        }),
    };
    json!({ "aps": aps })
}

fn fcm_work(token: &str, sealed: &[&str], urgent: bool) -> Value {
    let parts = serde_json::to_string(sealed).expect("strings are JSON");
    json!({
        "token": token,
        "data": { push_payload::TYPE: "work", push_payload::PARTS: parts },
        "android": { "priority": if urgent { "HIGH" } else { "NORMAL" }, "ttl": "7200s", "collapse_key": "work" },
    })
}

/// The parts, by server, without the oldest ones that don't fit.
fn fitted(parts: &[WorkPart], fits: impl Fn(&[&str]) -> bool) -> Vec<&str> {
    let mut kept: Vec<&WorkPart> = parts.iter().collect();
    loop {
        let sealed: Vec<&str> = kept.iter().map(|part| part.sealed.as_str()).collect();
        if kept.is_empty() || fits(&sealed) {
            return sealed;
        }
        let oldest = (0..kept.len()).min_by_key(|&index| kept[index].at).expect("parts are kept");
        kept.remove(oldest);
    }
}

/// A JSON Web Token: `header` and `claims`, signed by `sign`.
fn jwt(header: &Value, claims: &Value, sign: impl FnOnce(&[u8]) -> Option<Vec<u8>>) -> Option<String> {
    let message =
        format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(claims.to_string()));
    let signature = sign(message.as_bytes())?;
    Some(format!("{message}.{}", URL_SAFE_NO_PAD.encode(signature)))
}

/// The DER inside a PEM's text, which may come with its line breaks written as `\n`.
pub fn pem_body(pem: &str) -> Option<Vec<u8>> {
    let pem = pem.replace("\\n", "\n");
    let body: String = pem.lines().filter(|line| !line.starts_with("-----")).flat_map(str::chars).collect();
    let body: String = body.chars().filter(|character| !character.is_whitespace()).collect();
    STANDARD.decode(body).ok()
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    fn part(server: &str, size: usize, age_secs: i64) -> WorkPart {
        WorkPart {
            server: server.into(),
            sealed: "x".repeat(size),
            working: 1,
            waiting: 0,
            at: Utc::now() - Duration::seconds(age_secs),
        }
    }

    #[test]
    fn the_oldest_parts_are_dropped_until_the_push_fits() {
        let parts = [part("a", 1500, 30), part("b", 1500, 600), part("c", 1500, 5)];
        let counts = Counts::of(&parts);
        let fits = |sealed: &[&str]| live(LiveEvent::Update, sealed, counts, 0).to_string().len() <= MAX_PAYLOAD_BYTES;
        let kept = fitted(&parts, fits);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept, vec![parts[0].sealed.as_str(), parts[2].sealed.as_str()]);
        assert!(fitted(&[part("a", 5000, 0)], fits).is_empty());
    }

    #[test]
    fn a_pem_reads_with_its_line_breaks_escaped() {
        let pem = "-----BEGIN PRIVATE KEY-----\nAAEC\nAwQ=\n-----END PRIVATE KEY-----\n";
        assert_eq!(pem_body(pem), Some(vec![0, 1, 2, 3, 4]));
        assert_eq!(pem_body(&pem.replace('\n', "\\n")), Some(vec![0, 1, 2, 3, 4]));
    }
}
