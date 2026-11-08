//! The account's S3 bucket. Each server keeps its own threads and media under its key there and
//! copies the other servers' threads from it, so the servers sync with no client between them, and
//! a server that is lost leaves its threads and media behind:
//!
//! - `<key>/server.json`: its name, its platform and its log, once the log is all there
//! - `<key>/log/<log id>/<up to>-<after>.zst`: the log's batches, numbered to sort as numbers do
//! - `<key>/media/<id>`: its images, videos and files
//!
//! A server copies every other server of the account, the unlinked ones too, and deletes the
//! prefix of one the account deleted completely.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use futures_lite::StreamExt;
use motile_protocol::media::{Namer, is_id};
use motile_protocol::wire::{BackupProgress, BackupState, Bucket};
use object_store::aws::AmazonS3Builder;
use object_store::buffered::BufWriter;
use object_store::path::Path as Key;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;

use crate::access::{Access, Named};
use crate::hub::Hub;
use crate::store::Store;
use crate::sync::LogPart;

const EVERY: Duration = Duration::from_secs(60);
/// A part of a pass that ends sooner than this never says how far it is: the minute's few new
/// changes would only make the line flicker.
const QUIET: Duration = Duration::from_secs(3);
const BUCKET_SETTING: &str = "backup_bucket";
const UPLOADED_SETTING: &str = "backup_uploaded";
const SERVER_FILE: &str = "server.json";

/// What the bucket says of a server, which another needs to copy its log.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ServerFile {
    pub name: String,
    pub platform: String,
    pub log_id: String,
}

/// How far the server's own log is in a bucket.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Uploaded {
    bucket: String,
    log_id: String,
    change: u64,
}

/// What reads and writes the server's threads, which the bucket's work does off the runtime.
pub trait Database: Send + Sync + 'static {
    fn store(&self) -> &Store;
}

impl Database for Hub {
    fn store(&self) -> &Store {
        Hub::store(self)
    }
}

impl Database for Store {
    fn store(&self) -> &Store {
        self
    }
}

async fn blocking<D: Database, T: Send + 'static>(
    database: &Arc<D>,
    work: impl FnOnce(&Store) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    let database = database.clone();
    tokio::task::spawn_blocking(move || work(database.store())).await?
}

#[derive(Clone)]
struct Opened {
    settings: Bucket,
    store: Arc<dyn ObjectStore>,
}

#[derive(Default)]
struct Kept {
    opened: Option<Opened>,
    /// The servers whose logs it copies from the bucket.
    origins: Vec<String>,
    /// The media the bucket has of this server, once listed.
    media: Option<HashSet<String>>,
    state: BackupState,
}

pub struct Backup {
    hub: Arc<Hub>,
    access: Arc<Access>,
    me: String,
    media: PathBuf,
    kept: Mutex<Kept>,
    wake: Notify,
}

impl Backup {
    pub fn new(hub: Arc<Hub>, access: Arc<Access>, me: String, media: PathBuf) -> Arc<Self> {
        let backup = Arc::new(Self { hub, access, me, media, kept: Mutex::default(), wake: Notify::new() });
        let saved =
            backup.hub.store().setting(BUCKET_SETTING).and_then(|json| serde_json::from_str::<Bucket>(&json).ok());
        if let Some(settings) = saved {
            backup.keep_open(settings);
        }
        backup
    }

