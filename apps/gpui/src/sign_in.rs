//! Signing in, or connecting Linear, in the browser. On the Mac it is the browser sheet the Mac
//! app shows, the system's web authentication session, which hands back the `motile://` address
//! it ends on. Elsewhere it is the browser itself, whose address the system starts the client with
//! (`instance.rs`), and the copy that runs hands it to the sign-in waiting for it.

use futures::channel::oneshot;
use gpui_kit::App;

/// A sign-in that is open. It has to be kept until it ends; dropping it gives it up.
pub struct Session {
    #[cfg(target_os = "macos")]
    _session: objc2::rc::Retained<objc2_authentication_services::ASWebAuthenticationSession>,
    #[cfg(target_os = "macos")]
    _anchor: objc2::rc::Retained<macos::Anchor>,
    #[cfg(not(target_os = "macos"))]
    id: u64,
}

/// Starts the sign-in at `url`, which ends at `ends_at`. The answer is the address it was sent
/// back to, or nothing when it was given up.
#[cfg(target_os = "macos")]
pub fn start(url: &str, ends_at: &'static str, _: &App) -> Option<(Session, oneshot::Receiver<Option<String>>)> {
    use std::cell::RefCell;

    use block2::RcBlock;
    use objc2::runtime::ProtocolObject;
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_authentication_services::ASWebAuthenticationSession;
    use objc2_foundation::{NSError, NSString, NSURL};

    let main = MainThreadMarker::new()?;
    let url = NSURL::URLWithString(&NSString::from_str(url))?;
    let (sender, receiver) = oneshot::channel();
    let sender = RefCell::new(Some(sender));
    let done = RcBlock::new(move |callback: *mut NSURL, _: *mut NSError| {
        let address =
            unsafe { callback.as_ref() }.and_then(|url| url.absoluteString()).map(|address| address.to_string());
        if let Some(sender) = sender.borrow_mut().take() {
            let _ = sender.send(address);
        }
    });
    #[allow(deprecated)]
    let session = unsafe {
        ASWebAuthenticationSession::initWithURL_callbackURLScheme_completionHandler(
            ASWebAuthenticationSession::alloc(),
            &url,
            Some(&NSString::from_str(ends_at.split("://").next().unwrap_or(ends_at))),
            &*done as *const _ as *mut _,
        )
    };
    let anchor = macos::Anchor::new(main);
    unsafe {
        session.setPresentationContextProvider(Some(ProtocolObject::from_ref(&*anchor)));
        if !session.start() {
            return None;
        }
    }
    Some((Session { _session: session, _anchor: anchor }, receiver))
}

/// Whether a sign-in was waiting for the address, which it is then given. The Mac's sheet hands
/// its own back.
#[cfg(target_os = "macos")]
pub fn deliver(_: &str) -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn start(url: &str, ends_at: &'static str, cx: &App) -> Option<(Session, oneshot::Receiver<Option<String>>)> {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (sender, receiver) = oneshot::channel();
    let mut waiting = browser::WAITING.lock().ok()?;
    waiting.retain(|waiter| waiter.ends_at != ends_at);
    waiting.push(browser::Waiter { id, ends_at, sender });
    drop(waiting);
    cx.open_url(url);
    Some((Session { id }, receiver))
}

#[cfg(not(target_os = "macos"))]
pub fn deliver(address: &str) -> bool {
    let Ok(mut waiting) = browser::WAITING.lock() else { return false };
    let Some(index) = waiting.iter().position(|waiter| browser::ends_at(address, waiter.ends_at)) else {
        return false;
    };
    let _ = waiting.remove(index).sender.send(Some(address.to_string()));
    true
}

#[cfg(not(target_os = "macos"))]
impl Drop for Session {
    fn drop(&mut self) {
        if let Ok(mut waiting) = browser::WAITING.lock() {
            waiting.retain(|waiter| waiter.id != self.id);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod browser {
    use std::sync::Mutex;

    use futures::channel::oneshot;

    pub struct Waiter {
        pub id: u64,
        pub ends_at: &'static str,
        pub sender: oneshot::Sender<Option<String>>,
    }

    pub static WAITING: Mutex<Vec<Waiter>> = Mutex::new(Vec::new());

    /// Whether the address is the one a sign-in ends at, with what it was sent back.
    pub fn ends_at(address: &str, end: &str) -> bool {
        address.strip_prefix(end).is_some_and(|rest| rest.is_empty() || rest.starts_with(['?', '#', '/']))
    }

    #[cfg(test)]
    mod tests {
        use super::ends_at;

        #[test]
        fn an_address_ends_a_sign_in_only_at_its_own_end() {
            assert!(ends_at("motile://auth?code=a&state=b", "motile://auth"));
            assert!(ends_at("motile://linear", "motile://linear"));
            assert!(!ends_at("motile://auth.evil?code=a", "motile://auth"));
            assert!(!ends_at("motile://linear?code=a", "motile://auth"));
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::NSApplication;
    use objc2_authentication_services::{ASWebAuthenticationPresentationContextProviding, ASWebAuthenticationSession};

    define_class!(
        /// Says which window the sheet is shown over: the key one.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "MotileSignInAnchor"]
        pub struct Anchor;

        unsafe impl NSObjectProtocol for Anchor {}

        unsafe impl ASWebAuthenticationPresentationContextProviding for Anchor {
            #[unsafe(method_id(presentationAnchorForWebAuthenticationSession:))]
            fn presentation_anchor(&self, _: &ASWebAuthenticationSession) -> Retained<NSObject> {
                let app = NSApplication::sharedApplication(self.mtm());
                let window = app.keyWindow().or_else(|| app.windows().firstObject());
                match window {
                    Some(window) => Retained::into_super(Retained::into_super(window)),
                    None => NSObject::new(),
                }
            }
        }
    );

    impl Anchor {
        pub fn new(main: MainThreadMarker) -> Retained<Self> {
            unsafe { msg_send![Self::alloc(main), init] }
        }
    }
}
