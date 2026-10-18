use chrono::{DateTime, Duration, Utc};
use motile_protocol::auth_api::{Device, DeviceKind, Me, RemovedServer, User};
use sqlx::PgPool;
use uuid::Uuid;

use crate::google::Identity;

pub const SIGN_IN_LIFETIME: Duration = Duration::minutes(10);
pub const ENROLL_TOKEN_LIFETIME: Duration = Duration::minutes(15);
pub const SESSION_LIFETIME: Duration = Duration::days(30);
pub const MAX_DEVICES_PER_USER: i64 = 200;

#[derive(sqlx::FromRow, Clone)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
}

#[derive(sqlx::FromRow)]
struct DeviceRow {
    public_key: String,
    kind: String,
    name: String,
    platform: String,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct RemovedRow {
    public_key: String,
    name: String,
    platform: String,
    removed_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
pub struct SignIn {
    pub id: String,
    pub challenge: String,
    pub app_state: String,
    pub google_verifier: String,
    pub nonce: String,
    pub user_id: Option<Uuid>,
    pub web: bool,
}

pub fn kind_text(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Server => "server",
        DeviceKind::Client => "client",
    }
}

pub async fn upsert_user(db: &PgPool, identity: &Identity) -> Result<UserRow, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO users (id, subject, email, name, picture) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (subject) DO UPDATE SET
             email = excluded.email,
             name = excluded.name,
             picture = excluded.picture,
             last_sign_in_at = now()
         RETURNING id, email, name, picture",
    )
    .bind(Uuid::new_v4())
    .bind(&identity.subject)
    .bind(&identity.email)
    .bind(&identity.name)
    .bind(&identity.picture)
    .fetch_one(db)
    .await
}

pub async fn user_of_device(db: &PgPool, public_key: &str) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT users.id, users.email, users.name, users.picture
         FROM devices JOIN users ON users.id = devices.user_id
         WHERE devices.public_key = $1",
    )
    .bind(public_key)
    .fetch_optional(db)
    .await
}

pub async fn device_kind(db: &PgPool, public_key: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT kind FROM devices WHERE public_key = $1").bind(public_key).fetch_optional(db).await
}

pub async fn me(db: &PgPool, user: &UserRow) -> Result<Me, sqlx::Error> {
    let rows: Vec<DeviceRow> = sqlx::query_as(
        "SELECT public_key, kind, name, platform, created_at FROM devices WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user.id)
    .fetch_all(db)
    .await?;
    let devices = rows.into_iter().map(|row| Device {
        public_key: row.public_key,
        kind: if row.kind == "server" { DeviceKind::Server } else { DeviceKind::Client },
        name: row.name,
        platform: row.platform,
        created_at: seconds(row.created_at),
    });
    let gone: Vec<RemovedRow> = sqlx::query_as(
        "SELECT public_key, name, platform, removed_at, deleted_at FROM removed_servers
         WHERE user_id = $1 ORDER BY removed_at",
    )
    .bind(user.id)
    .fetch_all(db)
    .await?;
    let (deleted, removed): (Vec<RemovedRow>, Vec<RemovedRow>) =
        gone.into_iter().partition(|row| row.deleted_at.is_some());
    let removed = removed.into_iter().map(|row| RemovedServer {
        public_key: row.public_key,
        name: row.name,
        platform: row.platform,
        removed_at: seconds(row.removed_at),
    });
    Ok(Me {
        user: Some(User { email: user.email.clone(), name: user.name.clone(), picture: user.picture.clone() }),
        devices: devices.collect(),
        removed: removed.collect(),
        deleted: deleted.into_iter().map(|row| row.public_key).collect(),
    })
}

fn seconds(at: DateTime<Utc>) -> f64 {
    at.timestamp_millis() as f64 / 1000.0
}

/// Links the device to the user, taking it from another account if it was linked there: the
/// caller has proven it holds the key. `false` when it can't be linked.
pub async fn link_device(
    db: &PgPool,
    user_id: Uuid,
    public_key: &str,
    kind: DeviceKind,
    name: &str,
    platform: &str,
) -> Result<bool, sqlx::Error> {
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM devices WHERE user_id = $1").bind(user_id).fetch_one(db).await?;
    if count >= MAX_DEVICES_PER_USER {
        return Ok(false);
    }
    let linked = sqlx::query(
        "INSERT INTO devices (public_key, user_id, kind, name, platform) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (public_key) DO UPDATE SET
             user_id = excluded.user_id,
             name = excluded.name,
             platform = excluded.platform
         WHERE devices.kind = excluded.kind",
    )
    .bind(public_key)
    .bind(user_id)
    .bind(kind_text(kind))
    .bind(name)
    .bind(platform)
    .execute(db)
    .await?;
    if linked.rows_affected() != 1 {
        return Ok(false);
    }
    // A server linked again, even after it was deleted completely, is an ordinary server again.
    sqlx::query("DELETE FROM removed_servers WHERE user_id = $1 AND public_key = $2")
        .bind(user_id)
        .bind(public_key)
        .execute(db)
        .await?;
    Ok(true)
}

/// Removes the device; a server is remembered as removed, with its name.
pub async fn remove_device(db: &PgPool, user_id: Uuid, public_key: &str) -> Result<bool, sqlx::Error> {
    let mut transaction = db.begin().await?;
    let removed: Option<(String, String, String)> =
        sqlx::query_as("DELETE FROM devices WHERE public_key = $1 AND user_id = $2 RETURNING kind, name, platform")
            .bind(public_key)
            .bind(user_id)
            .fetch_optional(&mut *transaction)
            .await?;
    let Some((kind, name, platform)) = removed else { return Ok(false) };
    if kind == kind_text(DeviceKind::Server) {
        sqlx::query(
            "INSERT INTO removed_servers (user_id, public_key, name, platform) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, public_key) DO UPDATE SET
                 name = excluded.name, platform = excluded.platform, removed_at = now(), deleted_at = NULL",
        )
        .bind(user_id)
        .bind(public_key)
        .bind(&name)
        .bind(&platform)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(true)
}

/// Removes the server if it is still linked and marks it deleted completely. `false` when the
/// account has no such server, linked or removed.
pub async fn delete_server_completely(db: &PgPool, user_id: Uuid, public_key: &str) -> Result<bool, sqlx::Error> {
    let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM devices WHERE public_key = $1 AND user_id = $2")
        .bind(public_key)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    if kind.as_deref() == Some(kind_text(DeviceKind::Client)) {
        return Ok(false);
    }
    if kind.is_some() {
        remove_device(db, user_id, public_key).await?;
    }
    let deleted = sqlx::query(
        "UPDATE removed_servers SET deleted_at = COALESCE(deleted_at, now()) WHERE user_id = $1 AND public_key = $2",
    )
    .bind(user_id)
    .bind(public_key)
    .execute(db)
    .await?;
    Ok(deleted.rows_affected() == 1)
}

pub struct NewSignIn<'a> {
    pub id: &'a str,
    pub challenge: &'a str,
    pub app_state: &'a str,
    pub google_verifier: &'a str,
    pub nonce: &'a str,
    pub web: bool,
}

pub async fn create_sign_in(db: &PgPool, sign_in: &NewSignIn<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO sign_ins (id, challenge, app_state, google_verifier, nonce, web, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(sign_in.id)
    .bind(sign_in.challenge)
    .bind(sign_in.app_state)
    .bind(sign_in.google_verifier)
    .bind(sign_in.nonce)
    .bind(sign_in.web)
    .bind(Utc::now() + SIGN_IN_LIFETIME)
    .execute(db)
    .await?;
    Ok(())
}

/// The sign-in waiting for Google's answer.
pub async fn pending_sign_in(db: &PgPool, id: &str) -> Result<Option<SignIn>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, challenge, app_state, google_verifier, nonce, user_id, web FROM sign_ins
         WHERE id = $1 AND code_hash IS NULL AND expires_at > now()",
    )
    .bind(id)
    .fetch_optional(db)
    .await
}

