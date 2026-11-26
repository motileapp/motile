//! What a server tells the account's devices of its threads: an agent finished, failed, needs the
//! user or reached its usage limit, and what its agents are doing now. The clients connected to the
//! server show it; the phones are pushed it through the auth server, sealed to each phone's own key
//! so that the auth server never reads it.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hpke::aead::ChaCha20Poly1305;
use hpke::kdf::HkdfSha256;
use hpke::kem::X25519HkdfSha256;
use hpke::{Deserializable, Kem, OpModeR, OpModeS, Serializable};
use serde::{Deserialize, Serialize};

use crate::identity::{DeviceKey, x25519_public};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    Finished,
    Failed,
    /// A tool call waits for the user to allow it.
    Approval,
    /// The agent asks the user questions.
    Question,
    /// The agent presents a plan to approve.
    Plan,
    /// The agent reached its usage limit.
    Limit,
    #[serde(other)]
    Other,
}

impl AlertKind {
    /// Waits for the user: shown even through a Focus that lets time-sensitive alerts through.
    pub fn waits_for_user(self) -> bool {
        matches!(self, Self::Approval | Self::Question | Self::Plan)
    }

    pub fn headline(self) -> &'static str {
        match self {
            Self::Finished => "Finished",
            Self::Failed => "Failed",
            Self::Approval => "Needs your approval",
            Self::Question => "Has a question",
            Self::Plan => "Has a plan for you",
            Self::Limit => "Reached its usage limit",
            Self::Other => "Needs you",
        }
    }

    /// What a phone shows of an alert it couldn't open.
    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Finished => "An agent finished",
            Self::Failed => "An agent failed",
            Self::Limit => "An agent reached its usage limit",
            _ => "An agent needs you",
        }
    }
}

/// One thing that happened in a thread that the user would want to hear of.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Alert {
    /// Unique; a client that hears of the same alert twice shows it once.
    pub id: String,
    pub server_id: String,
    pub thread_id: String,
    pub kind: AlertKind,
    pub thread_title: String,
    pub project: String,
    /// What it waits for: the tool call to allow, or the title of the agent that asks when it is
    /// one the thread's agent started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// What the notification says under the thread's title: "Needs your approval · Run greet.py".
    pub body: String,
    pub at: f64,
}

pub struct NewAlert {
    pub server_id: String,
    pub thread_id: String,
    pub kind: AlertKind,
    pub thread_title: String,
    pub project: String,
    pub detail: Option<String>,
}

impl Alert {
    pub fn new(new: NewAlert, id: String, at: f64) -> Self {
        let detail = new.detail.filter(|detail| !detail.trim().is_empty());
        let body = match &detail {
            Some(detail) => format!("{} · {detail}", new.kind.headline()),
            None => new.kind.headline().to_string(),
        };
        Self {
            id,
            server_id: new.server_id,
            thread_id: new.thread_id,
            kind: new.kind,
            thread_title: new.thread_title,
            project: new.project,
            detail,
            body,
            at,
        }
    }
}

/// Which alerts a device wants.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct AlertChoices {
    pub finished: bool,
    pub failed: bool,
    pub approval: bool,
    /// Questions and plans.
    pub question: bool,
    pub limit: bool,
    /// A phone's alerts play the system's sound.
    #[serde(default)]
    pub sound: bool,
}

impl Default for AlertChoices {
    fn default() -> Self {
        Self { finished: true, failed: true, approval: true, question: true, limit: true, sound: false }
    }
}

impl AlertChoices {
    pub fn allows(&self, kind: AlertKind) -> bool {
        match kind {
            AlertKind::Finished => self.finished,
            AlertKind::Failed => self.failed,
            AlertKind::Approval => self.approval,
            AlertKind::Question | AlertKind::Plan => self.question,
            AlertKind::Limit => self.limit,
            AlertKind::Other => true,
        }
    }
}

/// How long a thread that finished or failed stays in what its server's agents are doing.
pub const ENDED_WORK_STAYS_SECS: f64 = 15.0 * 60.0;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Working,
    /// It waits for the user: an approval, an answer or a plan.
    Waiting,
    Finished,
    Failed,
    #[serde(other)]
    Other,
}

/// A thread in what a server's agents are doing.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct WorkItem {
    pub thread_id: String,
    pub title: String,
    pub project: String,
    pub state: WorkState,
    /// When it came to this state.
    pub since: f64,
}