    fn kept(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn keep_open(&self, settings: Bucket) {
        let opened = open(&settings);
        let mut kept = self.kept();
        kept.state = BackupState { bucket: without_secret(&settings), ..BackupState::default() };
        match opened {
            Ok(store) => kept.opened = Some(Opened { settings, store }),
            Err(error) => kept.state.error = Some(motile_protocol::error_text(&error)),
        }
        kept.origins.clear();
        kept.media = None;
        let state = kept.state.clone();
        drop(kept);
        self.hub.set_backup(Some(state));
    }

    /// Changes what the server's state says, telling the clients when it changed.
    fn report(&self, change: impl FnOnce(&mut BackupState)) {
        let mut kept = self.kept();
        if kept.opened.is_none() {
            return;
        }
        let before = kept.state.clone();
        change(&mut kept.state);
        if kept.state == before {
            return;
        }
        let state = kept.state.clone();
        drop(kept);
        self.hub.set_backup(Some(state));
    }

    /// The servers whose logs it copies from the bucket, which no client needs to carry to it.
    pub fn origins(&self) -> Vec<String> {
        self.kept().origins.clone()
    }

    /// Keeps the bucket once it can list, write, read and delete in it, or stops with `None`.
    pub async fn set(&self, bucket: Option<Bucket>) -> anyhow::Result<()> {
        let Some(settings) = bucket else {
            self.hub.store().set_setting(BUCKET_SETTING, None)?;
            *self.kept() = Kept::default();
            self.hub.set_backup(None);
            return Ok(());
        };
        let store = open(&settings)?;
        check(store.as_ref(), &self.me).await.map_err(|error| anyhow::anyhow!(said(&error)))?;
        self.hub.store().set_setting(BUCKET_SETTING, Some(&serde_json::to_string(&settings)?))?;
        self.keep_open(settings);
        self.wake.notify_one();
        Ok(())
    }

    pub fn start(self: &Arc<Self>) {
        let backup = self.clone();
        tokio::spawn(async move {
            loop {
                backup.pass().await;
                tokio::select! {
                    _ = tokio::time::sleep(EVERY) => {}
                    _ = backup.wake.notified() => {}
                }
            }
        });
    }

    /// Puts what the bucket lacks of this server there, then copies what it has of the others.
    async fn pass(&self) {
        let Some(opened) = self.kept().opened.clone() else { return };
        let uploaded = self.upload(&opened).await;
        self.report(|state| state.progress = None);
        if let Err(error) = &uploaded {
            tracing::warn!("couldn't back up to the bucket: {error:#}");
        }
        let copied = self.copy_others(&opened).await;
        if let Err(error) = &copied {
            tracing::warn!("couldn't copy the other servers' threads from the bucket: {error:#}");
        }
        let error = uploaded.err().or(copied.err()).map(|error| said(&error));
        self.report(|state| {
            state.progress = None;
            state.error = error;
        });
    }

    /// Puts the log's new changes in the bucket, then the media it lacks, saying how far both are
    /// together, in bytes.
    async fn upload(&self, opened: &Opened) -> anyhow::Result<()> {
        let kept = self.hub.store().setting(UPLOADED_SETTING).and_then(|json| serde_json::from_str(&json).ok());
        let named = self.access.servers().await.remove(&self.me);
        let file = ServerFile {
            name: named
                .as_ref()
                .map_or_else(crate::agents::environment::Environment::hostname, |named| named.name.clone()),
            platform: named.map_or_else(|| std::env::consts::OS.to_string(), |named| named.platform),
            log_id: String::new(),
        };
        let listed = self.kept().media.take();
        let mut known = match listed {
            Some(known) => known,
            None => listed_media(opened.store.as_ref(), &self.me).await?,
        };
        let missing = missing_media(&self.media, &known);
        let media_bytes: u64 = missing.iter().map(|(_, size)| size).sum();
        let meter = Meter::new(self, true);
        let log_bytes = AtomicU64::new(0);
        let log = async {
            let report = |sent: LogSent| {
                log_bytes.store(sent.bytes, Ordering::Relaxed);
                meter.show(sent.bytes, sent.whole() + media_bytes);
            };
            let hub = self.hub.clone();
            let bucket_id = bucket_id(&opened.settings);
            let uploaded = upload_log(&hub, opened.store.as_ref(), &self.me, &bucket_id, kept, &file, report).await?;
            self.hub.store().set_setting(UPLOADED_SETTING, Some(&serde_json::to_string(&uploaded)?))?;
            anyhow::Ok(())
        };
        if let Err(error) = log.await {
            self.kept().media = Some(known);
            return Err(error);
        }
        let log_bytes = log_bytes.load(Ordering::Relaxed);
        let report = |sent| meter.show(log_bytes + sent, log_bytes + media_bytes);
        let media = upload_media(opened.store.clone(), &self.me, &self.media, missing, &mut known, report).await;
        self.kept().media = Some(known);
        media?;
        self.report(|state| state.backed_up_at = Some(motile_protocol::now()));
        Ok(())
    }

    async fn copy_others(&self, opened: &Opened) -> anyhow::Result<()> {
        let servers = self.access.servers().await;
        if servers.is_empty() {
            return Ok(());
        }
        let deleted: HashSet<String> = self.access.deleted().await.into_iter().collect();
        let meter = Meter::new(self, false);
        let report = |done, total| meter.show(done, total);
        let hub = self.hub.clone();
        let origins = copy_others(&hub, opened.store.as_ref(), &self.me, &servers, &deleted, report).await?;
        self.kept().origins = origins;
        Ok(())
    }

    /// Fetches media this server doesn't have from the other servers' prefixes and keeps it.
    /// Whether it found it.
    pub async fn fetch_media(&self, id: &str) -> anyhow::Result<bool> {
        let (opened, origins) = {
            let kept = self.kept();
            (kept.opened.clone(), kept.origins.clone())
        };
        let Some(opened) = opened.filter(|_| is_id(id)) else { return Ok(false) };
        for origin in origins {
            let found = match opened.store.get(&Key::from(format!("{origin}/media/{id}"))).await {
                Ok(found) => found.bytes().await?,
                Err(object_store::Error::NotFound { .. }) => continue,
                Err(error) => return Err(error.into()),
            };
            keep_media(&self.media, id, &found)?;
            return Ok(true);
        }
        Ok(false)
    }
}

/// Says how far a part of a pass is, as one number that only goes up, once the part has run
/// longer than [`QUIET`].
struct Meter<'a> {
    backup: &'a Backup,
    uploading: bool,
    started: Instant,
    shown: AtomicU8,
}

