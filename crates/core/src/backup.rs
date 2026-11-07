//! The account's S3 bucket as this device knows it: the keys it keeps beside its device key, which
//! it gives to a server that has none, and what Settings → Servers says of the backup.

use std::path::{Path, PathBuf};

use motile_protocol::wire::{BackupState, Bucket, ServerInfo};

use crate::api::{BackupView, SyncState, SyncView};

/// The first protocol whose servers back up to a bucket.
pub const BACKUP_PROTOCOL: u32 = 39;

pub struct Saved {
    file: PathBuf,
    pub bucket: Option<Bucket>,
}

impl Saved {
    pub fn load(data_dir: &Path) -> Self {
        let file = data_dir.join("backup.json");
        let bucket = std::fs::read_to_string(&file).ok().and_then(|json| serde_json::from_str(&json).ok());
        Self { file, bucket }
    }

    pub fn save(&mut self, bucket: Option<Bucket>) -> std::io::Result<()> {
        match &bucket {
            Some(bucket) => write_private(&self.file, &serde_json::to_string(bucket)?)?,
            None => match std::fs::remove_file(&self.file) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
                _ => {}
            },
        }
        self.bucket = bucket;
        Ok(())
    }
}

fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    let unfinished = path.with_extension("json.part");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(&unfinished)?.write_all(contents.as_bytes())?;
    std::fs::rename(&unfinished, path)
}

/// Where the bucket is, as a line says it: "motile on 4f1c.r2.cloudflarestorage.com".
fn place(bucket: &Bucket) -> String {
    let endpoint = bucket.endpoint.trim();
    let host = endpoint.split_once("://").map_or(endpoint, |(_, rest)| rest).trim_end_matches('/');
    match (host.is_empty(), bucket.region.trim()) {
        (false, _) => format!("{} on {host}", bucket.bucket),
        (true, "") => format!("{} on AWS", bucket.bucket),
        (true, region) => format!("{} on AWS · {region}", bucket.bucket),
    }
}

/// The backup as Settings shows it: the bucket this device keeps, or else the one a server uses.
pub fn view<'a>(saved: Option<&Bucket>, infos: impl Iterator<Item = &'a ServerInfo>) -> BackupView {
    let used = || infos.filter_map(|info| info.backup.as_ref()).map(|state| state.bucket.clone()).next();
    let bucket = saved.cloned().or_else(used);
    let description = match &bucket {
        Some(bucket) => place(bucket),
        None => "Keep every server's threads, images and files in your own S3 bucket, and sync your servers through it"
            .into(),
    };
    BackupView { bucket, description, saved: saved.is_some() }
}

/// What the line under a server says of its backup.
pub fn line(state: Option<&BackupState>, failed: Option<&String>, saved: bool) -> SyncView {
    let view =
        |state, text: &str, error: Option<String>, synced_at| SyncView { state, text: text.into(), error, synced_at };
    let Some(backup) = state else {
        return match (failed, saved) {
            (Some(error), _) => view(SyncState::Failed, "Couldn't use the bucket", Some(error.clone()), None),
            (None, true) => view(SyncState::Copying, "Setting up the backup", None, None),
            (None, false) => {
                let why = "Its keys aren't on this device. Set the backup up again here to give them to it.";
                view(SyncState::Waiting, "Not backed up", Some(why.into()), None)
            }
        };
    };
    if let Some(progress) = &backup.progress {
        let doing = if progress.uploading { "Backing up" } else { "Syncing" };
        return view(SyncState::Copying, &format!("{doing} · {}%", progress.percent), None, None);
    }
    if let Some(error) = &backup.error {
        return view(SyncState::Failed, "Couldn't back up", Some(error.clone()), backup.backed_up_at);
    }
    match backup.backed_up_at {
        Some(at) => view(SyncState::Synced, "Backed up", None, Some(at)),
        None => view(SyncState::Copying, "Backing up", None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motile_protocol::wire::BackupProgress;

    fn bucket(endpoint: &str, region: &str) -> Bucket {
        Bucket { endpoint: endpoint.into(), region: region.into(), bucket: "motile".into(), ..Bucket::default() }
    }

    #[test]
    fn the_bucket_is_named_with_where_it_is() {
        assert_eq!(
            place(&bucket("https://4f1c.r2.cloudflarestorage.com/", "auto")),
            "motile on 4f1c.r2.cloudflarestorage.com"
        );
        assert_eq!(place(&bucket("", "eu-west-1")), "motile on AWS · eu-west-1");
        assert_eq!(place(&bucket("", "")), "motile on AWS");
    }

    #[test]
    fn the_keys_live_in_a_file_only_this_user_reads_until_the_backup_stops() {
        let folder = tempfile::tempdir().unwrap();
        let mut saved = Saved::load(folder.path());
        assert!(saved.bucket.is_none());
        saved.save(Some(bucket("", "eu-west-1"))).unwrap();
        assert_eq!(Saved::load(folder.path()).bucket, Some(bucket("", "eu-west-1")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(folder.path().join("backup.json")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        saved.save(None).unwrap();
        saved.save(None).unwrap();
        assert!(Saved::load(folder.path()).bucket.is_none());
    }

    #[test]
    fn a_servers_line_says_how_far_its_backup_is() {
        let state = |progress, error: Option<&str>, at| BackupState {
            bucket: bucket("", ""),
            backed_up_at: at,
            progress,
            error: error.map(String::from),
        };
        let text = |view: SyncView| (view.state, view.text);
        let uploading = Some(BackupProgress { uploading: true, percent: 40 });
        assert_eq!(
            text(line(Some(&state(uploading, None, None)), None, true)),
            (SyncState::Copying, "Backing up · 40%".into())
        );
        let copying = Some(BackupProgress { uploading: false, percent: 7 });
        assert_eq!(
            text(line(Some(&state(copying, None, None)), None, true)),
            (SyncState::Copying, "Syncing · 7%".into())
        );
        assert_eq!(text(line(Some(&state(None, Some("denied"), Some(5.0))), None, true)).1, "Couldn't back up");
        let done = line(Some(&state(None, None, Some(5.0))), None, false);
        assert_eq!((done.state, done.synced_at), (SyncState::Synced, Some(5.0)));
        assert_eq!(text(line(None, None, false)), (SyncState::Waiting, "Not backed up".into()));
        assert_eq!(text(line(None, None, true)).1, "Setting up the backup");
        assert_eq!(text(line(None, Some(&"denied".to_string()), true)).1, "Couldn't use the bucket");
    }
}
