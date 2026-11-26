//! JSON exchanged with the auth server.

use serde::{Deserialize, Serialize};

use crate::alerts::{AlertChoices, AlertKind};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    /// A machine that runs the agents. Up to 0.1.6 it was called a host.
    #[serde(alias = "host")]
    Server,
    /// A client that drives servers.
    Client,
    /// The account's gateway, which serves its agents' logins and its MCP servers to every server.
    /// One per account. Up to 0.1.770 it was called the proxy.
    #[serde(alias = "proxy")]
    Gateway,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Device {
    pub public_key: String,
    pub kind: DeviceKind,
    pub name: String,
    pub platform: String,
    pub created_at: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct User {
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
}

/// What the auth server knows about the device that asked. A device nobody linked gets
/// `user: None` rather than an error.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Me {
    pub user: Option<User>,
    #[serde(default)]
    pub devices: Vec<Device>,
    /// The servers removed from the account, whose threads live on as copies on its other servers.
    #[serde(default)]
    pub removed: Vec<RemovedServer>,
    /// The servers deleted completely: no copy of their threads is kept anywhere.
    #[serde(default)]
    pub deleted: Vec<String>,
    /// The account's own iroh relay, which its devices use in place of iroh's public ones.
    #[serde(default)]
    pub relay: Option<String>,
}

/// Sets the account's relay; `None` goes back to iroh's public relays.
#[derive(Serialize, Deserialize, Debug)]
pub struct RelayRequest {
    pub url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct RemovedServer {
    pub public_key: String,
    pub name: String,
    pub platform: String,
    pub removed_at: f64,
}

/// Finishes a sign-in: the code the browser was sent back with, the secret whose hash started
/// the sign-in, and the device to link.
#[derive(Serialize, Deserialize, Debug)]
pub struct ExchangeRequest {
    pub code: String,
    pub verifier: String,
    pub public_key: String,
    pub name: String,
    pub platform: String,
    /// The device key's signature over `link_message(code)`.
    pub signature: String,
}

pub fn link_message(code: &str) -> Vec<u8> {
    format!("link\n{code}").into_bytes()
}

/// Finishes a sign-in the web app started: the code its callback was sent, and the secret whose
/// hash started the sign-in.
#[derive(Serialize, Deserialize, Debug)]
pub struct SessionRequest {
    pub code: String,
    pub verifier: String,
}

/// A browser signed in to the web app. The token is sent as `Authorization: Bearer <token>`.
#[derive(Serialize, Deserialize, Debug)]
pub struct Session {
    pub token: String,
    pub expires_at: f64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct EnrollToken {
    /// The code in the command: 8 characters of A-Z and 0-9, good for 15 minutes.
    pub token: String,
    pub expires_at: f64,
    /// The one command to run on the server.
    pub command: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EnrollRequest {
    pub token: String,
    pub public_key: String,
    pub name: String,
    pub platform: String,
    /// The device key's signature over `enroll_message(token)`.
    pub signature: String,
    /// What enrolls: a server, or the account's gateway. Missing from servers up to 0.1.267.
    #[serde(default = "server_kind")]
    pub kind: DeviceKind,
}

fn server_kind() -> DeviceKind {
    DeviceKind::Server
}

pub fn enroll_message(token: &str) -> Vec<u8> {
    format!("enroll\n{token}").into_bytes()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EnrollResponse {
    pub email: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DevLoginRequest {
    pub challenge: String,
    pub email: String,
    /// The code is for the web app, to open a session with, rather than for a client to link itself.
    #[serde(default)]
    pub web: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DevLoginResponse {
    pub code: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiError {
    pub error: String,
}

/// Where the auth server pushes to a phone.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PushService {
    /// Apple's push service, for the app whose bundle identifier is `topic`. `sandbox` for an app
    /// run from Xcode, whose tokens only the sandbox takes.
    Apns { topic: String, sandbox: bool },
    /// Firebase Cloud Messaging.
    Fcm,
}

/// A phone's push registration, which it sends again whenever any of it changes.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PushRegistration {
    pub service: PushService,
    pub token: String,
    pub choices: AlertChoices,
    /// Shows what the agents do while they work: a Live Activity, or an ongoing notification.
    pub live: bool,
    /// The token that starts a Live Activity on the phone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_start_token: Option<String>,
    /// The token of the Live Activity that runs on the phone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_token: Option<String>,
}

/// A phone of the account that takes pushes.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PushTarget {
    /// The phone's device key, which what is pushed to it is sealed to (`alerts::seal`).
    pub device: String,
    pub choices: AlertChoices,
    pub live: bool,
}

/// An alert a server sealed for one phone.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct SealedAlert {
    pub device: String,
    pub thread_id: String,
    pub kind: AlertKind,
    pub sealed: String,
}

/// What a server's agents are doing, sealed for each phone. The counts are open, so that the auth
/// server knows when to start and end a Live Activity.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PushWork {
    pub working: u32,
    pub waiting: u32,
    pub sealed: Vec<SealedFor>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct SealedFor {
    pub device: String,
    pub sealed: String,
}

/// What a push to a phone carries besides the alert's placeholder text: in the APNs payload beside
/// `aps`, and as the FCM message's data.
pub mod push_payload {
    /// `alert` or `work`.
    pub const TYPE: &str = "motile";
    /// The sealed `alerts::Pushed`.
    pub const SEALED: &str = "sealed";
    pub const THREAD: &str = "thread";
    /// For `work` on Android: every server's sealed `ServerWork`, as a JSON array of strings.
    pub const PARTS: &str = "parts";
}
