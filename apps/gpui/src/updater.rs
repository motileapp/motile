//! Finds out which version the newest release has, which the servers are compared with, and
//! replaces this client with the release's on Linux and Windows: it downloads the release's
//! archive for this system, puts the program in it in this one's place and starts it. The Mac's
//! client is the Mac app, so on the Mac a new version is offered as the release's page.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use futures::StreamExt;
use gpui_kit::*;

use crate::models::is_older;
use crate::store::Store;

const LATEST_RELEASE: &str = "https://github.com/motileapp/motile/releases/latest";
/// What a release calls this client's archive for the system and processor it runs on, which
/// holds the program in a folder of its name, as `scripts/package.sh` writes it.
const ASSET: Option<&str> = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("Motile-GPUI-linux-x86_64.tar.gz")
} else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
    Some("Motile-GPUI-linux-aarch64.tar.gz")
} else if cfg!(all(windows, target_arch = "x86_64")) {
    Some("Motile-GPUI-windows-x86_64.zip")
} else if cfg!(all(windows, target_arch = "aarch64")) {
    Some("Motile-GPUI-windows-aarch64.zip")
} else {
    None
};
const FOLDER: &str = "motile-gpui";
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
/// How long "Up to date" stays said after the user asked.
const UP_TO_DATE_FOR: Duration = Duration::from_secs(4);
/// Given to the copy an update starts, which the one that started it is still quitting for. An
/// argument, not a variable, so that the programs it starts don't pass it on.
const RELAUNCHED: &str = "--relaunched";

#[derive(Clone, PartialEq, Debug, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    /// The user asked and the answer isn't here yet.
    Checking,
    UpToDate,
    Available(String),
    Downloading(String, f64),
    Installing(String),
    /// Installed; the running client is still the old one.
    Ready(String),
    Failed(String),
}

impl UpdateState {
    /// Whether the sidebar has a row for it: something to do or to wait for.
    pub fn shows_row(&self) -> bool {
        !matches!(self, UpdateState::Idle | UpdateState::Checking | UpdateState::UpToDate)
    }
}

/// How a download goes, from the thread that runs it.
enum Progress {
    Fraction(f64),
    /// The release doesn't carry this client.
    Missing,
    Done(Result<PathBuf, String>),
}

pub struct Updater {
    pub current: &'static str,
    /// The newest release's version. The servers are compared with it as well.
    pub latest: Option<String>,
    pub state: UpdateState,
    cancelled: Arc<AtomicBool>,
    _timer: Option<Task<()>>,
    _check: Option<Task<()>>,
    _download: Option<Task<()>>,
}

impl Updater {
    pub fn new() -> Self {
        Self {
            current: release_version(),
            latest: None,
            state: UpdateState::Idle,
            cancelled: Arc::default(),
            _timer: None,
            _check: None,
            _download: None,
        }
    }

    /// Looks for a new release now and every few hours.
    pub fn start(&mut self, cx: &mut Context<Store>) {
        self._timer = Some(cx.spawn(async move |this, cx| {
            loop {
                let heard = this.update(cx, |store, cx| store.updater.check(false, cx));
                if heard.is_err() {
                    break;
                }
                cx.background_executor().timer(CHECK_EVERY).await;
            }
        }));
    }

