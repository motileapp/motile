//! The browser sheet for signing in, as the Mac app shows it: the system's web authentication
//! session, which hands back the `motile://auth` address the sign-in ends on.

use futures::channel::oneshot;

/// A sign-in that is open. It has to be kept until it ends.
pub struct Session {
    #[cfg(target_os = "macos")]
    _session: objc2::rc::Retained<objc2_authentication_services::ASWebAuthenticationSession>,
    #[cfg(target_os = "macos")]
    _anchor: objc2::rc::Retained<macos::Anchor>,
}

/// Starts the sign-in at `url`. The answer is the address it was sent back to, or nothing when it
/// was given up.
#[cfg(target_os = "macos")]
pub fn start(url: &str) -> Option<(Session, oneshot::Receiver<Option<String>>)> {
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
            Some(&NSString::from_str("motile")),
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

#[cfg(not(target_os = "macos"))]
pub fn start(_: &str) -> Option<(Session, oneshot::Receiver<Option<String>>)> {
    None
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