/// What a server's agents are doing: the threads that work or wait for the user, and those that
/// ended in the last `ENDED_WORK_STAYS_SECS`, the most recent first. A phone shows the work of
/// every server of the account together, in a Live Activity, an ongoing notification and a widget.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct ServerWork {
    pub server_id: String,
    pub server_name: String,
    /// The first `WORK_ITEMS`: those that wait, then those that work, then those that ended.
    pub items: Vec<WorkItem>,
    /// How many work and wait in all, beyond the items listed too.
    #[serde(default)]
    pub working: u32,
    #[serde(default)]
    pub waiting: u32,
    pub at: f64,
}

/// How many threads a server's work lists.
pub const WORK_ITEMS: usize = 6;

impl ServerWork {
    pub fn new(server_id: String, server_name: String, mut items: Vec<WorkItem>, at: f64) -> Self {
        let count = |state| items.iter().filter(|item| item.state == state).count() as u32;
        let (working, waiting) = (count(WorkState::Working), count(WorkState::Waiting));
        let rank = |state: WorkState| match state {
            WorkState::Waiting => 0,
            WorkState::Working => 1,
            _ => 2,
        };
        items.sort_by(|a, b| rank(a.state).cmp(&rank(b.state)).then(b.since.total_cmp(&a.since)));
        items.truncate(WORK_ITEMS);
        Self { server_id, server_name, items, working, waiting, at }
    }
}

/// What a sealed push carries.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Pushed {
    Alert(Alert),
    Work(ServerWork),
}

const SEAL_INFO: &[u8] = b"motile push 1";
const ENCAPPED_LEN: usize = 32;

/// What a phone opens what is pushed to it with: its own device key, converted from Ed25519 to
/// X25519 (the SHA-512 of its seed, as libsodium's `crypto_sign_ed25519_sk_to_curve25519` does).
/// Sealed with HPKE: X25519, HKDF-SHA256 and ChaCha20-Poly1305 (RFC 9180, base mode), which Apple's
/// CryptoKit opens as `.Curve25519_SHA256_ChachaPoly`.
pub struct PushKey([u8; 32]);

impl PushKey {
    pub fn of_device(key: &DeviceKey) -> Self {
        Self(key.x25519_secret())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The X25519 secret, for the part of a phone's app that opens pushes without the core.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    pub fn open(&self, sealed: &str) -> Option<Vec<u8>> {
        let bytes = STANDARD.decode(sealed.trim()).ok()?;
        if bytes.len() < ENCAPPED_LEN {
            return None;
        }
        let (encapped, ciphertext) = bytes.split_at(ENCAPPED_LEN);
        let secret = <X25519HkdfSha256 as Kem>::PrivateKey::from_bytes(&self.0).ok()?;
        let encapped = <X25519HkdfSha256 as Kem>::EncappedKey::from_bytes(encapped).ok()?;
        hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256>(
            &OpModeR::Base,
            &secret,
            &encapped,
            SEAL_INFO,
            ciphertext,
            b"",
        )
        .ok()
    }

    pub fn open_pushed(&self, sealed: &str) -> Option<Pushed> {
        serde_json::from_slice(&self.open(sealed)?).ok()
    }
}

/// Seals `plaintext` to the phone whose device key is `device`: the encapsulated key, then the
/// ciphertext, in base64. `None` when `device` isn't a device key.
pub fn seal(device: &str, plaintext: &[u8]) -> Option<String> {
    let public = <X25519HkdfSha256 as Kem>::PublicKey::from_bytes(&x25519_public(device)?).ok()?;
    let (encapped, ciphertext) = hpke::single_shot_seal::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256>(
        &OpModeS::Base,
        &public,
        SEAL_INFO,
        plaintext,
        b"",
    )
    .ok()?;
    let mut bytes = encapped.to_bytes().to_vec();
    bytes.extend_from_slice(&ciphertext);
    Some(STANDARD.encode(bytes))
}

