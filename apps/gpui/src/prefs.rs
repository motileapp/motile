//! What the app remembers between runs: a JSON object in a file, read once and written in the
//! background a moment after something changes, so that a burst of changes, like a window
//! being resized, is written once and never turned into text on the main thread.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

pub struct Prefs {
    file: PathBuf,
    /// The values, shared with the thread that writes them.
    values: Arc<Mutex<Map<String, Value>>>,
    /// A write is on its way, which takes every change made until it starts.
    writing: Arc<Mutex<bool>>,
}

impl Prefs {
    pub fn load(file: PathBuf) -> Self {
        let values = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str::<Map<String, Value>>(&text).ok())
            .unwrap_or_default();
        Self { file, values: Arc::new(Mutex::new(values)), writing: Arc::default() }
    }

    fn values(&self) -> MutexGuard<'_, Map<String, Value>> {
        self.values.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let value = self.values().get(key).cloned()?;
        serde_json::from_value(value).ok()
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
        {
            let mut values = self.values();
            if value.is_null() {
                if values.remove(key).is_none() {
                    return;
                }
            } else if values.get(key) == Some(&value) {
                return;
            } else {
                values.insert(key.to_string(), value);
            }
        }
        self.save();
    }

    pub fn remove(&mut self, key: &str) {
        if self.values().remove(key).is_some() {
            self.save();
        }
    }

    fn save(&self) {
        {
            let mut writing = self.writing.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if *writing {
                return;
            }
            *writing = true;
        }
        let (values, writing, file) = (self.values.clone(), self.writing.clone(), self.file.clone());
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            *writing.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
            let kept = values.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            let Ok(text) = serde_json::to_string_pretty(&kept) else { return };
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
