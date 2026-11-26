//! Desktop notifications (org.freedesktop.Notifications), shown whether or
//! not the window is visible: profile switches, and what a global shortcut
//! just did.
//!
//! Sending happens on its own thread so a slow notification daemon never
//! holds up engine events. Notes with the same `key` replace each other, so
//! holding a volume key updates one bubble instead of stacking dozens.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub struct Note {
    pub key: String,
    pub summary: String,
    pub body: String,
    pub icon: &'static str,
    /// 0..=100, drawn as a bar by daemons that support the `value` hint.
    pub value: Option<i32>,
    pub timeout_ms: i32,
}

#[derive(Clone)]
pub struct Notifier {
    tx: mpsc::Sender<Note>,
    pub enabled: Arc<AtomicBool>,
}

impl Notifier {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Note>();
        let enabled = Arc::new(AtomicBool::new(load_settings().notifications));
        std::thread::Builder::new()
            .name("lm-notify".into())
            .spawn(move || {
                let mut ids: HashMap<String, u32> = HashMap::new();
                while let Ok(note) = rx.recv() {
                    let mut n = notify_rust::Notification::new();
                    n.appname("linuxmeeter")
                        .summary(&note.summary)
                        .body(&note.body)
                        .icon(note.icon)
                        .hint(notify_rust::Hint::Category("device".into()))
                        .hint(notify_rust::Hint::Transient(true))
                        .timeout(notify_rust::Timeout::Milliseconds(note.timeout_ms as u32));
                    if let Some(v) = note.value {
                        n.hint(notify_rust::Hint::CustomInt(
                            "value".into(),
                            v.clamp(0, 100),
                        ));
                    }
                    if let Some(id) = ids.get(&note.key) {
                        n.id(*id);
                    }
                    match n.show() {
                        Ok(handle) => {
                            ids.insert(note.key, handle.id());
                        }
                        Err(e) => tracing::warn!("notification failed: {e}"),
                    }
                }
            })
            .expect("spawn notifier");
        Self { tx, enabled }
    }

    pub fn send(&self, note: Note) {
        if self.enabled.load(Ordering::Relaxed) {
            let _ = self.tx.send(note);
        }
    }

    pub fn set_enabled(&self, on: bool) -> Result<(), String> {
        self.enabled.store(on, Ordering::Relaxed);
        save_settings(&Settings { notifications: on })
    }
}

/// UI-side settings, kept apart from the engine's config.toml so the two
/// threads never write the same file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "yes")]
    pub notifications: bool,
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            notifications: true,
        }
    }
}

fn settings_path() -> PathBuf {
    lm_engine::persist::config_dir().join("app.toml")
}

pub fn load_settings() -> Settings {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(s: &Settings) -> Result<(), String> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Fader position as a percentage, the number people think of as "volume".
pub fn percent(db: f32) -> i32 {
    (lm_protocol::taper::db_to_pos(db) * 100.0).round() as i32
}

pub fn fmt_db(db: f32) -> String {
    if db <= lm_protocol::taper::MIN_DB + 1.0 {
        "-∞ dB".into()
    } else {
        format!("{db:+.1} dB")
    }
}
