//! JSON exchanged with the auth server.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    /// A machine that runs the agents.
    Host,
    /// An app that drives hosts.
    Client,
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

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct EnrollToken {
    pub token: String,
    pub expires_at: f64,
    /// The one command to run on the host.
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
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DevLoginResponse {
    pub code: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiError {
    pub error: String,
}
