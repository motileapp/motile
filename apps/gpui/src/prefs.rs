//! What the app remembers between runs: a JSON object in a file, read once and written in the
//! background whenever something changes.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

pub struct Prefs {
    file: PathBuf,
    values: Map<String, Value>,
    /// The latest text to write, shared with the thread that writes it.
    pending: Arc<Mutex<Option<String>>>,
}

impl Prefs {
    pub fn load(file: PathBuf) -> Self {
        let values = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str::<Map<String, Value>>(&text).ok())
            .unwrap_or_default();
        Self { file, values, pending: Arc::default() }
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.values.get(key).and_then(|value| serde_json::from_value(value.clone()).ok())
    }

    pub fn string(&self, key: &str) -> Option<String> {
        self.get(key)
    }

    pub fn bool(&self, key: &str) -> bool {
        self.get(key).unwrap_or(false)
    }

    pub fn f32_or(&self, key: &str, default: f32) -> f32 {
        self.get(key).unwrap_or(default)
    }

    pub fn set<T: Serialize>(&mut self, key: &str, value: T) {
        let value = serde_json::to_value(value).unwrap_or(Value::Null);
        if value.is_null() {
            if self.values.remove(key).is_none() {
                return;
            }
        } else if self.values.get(key) == Some(&value) {
            return;
        } else {
            self.values.insert(key.to_string(), value);
        }
        self.save();
    }

    pub fn remove(&mut self, key: &str) {
        if self.values.remove(key).is_some() {
            self.save();
        }
    }

    fn save(&self) {
        let Ok(text) = serde_json::to_string_pretty(&self.values) else { return };
        let was_waiting = self.pending.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).replace(text).is_some();
        if was_waiting {
            return;
        }
        let pending = self.pending.clone();
        let file = self.file.clone();
        std::thread::spawn(move || {
            // Changes that come in a burst are written once.
            std::thread::sleep(std::time::Duration::from_millis(200));
            let Some(text) = pending.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take() else { return };
            if let Some(folder) = file.parent() {
                let _ = std::fs::create_dir_all(folder);
            }
            let temporary = file.with_extension("json.new");
            if std::fs::write(&temporary, text).is_ok() {
                let _ = std::fs::rename(&temporary, &file);
            }
        });
    }
}
