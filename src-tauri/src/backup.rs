//! Backup of everything: all profiles, the active one, and the app settings
//! (notifications, shortcuts), as one JSON file.

use std::collections::BTreeMap;

use lm_protocol::AppState;
use serde::{Deserialize, Serialize};

const FORMAT: &str = "linuxmeeter-backup";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    format: String,
    version: u32,
    pub active_profile: String,
    /// Profile name -> profile file (TOML).
    pub profiles: BTreeMap<String, String>,
    /// app.toml contents.
    pub app: crate::settings::Settings,
}

/// Collect a backup. The active profile comes from the live state, so it is
/// current even if the engine has not saved its latest changes yet.
pub fn collect(state: &AppState) -> Result<Backup, String> {
    let mut profiles = BTreeMap::new();
    for name in lm_engine::persist::list_profiles() {
        if let Some(text) = lm_engine::persist::read_profile_text(&name) {
            profiles.insert(name, text);
        }
    }
    let live = lm_engine::persist::Profile {
        take_default_output: state.take_default_output,
        strips: state.strips.clone(),
        buses: state.buses.clone(),
    };
    profiles.insert(state.profile_name.clone(), lm_engine::persist::profile_toml(&live).map_err(|e| e.to_string())?);
    Ok(Backup {
        format: FORMAT.into(),
        version: 1,
        active_profile: state.profile_name.clone(),
        profiles,
        app: crate::settings::get(),
    })
}

pub fn parse(text: &str) -> Result<Backup, String> {
    let b: Backup = serde_json::from_str(text).map_err(|_| "this is not a linuxmeeter backup file".to_string())?;
    if b.format != FORMAT {
        return Err("this is not a linuxmeeter backup file".into());
    }
    if b.version > 1 {
        return Err("this backup was made by a newer version of linuxmeeter".into());
    }
    for (name, text) in &b.profiles {
        toml::from_str::<lm_engine::persist::Profile>(text).map_err(|e| format!("profile \"{name}\" in the backup is damaged: {e}"))?;
    }
    if !b.profiles.contains_key(&b.active_profile) {
        return Err("the backup's active profile is missing".into());
    }
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_other_json_and_damaged_profiles() {
        assert!(parse("{}").is_err());
        assert!(parse("not json").is_err());
        let mut b = Backup {
            format: FORMAT.into(),
            version: 1,
            active_profile: "Default".into(),
            profiles: BTreeMap::from([("Default".into(), "strips = 5".into())]),
            app: Default::default(),
        };
        assert!(parse(&serde_json::to_string(&b).unwrap()).unwrap_err().contains("damaged"));
        b.profiles.insert("Default".into(), "strips = []\nbuses = []\n".into());
        assert!(parse(&serde_json::to_string(&b).unwrap()).is_ok());
        b.active_profile = "Gone".into();
        assert!(parse(&serde_json::to_string(&b).unwrap()).is_err());
    }
}