impl<'a> Meter<'a> {
    fn new(backup: &'a Backup, uploading: bool) -> Self {
        Self { backup, uploading, started: Instant::now(), shown: AtomicU8::new(0) }
    }

    fn show(&self, done: u64, total: u64) {
        if total == 0 || self.started.elapsed() < QUIET {
            return;
        }
        let percent = (u128::from(done.min(total)) * 100 / u128::from(total)) as u8;
        let percent = self.shown.fetch_max(percent, Ordering::Relaxed).max(percent);
        let uploading = self.uploading;
        self.backup.report(|state| state.progress = Some(BackupProgress { uploading, percent }));
    }
}

/// An error as the user reads it: what failed, and what the bucket said, without the S3 client
/// telling each retry again.
fn said(error: &anyhow::Error) -> String {
    let mut ours = Vec::new();
    for cause in error.chain() {
        if cause.downcast_ref::<object_store::Error>().is_none() {
            ours.push(cause.to_string());
            continue;
        }
        let text = cause.to_string();
        let message = text.split_once("<Message>").and_then(|(_, rest)| rest.split_once("</Message>"));
        let deepest = std::iter::successors(Some(cause), |cause| cause.source()).last().map(ToString::to_string);
        ours.push(message.map(|(message, _)| message.to_string()).or(deepest).unwrap_or(text));
        break;
    }
    ours.join(" ")
}

fn without_secret(bucket: &Bucket) -> Bucket {
    Bucket { secret_key: String::new(), ..bucket.clone() }
}

/// Which bucket a mark belongs to: another one starts again from the beginning.
fn bucket_id(bucket: &Bucket) -> String {
    format!("{}/{}", bucket.endpoint, bucket.bucket)
}

fn open(bucket: &Bucket) -> anyhow::Result<Arc<dyn ObjectStore>> {
    let region = match bucket.region.trim() {
        "" => "us-east-1",
        region => region,
    };
    let mut builder = AmazonS3Builder::new()
        .with_bucket_name(bucket.bucket.trim())
        .with_region(region)
        .with_access_key_id(bucket.access_key.trim())
        .with_secret_access_key(bucket.secret_key.trim());
    let endpoint = bucket.endpoint.trim().trim_end_matches('/');
    if !endpoint.is_empty() {
        let endpoint = match endpoint.contains("://") {
            true => endpoint.to_string(),
            false => format!("https://{endpoint}"),
        };
        builder = builder.with_allow_http(endpoint.starts_with("http://")).with_endpoint(endpoint);
    }
    Ok(Arc::new(builder.build().context("That bucket's settings aren't complete.")?))
}

/// Writes, reads, lists and deletes a file under the server's key.
async fn check(bucket: &dyn ObjectStore, me: &str) -> anyhow::Result<()> {
    let probe = Key::from(format!("{me}/check"));
    bucket
        .put(&probe, PutPayload::from_static(b"motile"))
        .await
        .context("Your server couldn't write to the bucket.")?;
    let read = async { bucket.get(&probe).await?.bytes().await };
    read.await.context("Your server couldn't read from the bucket.")?;
    bucket.list_with_delimiter(Some(&Key::from(me))).await.context("Your server couldn't list the bucket.")?;
    bucket.delete(&probe).await.context("Your server couldn't delete from the bucket.")?;
    Ok(())
}

