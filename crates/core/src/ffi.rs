//! The C functions the clients call. Everything crossing the boundary is JSON text: commands go in
//! through `motile_send`, events come out through the callback given to `motile_start`.

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Arc, OnceLock};

use tokio::runtime::Runtime;

use crate::api::{Config, Envelope, Event};
use crate::core::{self, Handle};

/// Called on a background thread with one event as NUL-terminated JSON. The text is only valid
/// during the call.
pub type EventCallback = extern "C" fn(event_json: *const c_char, context: *mut c_void);

struct Running {
    _runtime: Runtime,
    handle: Handle,
}

static RUNNING: OnceLock<Running> = OnceLock::new();

/// The client's context pointer, which it promises is safe to hand to any thread.
struct Context(*mut c_void);
unsafe impl Send for Context {}
unsafe impl Sync for Context {}

fn text(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(pointer) }.to_str().ok().map(String::from)
}

/// Starts the core. `config_json` is an `api::Config`. Returns false if it couldn't start;
/// calling it again once started does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn motile_start(config_json: *const c_char, callback: EventCallback, context: *mut c_void) -> bool {
    if RUNNING.get().is_some() {
        return true;
    }
    let Some(config) = text(config_json).and_then(|json| serde_json::from_str::<Config>(&json).ok()) else {
        return false;
    };
    init_logging();
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build() else {
        return false;
    };

    let context = Context(context);
    let sink = Arc::new(move |event: Event| {
        let context = &context;
        let Ok(json) = serde_json::to_string(&event) else { return };
        let Ok(json) = CString::new(json) else { return };
        callback(json.as_ptr(), context.0);
    });
    let started = {
        let _guard = runtime.enter();
        core::start(config, sink)
    };
    match started {
        Ok(handle) => RUNNING.set(Running { _runtime: runtime, handle }).is_ok(),
        Err(error) => {
            tracing::error!("the core couldn't start: {error:#}");
            false
        }
    }
}

/// Sends one command: an `api::Envelope` as JSON. The answer arrives as a `reply` event.
#[unsafe(no_mangle)]
pub extern "C" fn motile_send(command_json: *const c_char) {
    let Some(running) = RUNNING.get() else { return };
    let Some(json) = text(command_json) else { return };
    match serde_json::from_str::<Envelope>(&json) {
        Ok(envelope) => running.handle.send(envelope.id, envelope.command),
        Err(error) => tracing::warn!("ignored a malformed command: {error}"),
    }
}

fn init_logging() {
    let filter = std::env::var("MOTILE_LOG").unwrap_or_else(|_| "warn,motile_core=info".to_string());
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).with_ansi(false).try_init();
}
