//! Finds out which version the newest release has, which the servers are compared with to offer
//! updating them, and which this client says in its settings. The releases don't carry this
//! client yet, so a new version is offered as a download, not installed.

use std::time::Duration;

use gpui_kit::*;

use crate::models::is_older;
use crate::store::Store;

const LATEST_RELEASE: &str = "https://github.com/motileapp/motile/releases/latest";
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
/// How long "the newest version" stays said after the user asked.
const UP_TO_DATE_FOR: Duration = Duration::from_secs(4);

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(String),
    Failed(String),
}

pub struct Updater {
    pub current: &'static str,
    pub latest: Option<String>,
    pub state: UpdateState,
    _timer: Option<Task<()>>,
    _check: Option<Task<()>>,
}

impl Updater {
    pub fn new() -> Self {
        Self { current: env!("CARGO_PKG_VERSION"), latest: None, state: UpdateState::Idle, _timer: None, _check: None }
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
        if self.state == UpdateState::Checking {
            return;
        }
        if asked {
            self.state = UpdateState::Checking;
            cx.notify();
        }
        self._check = Some(cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async move { latest_version() }).await;
            let _ = this.update(cx, |store, cx| {
                store.updater.found(found, asked);
                cx.notify();
            });
            if !asked || this.read_with(cx, |store, _| store.updater.state != UpdateState::UpToDate).unwrap_or(true) {
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

    /// Opens the newest release's page, where the download is.
    pub fn download(&self, cx: &mut App) {
        cx.open_url(LATEST_RELEASE);
    }
}

/// The version of the newest release: the address of the latest release redirects to the
/// release's own page, whose last part is its tag.
fn latest_version() -> Option<String> {
    motile_protocol::tls::install();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
    runtime.block_on(async {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(20)).build().ok()?;
        let response = client.head(LATEST_RELEASE).send().await.ok()?;
        let tag = response.url().path_segments()?.next_back()?.to_string();
        tag.strip_prefix('v').map(String::from)
    })
}