fn batch_key(origin: &str, log_id: &str, up_to: u64, after: u64) -> Key {
    Key::from(format!("{origin}/log/{log_id}/{up_to:020}-{after:020}.zst"))
}

fn parse_batch(name: &str) -> Option<(u64, u64)> {
    let (up_to, after) = name.strip_suffix(".zst")?.split_once('-')?;
    Some((up_to.parse().ok()?, after.parse().ok()?))
}

/// How much of its log a pass has put in the bucket so far.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LogSent {
    pub changes: u64,
    /// The changes the pass puts there.
    pub of: u64,
    pub bytes: u64,
}

impl LogSent {
    /// The bytes the whole pass will take, judged by what went so far.
    pub fn whole(&self) -> u64 {
        match self.changes {
            0 => self.bytes,
            changes => (u128::from(self.bytes) * u128::from(self.of.max(changes)) / u128::from(changes)) as u64,
        }
    }
}

/// Puts the server's log in the bucket after where `kept` says it is there, then its
/// `server.json`, and says how far the log is there now.
pub async fn upload_log<D: Database>(
    database: &Arc<D>,
    bucket: &dyn ObjectStore,
    me: &str,
    bucket_id: &str,
    kept: Option<Uploaded>,
    file: &ServerFile,
    report: impl Fn(LogSent),
) -> anyhow::Result<Uploaded> {
    let own = blocking(database, |store| Ok(store.watermark()?)).await?;
    let mut uploaded = match kept {
        Some(kept) if kept.bucket == bucket_id && kept.log_id == own.log_id => kept,
        _ => Uploaded { bucket: bucket_id.to_string(), log_id: own.log_id.clone(), change: 0 },
    };
    let start = uploaded.change;
    let mut bytes = 0;
    while uploaded.change < own.change {
        let after = uploaded.change;
        let (part, compressed) = blocking(database, move |store| {
            let part = store.changes(after)?;
            let compressed = part.compress()?;
            Ok((part, compressed))
        })
        .await?;
        if part.up_to <= after || part.log_id != uploaded.log_id {
            break;
        }
        bytes += compressed.len() as u64;
        bucket.put(&batch_key(me, &part.log_id, part.up_to, part.after), compressed.into()).await?;
        uploaded.change = part.up_to;
        report(LogSent { changes: uploaded.change - start, of: own.change - start, bytes });
        if !part.more {
            break;
        }
    }
    let file = ServerFile { log_id: uploaded.log_id.clone(), ..file.clone() };
    bucket.put(&Key::from(format!("{me}/{SERVER_FILE}")), serde_json::to_vec(&file)?.into()).await?;
    Ok(uploaded)
}

async fn listed_media(bucket: &dyn ObjectStore, me: &str) -> anyhow::Result<HashSet<String>> {
    let listed: Vec<_> = bucket.list(Some(&Key::from(format!("{me}/media")))).try_collect().await?;
    Ok(listed.into_iter().filter_map(|meta| meta.location.filename().map(String::from)).collect())
}

/// The media the bucket doesn't have yet, with their sizes.
fn missing_media(folder: &Path, known: &HashSet<String>) -> Vec<(String, u64)> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let sized = |entry: std::fs::DirEntry| Some((entry.file_name().into_string().ok()?, entry.metadata().ok()?.len()));
    entries.flatten().filter_map(sized).filter(|(name, _)| is_id(name) && !known.contains(name)).collect()
}

/// Puts the missing media there, saying how many bytes went.
pub async fn upload_media(
    bucket: Arc<dyn ObjectStore>,
    me: &str,
    folder: &Path,
    missing: Vec<(String, u64)>,
    known: &mut HashSet<String>,
    report: impl Fn(u64),
) -> anyhow::Result<()> {
    let (mut sent, mut chunk) = (0, vec![0; 1 << 20]);
    for (name, _) in missing {
        let mut file = tokio::fs::File::open(folder.join(&name)).await?;
        let mut upload = BufWriter::new(bucket.clone(), Key::from(format!("{me}/media/{name}")));
        loop {
            let read = file.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            upload.write_all(&chunk[..read]).await?;
            sent += read as u64;
            report(sent);
        }
        upload.shutdown().await?;
        known.insert(name);
    }
    Ok(())
}