pub async fn complete_sign_in(db: &PgPool, id: &str, user_id: Uuid, code_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE sign_ins SET user_id = $2, code_hash = $3 WHERE id = $1")
        .bind(id)
        .bind(user_id)
        .bind(code_hash)
        .execute(db)
        .await?;
    Ok(())
}

/// Takes the sign-in the code belongs to. A code works once, and only for what it was started
/// for: `web` to open a session, otherwise to link a client.
pub async fn take_sign_in(db: &PgPool, code_hash: &str, web: bool) -> Result<Option<SignIn>, sqlx::Error> {
    sqlx::query_as(
        "DELETE FROM sign_ins WHERE code_hash = $1 AND web = $2 AND expires_at > now()
         RETURNING id, challenge, app_state, google_verifier, nonce, user_id, web",
    )
    .bind(code_hash)
    .bind(web)
    .fetch_optional(db)
    .await
}

pub async fn create_session(db: &PgPool, user_id: Uuid, token_hash: &str) -> Result<DateTime<Utc>, sqlx::Error> {
    let expires_at = Utc::now() + SESSION_LIFETIME;
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(token_hash)
        .bind(user_id)
        .bind(expires_at)
        .execute(db)
        .await?;
    Ok(expires_at)
}

pub async fn user_of_session(db: &PgPool, token_hash: &str) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT users.id, users.email, users.name, users.picture
         FROM sessions JOIN users ON users.id = sessions.user_id
         WHERE sessions.token_hash = $1 AND sessions.expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(db)
    .await
}

pub async fn end_session(db: &PgPool, token_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1").bind(token_hash).execute(db).await?;
    Ok(())
}

pub async fn create_enroll_token(db: &PgPool, user_id: Uuid, token_hash: &str) -> Result<DateTime<Utc>, sqlx::Error> {
    let expires_at = Utc::now() + ENROLL_TOKEN_LIFETIME;
    sqlx::query("INSERT INTO enroll_tokens (token_hash, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(token_hash)
        .bind(user_id)
        .bind(expires_at)
        .execute(db)
        .await?;
    Ok(expires_at)
}

/// Marks the token as used by the server and returns whose it is. A token works for one server.
pub async fn use_enroll_token(db: &PgPool, token_hash: &str, public_key: &str) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE enroll_tokens SET used_by = $2
         WHERE token_hash = $1 AND expires_at > now() AND (used_by IS NULL OR used_by = $2)
         RETURNING user_id",
    )
    .bind(token_hash)
    .bind(public_key)
    .fetch_optional(db)
    .await
}

pub async fn user_by_id(db: &PgPool, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, email, name, picture FROM users WHERE id = $1").bind(id).fetch_optional(db).await
}

pub async fn cleanup(db: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sign_ins WHERE expires_at < now()").execute(db).await?;
    sqlx::query("DELETE FROM enroll_tokens WHERE expires_at < now()").execute(db).await?;
    sqlx::query("DELETE FROM sessions WHERE expires_at < now()").execute(db).await?;
    Ok(())
}