pub fn seal_pushed(device: &str, pushed: &Pushed) -> Option<String> {
    seal(device, &serde_json::to_vec(pushed).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(kind: AlertKind, detail: Option<&str>) -> Alert {
        let new = NewAlert {
            server_id: "s".into(),
            thread_id: "t".into(),
            kind,
            thread_title: "Fix the login".into(),
            project: "motile".into(),
            detail: detail.map(String::from),
        };
        Alert::new(new, "a".into(), 1.0)
    }

    #[test]
    fn a_sealed_push_opens_only_with_its_key() {
        let device = DeviceKey::generate();
        let pushed = Pushed::Alert(alert(AlertKind::Approval, Some("Run greet.py")));
        let sealed = seal_pushed(&device.public(), &pushed).unwrap();
        assert_eq!(PushKey::of_device(&device).open_pushed(&sealed), Some(pushed.clone()));
        let exported = PushKey::from_bytes(PushKey::of_device(&device).to_bytes());
        assert_eq!(exported.open_pushed(&sealed), Some(pushed));
        assert_eq!(PushKey::of_device(&DeviceKey::generate()).open(&sealed), None);
    }

    #[test]
    fn a_tampered_or_malformed_push_does_not_open() {
        let device = DeviceKey::generate();
        let key = PushKey::of_device(&device);
        let sealed = seal(&device.public(), b"hello").unwrap();
        let mut bytes = STANDARD.decode(&sealed).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        assert_eq!(key.open(&STANDARD.encode(bytes)), None);
        assert_eq!(key.open("not base64"), None);
        assert_eq!(key.open(&STANDARD.encode([0u8; 8])), None);
        assert_eq!(seal("not a key", b"hello"), None);
    }

    /// RFC 9180 A.2.1: DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20Poly1305, base mode, which
    /// is what Apple's CryptoKit seals and opens.
    #[test]
    fn opens_the_rfc_test_vector() {
        let secret: [u8; 32] = hex::decode("8057991eef8f1f1af18f4a9491d16a1ce333f695d4db8e38da75975c4478e0fb")
            .unwrap()
            .try_into()
            .unwrap();
        let encapped = hex::decode("1afa08d3dec047a643885163f1180476fa7ddb54c6a8029ea33f95796bf2ac4a").unwrap();
        let info = hex::decode("4f6465206f6e2061204772656369616e2055726e").unwrap();
        let aad = hex::decode("436f756e742d30").unwrap();
        let ciphertext =
            hex::decode("1c5250d8034ec2b784ba2cfd69dbdb8af406cfe3ff938e131f0def8c8b60b4db21993c62ce81883d2dd1b51a28")
                .unwrap();
        let secret = <X25519HkdfSha256 as Kem>::PrivateKey::from_bytes(&secret).unwrap();
        let encapped = <X25519HkdfSha256 as Kem>::EncappedKey::from_bytes(&encapped).unwrap();
        let opened = hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256>(
            &OpModeR::Base,
            &secret,
            &encapped,
            &info,
            &ciphertext,
            &aad,
        )
        .unwrap();
        assert_eq!(opened, b"Beauty is truth, truth beauty");
    }

    #[test]
    fn the_body_says_what_it_waits_for() {
        assert_eq!(alert(AlertKind::Approval, Some("Run greet.py")).body, "Needs your approval · Run greet.py");
        assert_eq!(alert(AlertKind::Finished, None).body, "Finished");
        let blank = alert(AlertKind::Finished, Some(" "));
        assert_eq!((blank.body.as_str(), blank.detail), ("Finished", None));
    }

    #[test]
    fn work_lists_the_first_items_and_counts_them_all() {
        let item = |id: usize, state, since| WorkItem {
            thread_id: id.to_string(),
            title: String::new(),
            project: String::new(),
            state,
            since,
        };
        let mut items: Vec<WorkItem> = (0..8).map(|id| item(id, WorkState::Working, id as f64)).collect();
        items.push(item(8, WorkState::Finished, 99.0));
        items.push(item(9, WorkState::Waiting, 1.0));
        let work = ServerWork::new("s".into(), "studio".into(), items, 100.0);
        assert_eq!((work.working, work.waiting), (8, 1));
        let listed: Vec<&str> = work.items.iter().map(|item| item.thread_id.as_str()).collect();
        assert_eq!(listed, ["9", "7", "6", "5", "4", "3"]);
    }

    #[test]
    fn choices_cover_plans_with_questions() {
        let choices = AlertChoices { question: false, ..AlertChoices::default() };
        assert!(!choices.allows(AlertKind::Plan));
        assert!(!choices.allows(AlertKind::Question));
        assert!(choices.allows(AlertKind::Approval));
    }
}
