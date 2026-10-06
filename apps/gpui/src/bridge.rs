//! The app's end of the core. The core runs on a tokio runtime of its own; its events arrive on a
//! channel that the store drains on the main thread. A command's answer comes back as a `reply`
//! event, which is handed to the closure the command was sent with. A large answer is read into
//! what the app keeps on the core's thread, before it reaches the main thread.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use motile_core::api::{Command, Config, Event};
use motile_core::core::Handle;
use serde_json::Value;
use tokio::runtime::Runtime;

use crate::transcript::model::{self, Prepared};

pub type Read = Box<dyn Any + Send>;
type Reader = Box<dyn FnOnce(Value) -> Read + Send>;

pub enum Incoming {
    Event(Box<Event>),
    /// The answer to a command sent with `send_read`, already read.
    Read {
        id: u64,
        result: Result<Read, String>,
    },
    /// Rows of a transcript, made ready to draw.
    Rows(Box<Prepared>),
}

pub struct Bridge {
    _runtime: Runtime,
    handle: Handle,
    next_id: u64,
    readers: Arc<Mutex<HashMap<u64, Reader>>>,
}

impl Bridge {
    pub fn start(config: Config) -> anyhow::Result<(Self, UnboundedReceiver<Incoming>)> {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build()?;
        let (sender, events) = unbounded();
        let readers: Arc<Mutex<HashMap<u64, Reader>>> = Arc::default();
        let waiting = readers.clone();
        let sink = Arc::new(move |event: Event| {
            let incoming = match event {
                Event::Reply { id, ok, value } => {
                    let reader = waiting.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&id);
                    match reader {
                        Some(read) if ok => Incoming::Read { id, result: Ok(read(value)) },
                        Some(_) => Incoming::Read { id, result: Err(error_of(&value)) },
                        None => Incoming::Event(Box::new(Event::Reply { id, ok, value })),
                    }
                }
                event => match model::prepare(event) {
                    Ok(prepared) => Incoming::Rows(Box::new(prepared)),
                    Err(event) => Incoming::Event(Box::new(event)),
                },
            };
            let _ = sender.unbounded_send(incoming);
        });
        let handle = {
            let _guard = runtime.enter();
            motile_core::core::start(config, sink)?
        };
        Ok((Self { _runtime: runtime, handle, next_id: 0, readers }, events))
    }

    /// Sends the command and answers with its id, which its reply carries.
    pub fn send(&mut self, command: Command) -> u64 {
        self.next_id += 1;
        self.handle.send(self.next_id, command);
        self.next_id
    }

    /// Sends a command whose answer `read` turns into what the app keeps, off the main thread.
    pub fn send_read<T: Send + 'static>(
        &mut self,
        command: Command,
        read: impl FnOnce(Value) -> T + Send + 'static,
    ) -> u64 {
        self.next_id += 1;
        let reader: Reader = Box::new(move |value| Box::new(read(value)));
        self.readers.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(self.next_id, reader);
        self.handle.send(self.next_id, command);
        self.next_id
    }

    pub fn stop(&self) {
        self.handle.stop();
    }
}

pub fn error_of(value: &Value) -> String {
    value["error"].as_str().unwrap_or("Something went wrong.").to_string()
}

pub fn init_logging() {
    let filter = std::env::var("MOTILE_LOG").unwrap_or_else(|_| "warn,motile_core=info".to_string());
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).with_ansi(false).try_init();
}
