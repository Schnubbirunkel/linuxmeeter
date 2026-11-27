//! UI-side settings (`~/.config/linuxmeeter/app.toml`), kept apart from the
//! engine's config.toml so the two threads never write the same file.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "yes")]
    pub notifications: bool,
    /// Warning tone when the microphones are muted or go live.
    #[serde(default = "yes")]
    pub mic_tone: bool,
    /// Update channel: "stable" (releases) or "edge" (every push to main).
    #[serde(default = "stable")]
    pub update_channel: crate::updater::Channel,
    /// Phone remote: on/off, port, and the key in its URL / QR code.
    #[serde(default)]
    pub remote_enabled: bool,
    #[serde(default = "remote_port")]
    pub remote_port: u16,
    #[serde(default)]
    pub remote_key: String,
    /// Plasma's own volume-key bindings while linuxmeeter holds them
    /// (kmix action -> key sequences), to give back.
    #[serde(default)]
    pub kmix_volume_keys: BTreeMap<String, Vec<Vec<i32>>>,
    /// Global shortcuts: ctl action ("toggle-mute strip:3") -> keys ("Ctrl+Alt+M").
    #[serde(default)]
    pub shortcuts: BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}

fn remote_port() -> u16 {
    crate::remote::DEFAULT_PORT
}

fn stable() -> crate::updater::Channel {
    crate::updater::Channel::Stable
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            notifications: true,
            mic_tone: true,
            update_channel: crate::updater::Channel::Stable,
            remote_enabled: false,
            remote_port: crate::remote::DEFAULT_PORT,
            remote_key: String::new(),
            kmix_volume_keys: BTreeMap::new(),
            shortcuts: BTreeMap::new(),
        }
    }
}

fn path() -> PathBuf {
    lm_engine::persist::config_dir().join("app.toml")
}

fn cell() -> &'static Mutex<Settings> {
    static CELL: OnceLock<Mutex<Settings>> = OnceLock::new();
    CELL.get_or_init(|| {
        let s = std::fs::read_to_string(path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        Mutex::new(s)
    })
}

pub fn get() -> Settings {
    cell().lock().expect("settings lock").clone()
}

/// Change the settings and write them to disk.
pub fn update(f: impl FnOnce(&mut Settings)) -> Result<(), String> {
    let mut s = cell().lock().expect("settings lock");
    f(&mut s);
    let path = path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(&*s).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}