    /// `asked` is for when the user wants to know: finding nothing new is then said too.
    pub fn check(&mut self, asked: bool, cx: &mut Context<Store>) {
        if matches!(
            self.state,
            UpdateState::Checking | UpdateState::Downloading(..) | UpdateState::Installing(_) | UpdateState::Ready(_)
        ) {
            return;
        }
        if asked {
            self.state = UpdateState::Checking;
            cx.notify();
        }
        self._check = Some(cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async move { latest_version() }).await;
            let _ = this.update(cx, |store, cx| {
                let heard = found.is_some();
                store.updater.found(found, asked);
                if heard {
                    store.show_server_updates(asked);
                }
                cx.notify();
            });
            if this.read_with(cx, |store, _| store.updater.state != UpdateState::UpToDate).unwrap_or(true) {
                return;
            }
            cx.background_executor().timer(UP_TO_DATE_FOR).await;
            let _ = this.update(cx, |store, cx| {
                if store.updater.state == UpdateState::UpToDate {
                    store.updater.state = UpdateState::Idle;
                    cx.notify();
                }
            });
        }));
    }

    fn found(&mut self, version: Option<String>, asked: bool) {
        let Some(version) = version else {
            if asked {
                self.state = UpdateState::Failed("GitHub couldn’t be reached to look for a new version.".into());
            }
            return;
        };
        self.latest = Some(version.clone());
        if is_older(self.current, Some(&version)) {
            self.state = UpdateState::Available(version);
        } else if asked {
            self.state = UpdateState::UpToDate;
        } else if self.state == UpdateState::Checking {
            self.state = UpdateState::Idle;
        }
    }

    /// Downloads and installs the new version at once, unless this copy can't replace itself;
    /// on the Mac, or for a release without this client, its page opens instead.
    pub fn install(&mut self, cx: &mut Context<Store>) {
        let UpdateState::Available(version) = self.state.clone() else { return };
        let Some(asset) = ASSET else {
            cx.open_url(LATEST_RELEASE);
            return;
        };
        if let Some(obstacle) = obstacle() {
            self.state = UpdateState::Failed(obstacle);
            return;
        }
        self.state = UpdateState::Downloading(version.clone(), 0.);
        self.download(version, asset, cx);
    }

    fn download(&mut self, version: String, asset: &'static str, cx: &mut Context<Store>) {
        let url = format!("https://github.com/motileapp/motile/releases/download/v{version}/{asset}");
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancelled = cancelled.clone();
        let (sender, mut progress) = futures::channel::mpsc::unbounded::<Progress>();
        std::thread::spawn(move || {
            let done = fetch(&url, asset, &sender, &cancelled);
            let _ = sender.unbounded_send(Progress::Done(done));
        });
        self._download = Some(cx.spawn(async move |this, cx| {
            while let Some(step) = progress.next().await {
                let finished = match step {
                    Progress::Missing => {
                        let _ = this.update(cx, |store, cx| {
                            if matches!(store.updater.state, UpdateState::Downloading(..)) {
                                store.updater.state = UpdateState::Available(version.clone());
                                cx.open_url(LATEST_RELEASE);
                                cx.notify();
                            }
                        });
                        return;
                    }
                    Progress::Fraction(fraction) => {
                        let _ = this.update(cx, |store, cx| {
                            if matches!(store.updater.state, UpdateState::Downloading(..)) {
                                store.updater.state = UpdateState::Downloading(version.clone(), fraction);
                                cx.notify();
                            }
                        });
                        continue;
                    }
                    Progress::Done(finished) => finished,
                };
                let archive = match finished {
                    Ok(archive) => archive,
                    Err(message) => {
                        let _ = this.update(cx, |store, cx| {
                            if matches!(store.updater.state, UpdateState::Downloading(..)) {
                                store.updater.state = UpdateState::Failed(message);
                                cx.notify();
                            }
                        });
                        return;
                    }
                };
                let installing = this.update(cx, |store, cx| {
                    let downloading = matches!(store.updater.state, UpdateState::Downloading(..));
                    if downloading {
                        store.updater.state = UpdateState::Installing(version.clone());
                        cx.notify();
                    }
                    downloading
                });
                if !installing.unwrap_or(false) {
                    return;
                }
                let installed = cx.background_executor().spawn(async move { replace_this_app(&archive) }).await;
                let _ = this.update(cx, |store, cx| {
                    store.updater.state = match installed {
                        Ok(()) => UpdateState::Ready(version.clone()),
                        Err(message) => UpdateState::Failed(message),
                    };
                    cx.notify();
                });
                return;
            }
        }));
    }

    /// Stops the download and offers the update again.
    pub fn cancel_download(&mut self) {
        let UpdateState::Downloading(version, _) = self.state.clone() else { return };
        self.cancelled.store(true, Ordering::Relaxed);
        self._download = None;
        self.state = UpdateState::Available(version);
    }

    /// Back to offering the update, after it failed.
    pub fn retry(&mut self, cx: &mut Context<Store>) {
        self.state = UpdateState::Idle;
        self.check(true, cx);
    }

    /// Shows a state without anything happening, for the dev app's pictures.
    pub fn show(&mut self, state: UpdateState, latest: Option<String>) {
        self.state = state;
        if latest.is_some() {
            self.latest = latest;
        }
    }
}

/// This client's version, which is the release's: the version of the repository's Cargo.toml,
/// as the release workflow writes it, and as the Mac app takes its own.
pub fn release_version() -> &'static str {
    const WORKSPACE: &str = include_str!("../../../Cargo.toml");
    WORKSPACE
        .lines()
        .find_map(|line| line.strip_prefix("version = \"")?.strip_suffix('"'))
        .unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// Starts the installed version, and quits this one.
pub fn relaunch(cx: &mut App) {
    let Some(program) = program() else { return };
    let started = std::process::Command::new(program).arg(RELAUNCHED).spawn();
    if started.is_ok() {
        cx.quit();
    }
}

/// Whether an update started this copy, which then doesn't hand itself over to the one quitting.
#[cfg(not(target_os = "macos"))]
pub fn relaunched() -> bool {
    std::env::args().skip(1).any(|arg| arg == RELAUNCHED)
}

/// The program this copy runs, as it was named when it started: once replaced, the system
/// names the running one by where it went.
fn program() -> Option<&'static Path> {
    static PROGRAM: OnceLock<Option<PathBuf>> = OnceLock::new();
    PROGRAM.get_or_init(|| std::env::current_exe().ok()).as_deref()
}

/// What an update on Windows set aside: the program that ran while it was replaced.
pub fn remove_replaced() {
    let Some(program) = program() else { return };
    let _ = std::fs::remove_file(set_aside(program));
}

fn set_aside(program: &Path) -> PathBuf {
    program.with_extension("old")
}