/// Keeps a file fetched from the bucket, if it is what its name says.
fn keep_media(folder: &Path, id: &str, bytes: &[u8]) -> anyhow::Result<()> {
    let mut namer = Namer::default();
    namer.update(bytes);
    let extension = id.split_once('.').map_or("", |(_, extension)| extension);
    anyhow::ensure!(namer.id(extension) == id, "The bucket's copy of that file isn't what its name says.");
    std::fs::create_dir_all(folder)?;
    let unfinished = folder.join(format!("{}.part", uuid::Uuid::new_v4().simple()));
    std::fs::write(&unfinished, bytes)?;
    std::fs::rename(&unfinished, folder.join(id))?;
    Ok(())
}

/// The batches of an origin's log that a server's copy lacks.
struct Copy {
    origin: String,
    named: Named,
    log_id: String,
    /// How far the copy went, and the batches after, in order.
    have: u64,
    batches: Vec<(u64, u64, Key)>,
}

impl Copy {
    fn changes(&self) -> u64 {
        self.batches.last().map_or(0, |(up_to, _, _)| up_to - self.have)
    }
}

/// Copies what the bucket has of every other server of the account, and deletes what it has of
/// the servers the account deleted completely. Says whose logs it copied, and how many of the
/// changes it found it has copied as it goes.
pub async fn copy_others<D: Database>(
    database: &Arc<D>,
    bucket: &dyn ObjectStore,
    me: &str,
    servers: &HashMap<String, Named>,
    deleted: &HashSet<String>,
    report: impl Fn(u64, u64),
) -> anyhow::Result<Vec<String>> {
    let listed = bucket.list_with_delimiter(None).await?;
    let keys: Vec<String> = listed.common_prefixes.iter().map(|prefix| prefix.as_ref().to_string()).collect();
    let mut origins = Vec::new();
    let mut copies = Vec::new();
    let mut failed = None;
    for key in keys.iter().filter(|key| *key != me) {
        if deleted.contains(key) {
            if let Err(error) = delete_prefix(bucket, key).await {
                tracing::warn!(key, "couldn't delete a deleted server's backup: {error:#}");
            }
            continue;
        }
        let Some(named) = servers.get(key) else { continue };
        let file = match bucket.get(&Key::from(format!("{key}/{SERVER_FILE}"))).await {
            Ok(found) => serde_json::from_slice::<ServerFile>(&found.bytes().await?)?,
            Err(object_store::Error::NotFound { .. }) => continue,
            Err(error) => return Err(error.into()),
        };
        origins.push(key.clone());
        match lacking(database, bucket, key, named, &file.log_id).await {
            Ok(copy) => copies.push(copy),
            Err(error) => failed = Some(error.context(format!("Couldn't copy {}'s threads.", named.name))),
        }
    }
    let total = copies.iter().map(Copy::changes).sum();
    let mut done = 0;
    for copy in copies {
        let name = copy.named.name.clone();
        let changes = copy.changes();
        if let Err(error) = apply_copy(database, bucket, copy, |copied| report(done + copied, total)).await {
            failed = Some(error.context(format!("Couldn't copy {name}'s threads.")));
        }
        done += changes;
    }
    match failed {
        Some(error) => Err(error),
        None => Ok(origins),
    }
}

/// Lists what the bucket has of `origin`'s log after what this server's copy has.
async fn lacking<D: Database>(
    database: &Arc<D>,
    bucket: &dyn ObjectStore,
    origin: &str,
    named: &Named,
    log_id: &str,
) -> anyhow::Result<Copy> {
    let key = origin.to_string();
    let copy = blocking(database, move |store| Ok(store.origin_watermark(&key)?)).await?;
    let have = copy.filter(|copy| copy.log_id == log_id).map_or(0, |copy| copy.change);
    let prefix = Key::from(format!("{origin}/log/{log_id}"));
    // A batch is named by its last change first, so every one that ends after `have` sorts after this.
    let offset = Key::from(format!("{origin}/log/{log_id}/{have:020}-~"));
    let listed: Vec<_> = bucket.list_with_offset(Some(&prefix), &offset).try_collect().await?;
    let mut batches: Vec<(u64, u64, Key)> = listed
        .into_iter()
        .filter_map(|meta| {
            let (up_to, after) = parse_batch(meta.location.filename()?)?;
            Some((up_to, after, meta.location))
        })
        .filter(|(up_to, _, _)| *up_to > have)
        .collect();
    batches.sort_by_key(|(up_to, _, _)| *up_to);
    Ok(Copy { origin: origin.to_string(), named: named.clone(), log_id: log_id.to_string(), have, batches })
}

