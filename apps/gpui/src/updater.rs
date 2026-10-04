//! Finds out which version the newest release has, which the servers are compared with to offer
//! updating them. The releases don't carry this app yet, so it doesn't offer to update itself.

use std::time::Duration;

use gpui_kit::*;

use crate::store::Store;

const LATEST_RELEASE: &str = "https://github.com/motileapp/motile/releases/latest";
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);

pub struct Updater {
    pub latest: Option<String>,
    _timer: Option<Task<()>>,
}

impl Updater {
    pub fn new() -> Self {
        Self { latest: None, _timer: None }
    }

    /// Looks for a new release now and every few hours.
    pub fn start(&mut self, cx: &mut Context<Store>) {
        self._timer = Some(cx.spawn(async move |this, cx| {
            loop {
                let found = cx.background_executor().spawn(async move { latest_version() }).await;
                let heard = this.update(cx, |store, cx| {
                    let Some(version) = found else { return };
                    store.updater.latest = Some(version);
                    cx.notify();
                });
                if heard.is_err() {
                    break;
                }
                cx.background_executor().timer(CHECK_EVERY).await;
            }
        }));
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
