//! The Android app's way into the core: the JNI functions of `app.motile.core.Native`, around
//! the C functions of `motile_core::ffi`. Events wait in a queue that the app's event thread
//! takes them from, one `next` at a time, so that no thread of the core calls into Java.

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};

use jni::EnvUnowned;
use jni::errors::ThrowRuntimeExAndDefault;
use jni::objects::{JClass, JObject, JString};

struct Queue {
    sender: Sender<String>,
    receiver: Mutex<Receiver<String>>,
}

static EVENTS: OnceLock<Queue> = OnceLock::new();

fn events() -> &'static Queue {
    EVENTS.get_or_init(|| {
        let (sender, receiver) = channel();
        Queue { sender, receiver: Mutex::new(receiver) }
    })
}

extern "C" fn received(json: *const c_char, _context: *mut c_void) {
    if json.is_null() {
        return;
    }
    let text = unsafe { CStr::from_ptr(json) }.to_string_lossy().into_owned();
    let _ = events().sender.send(text);
}

/// Starts the core with an `api::Config` as JSON. `context` is the application's, which the
/// system's certificate checks need.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_motile_core_Native_start<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    context: JObject<'caller>,
    config: JString<'caller>,
) -> bool {
    env.with_env(|_env| -> Result<bool, jni::errors::Error> {
        #[cfg(target_os = "android")]
        {
            logcat::forward_stderr();
            give_context(_env, &context)?;
            rustls_platform_verifier::android::init_with_env(_env, context)?;
        }
        #[cfg(not(target_os = "android"))]
        let _ = context;
        let Ok(config) = CString::new(config.to_string()) else { return Ok(false) };
        events();
        Ok(motile_core::ffi::motile_start(config.as_ptr(), received, std::ptr::null_mut()))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// Sends one command, an `api::Envelope` as JSON.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_motile_core_Native_send<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    command: JString<'caller>,
) {
    env.with_env(|_env| -> Result<(), jni::errors::Error> {
        let Ok(command) = CString::new(command.to_string()) else { return Ok(()) };
        motile_core::ffi::motile_send(command.as_ptr());
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// Waits for the next event and answers with it as JSON.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_motile_core_Native_next<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
) -> JString<'caller> {
    env.with_env(|env| -> Result<JString<'caller>, jni::errors::Error> {
        let event = events().receiver.lock().ok().and_then(|receiver| receiver.recv().ok()).unwrap_or_default();
        JString::from_str(env, event)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// Gives the JVM and the application to the crates that ask Android itself through `ndk_context`,
/// like iroh's DNS resolver, which reads the system's nameservers. The reference is kept for good.
#[cfg(target_os = "android")]
fn give_context(env: &mut jni::Env, context: &JObject) -> Result<(), jni::errors::Error> {
    static GIVEN: std::sync::Once = std::sync::Once::new();
    let mut result = Ok(());
    GIVEN.call_once(|| {
        result = (|| {
            let vm = env.get_java_vm()?.get_raw();
            let context = env.new_global_ref(context)?.into_raw();
            unsafe { ndk_context::initialize_android_context(vm.cast(), context.cast()) };
            Ok(())
        })();
    });
    result
}

/// The core logs to stderr, which Android throws away: this sends it to logcat instead.
#[cfg(target_os = "android")]
mod logcat {
    use std::ffi::{CString, c_char};
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::os::fd::FromRawFd;
    use std::sync::Once;

    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(priority: i32, tag: *const c_char, text: *const c_char) -> i32;
    }

    const INFO: i32 = 4;

    pub fn forward_stderr() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let mut fds = [0; 2];
            if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 || unsafe { libc::dup2(fds[1], libc::STDERR_FILENO) } < 0 {
                return;
            }
            let reader = BufReader::new(unsafe { File::from_raw_fd(fds[0]) });
            std::thread::spawn(move || {
                let tag = c"motile";
                for line in reader.lines().map_while(Result::ok) {
                    let Ok(text) = CString::new(line) else { continue };
                    unsafe { __android_log_write(INFO, tag.as_ptr(), text.as_ptr()) };
                }
            });
        });
    }
}