/// Applies the batches, saying how many changes of them it has copied.
async fn apply_copy<D: Database>(
    database: &Arc<D>,
    bucket: &dyn ObjectStore,
    copy: Copy,
    report: impl Fn(u64),
) -> anyhow::Result<()> {
    for (up_to, after, location) in copy.batches {
        let compressed = bucket.get(&location).await?.bytes().await?;
        let part = LogPart::decompress(&copy.log_id, after, up_to, &compressed)?;
        let (origin, named) = (copy.origin.clone(), copy.named.clone());
        blocking(database, move |store| store.apply_part(&origin, &named.name, &named.platform, &part, false)).await?;
        report(up_to - copy.have);
    }
    Ok(())
}

async fn delete_prefix(bucket: &dyn ObjectStore, key: &str) -> anyhow::Result<()> {
    let listed: Vec<_> = bucket.list(Some(&Key::from(key))).try_collect().await?;
    for meta in listed {
        bucket.delete(&meta.location).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    fn store() -> (tempfile::TempDir, Arc<Store>) {
        let folder = tempfile::tempdir().unwrap();
        let store = Store::open(&folder.path().join("motile.db")).unwrap();
        (folder, Arc::new(store))
    }

    fn add_thread(store: &Store, id: &str, items: usize) {
        store
            .connection()
            .execute(
                "INSERT INTO threads (id, title, title_source, project_id, cwd, agent, access, created_at, updated_at)
                 VALUES (?1, 'Title', 'user', 'p', '/tmp', 'claude', 'full', 0, 1)",
                [id],
            )
            .unwrap();
        for seq in 0..items {
            let item = serde_json::json!({
                "id": format!("{id}-{seq}"), "seq": seq, "rev": seq + 1, "created_at": 0.0,
                "type": "assistant", "text": format!("Reply {seq} in {id}")
            });
            store.save_item(id, &serde_json::from_value(item).unwrap()).unwrap();
        }
    }

    async fn objects(bucket: &InMemory) -> usize {
        bucket.list(None).try_collect::<_, _, Vec<_>>().await.unwrap().len()
    }

    fn file() -> ServerFile {
        ServerFile { name: "studio".into(), platform: "macos".into(), log_id: String::new() }
    }

    fn account(keys: &[&str]) -> HashMap<String, Named> {
        let named = |key: &&str| (key.to_string(), Named { name: format!("{key}-box"), platform: "linux".into() });
        keys.iter().map(named).collect()
    }

    fn thread_ids(store: &Store, origin: &str) -> Vec<String> {
        let mut ids: Vec<String> = store.copies(origin).unwrap().1.into_iter().map(|thread| thread.id).collect();
        ids.sort();
        ids
    }

    #[tokio::test]
    async fn a_servers_threads_reach_another_through_the_bucket_and_only_what_is_new_goes_up() {
        let bucket = InMemory::new();
        let (_a, a) = store();
        let (_b, b) = store();
        add_thread(&a, "kept", 3);
        add_thread(&a, "gone", 1);

        let sent = Mutex::new(None);
        let first = upload_log(&a, &bucket, "a", "bucket", None, &file(), |log| *sent.lock().unwrap() = Some(log))
            .await
            .unwrap();
        assert_eq!(first.change, a.watermark().unwrap().change);
        let sent = sent.into_inner().unwrap().unwrap();
        assert_eq!((sent.changes, sent.whole()), (sent.of, sent.bytes), "the pass ends with all of it there");
        let none = HashSet::new();
        let copied = Mutex::new(Vec::new());
        let report = |done, total| copied.lock().unwrap().push((done, total));
        let origins = copy_others(&b, &bucket, "b", &account(&["a", "b"]), &none, report).await.unwrap();
        assert_eq!(origins, vec!["a".to_string()]);
        let copied = copied.into_inner().unwrap();
        assert_eq!(copied.last(), Some(&(first.change, first.change)), "{copied:?}");
        assert_eq!(thread_ids(&b, "a"), vec!["gone", "kept"]);

        a.delete_thread("gone").unwrap();
        add_thread(&a, "new", 1);
        let before = objects(&bucket).await;
        let second = upload_log(&a, &bucket, "a", "bucket", Some(first), &file(), |_| {}).await.unwrap();
        assert_eq!(objects(&bucket).await, before + 1, "only the new batch goes up");
        copy_others(&b, &bucket, "b", &account(&["a", "b"]), &none, |_, _| {}).await.unwrap();

        assert_eq!(thread_ids(&b, "a"), vec!["kept", "new"]);
        assert_eq!(b.copied_items("a", "kept").unwrap().len(), 3);
        assert_eq!(b.origin_watermark("a").unwrap().unwrap().change, second.change);
        let copy_in_another_bucket =
            upload_log(&a, &bucket, "a", "other", Some(second), &file(), |_| {}).await.unwrap();
        assert_eq!(copy_in_another_bucket.change, a.watermark().unwrap().change, "another bucket gets the whole log");
    }

    #[tokio::test]
    async fn a_server_the_account_deleted_is_deleted_from_the_bucket_and_a_stranger_is_left_alone() {
        let bucket = InMemory::new();
        let (_a, a) = store();
        let (_b, b) = store();
        add_thread(&a, "t", 1);
        upload_log(&a, &bucket, "a", "bucket", None, &file(), |_| {}).await.unwrap();
        upload_log(&a, &bucket, "stranger", "bucket", None, &file(), |_| {}).await.unwrap();

        let deleted = HashSet::from(["a".to_string()]);
        let origins = copy_others(&b, &bucket, "b", &account(&["b"]), &deleted, |_, _| {}).await.unwrap();

        assert!(origins.is_empty());
        assert!(thread_ids(&b, "a").is_empty() && thread_ids(&b, "stranger").is_empty());
        let left: Vec<_> = bucket.list(None).try_collect().await.unwrap();
        assert!(left.iter().all(|meta| meta.location.as_ref().starts_with("stranger/")), "{left:?}");
        assert!(!left.is_empty());
    }

    #[test]
    fn a_log_is_judged_whole_by_what_its_first_batches_took() {
        assert_eq!(LogSent { changes: 0, of: 100, bytes: 0 }.whole(), 0);
        assert_eq!(LogSent { changes: 25, of: 100, bytes: 1000 }.whole(), 4000);
        assert_eq!(LogSent { changes: 120, of: 100, bytes: 1000 }.whole(), 1000, "changes made while it went");
    }

    #[test]
    fn an_error_says_what_failed_and_what_the_bucket_said() {
        let refused = object_store::Error::Generic {
            store: "S3",
            source: "Error performing PUT: <Error><Code>AccessDenied</Code><Message>Access Denied</Message></Error>"
                .into(),
        };
        let error = anyhow::Error::from(refused).context("Your server couldn't write to the bucket.");
        assert_eq!(said(&error), "Your server couldn't write to the bucket. Access Denied");
    }

    #[tokio::test]
    async fn media_goes_up_once_and_comes_back_only_as_what_its_name_says() {
        let bucket: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let folder = tempfile::tempdir().unwrap();
        let mut namer = Namer::default();
        namer.update(b"picture");
        let id = namer.id("png");
        std::fs::write(folder.path().join(&id), b"picture").unwrap();
        std::fs::write(folder.path().join("unfinished.part"), b"half").unwrap();

        let mut known = HashSet::new();
        let missing = missing_media(folder.path(), &known);
        assert_eq!(missing, vec![(id.clone(), 7)], "only what is named by its contents");
        let sent = AtomicU64::new(0);
        let report = |bytes| sent.store(bytes, Ordering::Relaxed);
        upload_media(bucket.clone(), "a", folder.path(), missing, &mut known, report).await.unwrap();
        assert_eq!(sent.load(Ordering::Relaxed), 7);
        assert!(missing_media(folder.path(), &known).is_empty());
        assert_eq!(known, HashSet::from([id.clone()]));
        assert_eq!(listed_media(bucket.as_ref(), "a").await.unwrap(), known);

        let elsewhere = tempfile::tempdir().unwrap();
        let fetched = bucket.get(&Key::from(format!("a/media/{id}"))).await.unwrap().bytes().await.unwrap();
        keep_media(elsewhere.path(), &id, &fetched).unwrap();
        assert_eq!(std::fs::read(elsewhere.path().join(&id)).unwrap(), b"picture");
        assert!(keep_media(elsewhere.path(), &id, b"something else").is_err());
    }
}
