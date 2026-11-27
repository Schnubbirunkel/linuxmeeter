//! Desktop notifications (org.freedesktop.Notifications), shown whether or
//! not the window is visible: profile switches, and what a global shortcut
//! just did.
//!
//! Sending happens on its own thread so a slow notification daemon never
//! holds up engine events. Notes with the same `key` replace each other, so
//! holding a volume key updates one bubble instead of stacking dozens.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
        let enabled = Arc::new(AtomicBool::new(crate::settings::get().notifications));
        std::thread::Builder::new()
            .name("lm-notify".into())
            .spawn(move || {
                // key -> (notification id, when its bubble times out). Only a
                // bubble still on screen is replaced: KDE Plasma treats a
                // replace of an expired notification as a silent update, so
                // reusing old ids made every later profile switch invisible.
                let mut ids: HashMap<String, (u32, Instant)> = HashMap::new();
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
                    let now = Instant::now();
                    if let Some((id, until)) = ids.get(&note.key) {
                        if now < *until {
                            n.id(*id);
                        }
                    }
                    match n.show() {
                        Ok(handle) => {
                            // A little short of the timeout, so a replace never
                            // races the bubble closing.
                            let visible = Duration::from_millis(note.timeout_ms.max(0) as u64)
                                .saturating_sub(Duration::from_millis(300));
                            ids.insert(note.key, (handle.id(), now + visible));
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
        crate::settings::update(|s| s.notifications = on)
    }
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