fn runtime() -> Option<tokio::runtime::Runtime> {
    motile_protocol::tls::install();
    tokio::runtime::Builder::new_current_thread().enable_all().build().ok()
}

/// The version of the newest release: the address of the latest release redirects to the
/// release's own page, whose last part is its tag.
fn latest_version() -> Option<String> {
    runtime()?.block_on(async {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(20)).build().ok()?;
        let response = client.head(LATEST_RELEASE).send().await.ok()?;
        let tag = response.url().path_segments()?.next_back()?.to_string();
        tag.strip_prefix('v').map(String::from)
    })
}

/// Downloads the archive into a folder of its own, saying how far it has come.
fn fetch(
    url: &str,
    asset: &str,
    progress: &futures::channel::mpsc::UnboundedSender<Progress>,
    cancelled: &AtomicBool,
) -> Result<PathBuf, String> {
    use std::io::Write;
    let runtime = runtime().ok_or("The download couldn’t start.")?;
    let folder = std::env::temp_dir().join(format!("motile-update-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).map_err(|error| format!("The download couldn’t be saved: {error}"))?;
    let archive = folder.join(asset);
    let mut file =
        std::fs::File::create(&archive).map_err(|error| format!("The download couldn’t be saved: {error}"))?;
    runtime.block_on(async {
        let failed = |error: reqwest::Error| format!("The new version couldn’t be downloaded: {error}");
        let client = reqwest::Client::builder().build().map_err(failed)?;
        let mut response = client.get(url).send().await.map_err(failed)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            let _ = progress.unbounded_send(Progress::Missing);
            return Err(String::new());
        }
        if !status.is_success() {
            return Err(format!("The new version couldn’t be downloaded ({}).", status.as_u16()));
        }
        let total = response.content_length().filter(|total| *total > 0);
        let mut received = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(failed)? {
            if cancelled.load(Ordering::Relaxed) {
                return Err("The download was cancelled.".into());
            }
            file.write_all(&chunk).map_err(|error| format!("The download couldn’t be saved: {error}"))?;
            received += chunk.len() as u64;
            if let Some(total) = total {
                let _ = progress.unbounded_send(Progress::Fraction(received as f64 / total as f64));
            }
        }
        Ok(archive)
    })
}

/// Why this copy can't replace itself, if it can't.
fn obstacle() -> Option<String> {
    if cfg!(debug_assertions) {
        return Some("This copy of Motile was built here, so it can’t update itself.".into());
    }
    program().is_none().then(|| "Motile couldn’t find where it runs from.".into())
}

/// Unpacks the archive and puts the program in it where this one is.
fn replace_this_app(archive: &Path) -> Result<(), String> {
    let folder = archive.parent().ok_or("The download couldn’t be read.")?;
    let result = install_from(archive, folder);
    let _ = std::fs::remove_dir_all(folder);
    result
}

fn install_from(archive: &Path, folder: &Path) -> Result<(), String> {
    let program = program().ok_or("Motile couldn’t find where it runs from.")?;
    let unpacked = unpack(archive, folder);
    let new_program = folder.join(FOLDER).join(format!("{FOLDER}{}", std::env::consts::EXE_SUFFIX));
    if !unpacked || !new_program.is_file() {
        return Err("The download isn’t a copy of Motile.".into());
    }
    let replaced = |error: std::io::Error| format!("Motile couldn’t be replaced where it is: {error}");
    // Copied beside the program first, so that it takes the program's place in one step.
    let beside = program.with_file_name(format!(".{FOLDER}-{}", uuid::Uuid::new_v4()));
    std::fs::copy(&new_program, &beside).map_err(replaced)?;
    let moved = take_place(&beside, program);
    if moved.is_err() {
        let _ = std::fs::remove_file(&beside);
    }
    moved.map_err(replaced)
}

/// The system's own `tar`, which unpacks a gzipped tarball on Linux and a zip on Windows.
fn unpack(archive: &Path, folder: &Path) -> bool {
    #[cfg(windows)]
    let mut tar = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let mut tar = std::process::Command::new(system.join(r"System32\tar.exe"));
        tar.creation_flags(CREATE_NO_WINDOW);
        tar
    };
    #[cfg(not(windows))]
    let mut tar = std::process::Command::new("tar");
    tar.arg("-xf").arg(archive).arg("-C").arg(folder).status().is_ok_and(|status| status.success())
}

/// Linux lets a running program be renamed over; it keeps running from the file it started with.
#[cfg(unix)]
fn take_place(new_program: &Path, program: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(new_program, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(new_program, program)
}

/// Windows doesn't let a running program be replaced, only renamed: it is set aside, and removed
/// when the new one starts.
#[cfg(windows)]
fn take_place(new_program: &Path, program: &Path) -> std::io::Result<()> {
    let aside = set_aside(program);
    let _ = std::fs::remove_file(&aside);
    std::fs::rename(program, &aside)?;
    std::fs::rename(new_program, program).inspect_err(|_| {
        let _ = std::fs::rename(&aside, program);
    })
}
