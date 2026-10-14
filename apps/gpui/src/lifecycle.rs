//! Tells the core when the computer wakes or changes networks, so that it dials again the moment
//! the client is back instead of waiting to find out. Coming to the front is told by the store.

use futures::StreamExt;
use futures::channel::mpsc::UnboundedSender;
use gpui_kit::*;
use motile_core::api::Command;

use crate::store::Store;

enum Signal {
    Woke,
    /// The network the computer is on now, or "down": on the Mac by the kind of its first
    /// interface, elsewhere by the interface of the default route and its addresses.
    Path(String),
}

pub fn start(store: &Entity<Store>, cx: &mut App) {
    let (sender, mut signals) = futures::channel::mpsc::unbounded();
    watch(sender);
    let store = store.downgrade();
    cx.spawn(async move |cx| {
        let mut path: Option<String> = None;
        while let Some(signal) = signals.next().await {
            let command = match signal {
                Signal::Woke => Command::Foreground,
                Signal::Path(name) => {
                    let changed = path.as_ref().is_some_and(|path| *path != name);
                    path = Some(name);
                    if !changed {
                        continue;
                    }
                    Command::NetworkChanged
                }
            };
            if store.update(cx, |store, _| store.send(command)).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// iroh's network monitor, which also says when the computer woke.
#[cfg(not(target_os = "macos"))]
fn watch(sender: UnboundedSender<Signal>) {
    use n0_watcher::Watcher;
    use netwatch::interfaces::State;

    fn name(state: &State) -> String {
        let interface = state.default_route_interface.as_ref().and_then(|name| state.interfaces.get(name));
        let Some(interface) = interface else {
            return if state.have_v4 || state.have_v6 { "up".into() } else { "down".into() };
        };
        let mut addresses: Vec<String> = interface.addrs().map(|address| address.addr().to_string()).collect();
        addresses.sort();
        format!("{} {}", interface.name(), addresses.join(" "))
    }

    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
        runtime.block_on(async move {
            let Ok(monitor) = netwatch::netmon::Monitor::new().await else { return };
            let mut states = monitor.interface_state();
            let mut last = states.get();
            let _ = sender.unbounded_send(Signal::Path(name(&last)));
            while let Ok(state) = states.updated().await {
                let woke = state.last_unsuspend.is_some() && state.last_unsuspend != last.last_unsuspend;
                if woke && sender.unbounded_send(Signal::Woke).is_err() {
                    return;
                }
                if sender.unbounded_send(Signal::Path(name(&state))).is_err() {
                    return;
                }
                last = state;
            }
        });
    });
}

#[cfg(target_os = "macos")]
fn watch(sender: UnboundedSender<Signal>) {
    use std::ptr::NonNull;

    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification};
    use objc2_foundation::NSNotification;

    let woke = sender.clone();
    let block = block2::RcBlock::new(move |_: NonNull<NSNotification>| {
        let _ = woke.unbounded_send(Signal::Woke);
    });
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // The observer lasts as long as the app.
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(NSWorkspaceDidWakeNotification), None, None, &block)
    };
    std::mem::forget(observer);
    network::watch(move |name| {
        let _ = sender.unbounded_send(Signal::Path(name));
    });
}

/// Network.framework's path monitor, which says which network the Mac is on whenever it changes.
#[cfg(target_os = "macos")]
mod network {
    use std::ffi::c_void;

    type Object = *mut c_void;

    const SATISFIED: i32 = 1;

    #[link(name = "Network", kind = "framework")]
    unsafe extern "C" {
        fn nw_path_monitor_create() -> Object;
        fn nw_path_monitor_set_update_handler(monitor: Object, handler: &block2::DynBlock<dyn Fn(Object)>);
        fn nw_path_monitor_set_queue(monitor: Object, queue: Object);
        fn nw_path_monitor_start(monitor: Object);
        fn nw_path_get_status(path: Object) -> i32;
        fn nw_path_enumerate_interfaces(path: Object, enumerate: &block2::DynBlock<dyn Fn(Object) -> u8>);
        fn nw_interface_get_type(interface: Object) -> i32;
    }

    unsafe extern "C" {
        fn dispatch_queue_create(label: *const std::ffi::c_char, attributes: Object) -> Object;
    }

    pub fn watch(changed: impl Fn(String) + 'static) {
        let handler = block2::RcBlock::new(move |path: Object| changed(name(path)));
        // The monitor and its queue last as long as the app.
        unsafe {
            let monitor = nw_path_monitor_create();
            let queue = dispatch_queue_create(c"app.motile.network".as_ptr(), std::ptr::null_mut());
            nw_path_monitor_set_update_handler(monitor, &handler);
            nw_path_monitor_set_queue(monitor, queue);
            nw_path_monitor_start(monitor);
        }
    }

    fn name(path: Object) -> String {
        if unsafe { nw_path_get_status(path) } != SATISFIED {
            return "down".into();
        }
        let first = std::rc::Rc::new(std::cell::Cell::new(None));
        let found = first.clone();
        let enumerate = block2::RcBlock::new(move |interface: Object| {
            found.set(Some(unsafe { nw_interface_get_type(interface) }));
            // C's `false`: the first is enough.
            0
        });
        unsafe { nw_path_enumerate_interfaces(path, &enumerate) };
        match first.get() {
            None => "up",
            Some(1) => "wifi",
            Some(2) => "cellular",
            Some(3) => "wiredEthernet",
            Some(4) => "loopback",
            Some(_) => "other",
        }
        .into()
    }
}
