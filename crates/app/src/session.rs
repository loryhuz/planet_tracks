//! The tuning session: every gameplay profile with its tuned parameters, its best run on each
//! map and whether the player eliminated it. Saved to tuning/session.json so it survives
//! restarts (on iOS, to the app's Documents folder).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use physics::CarParams;
use serde::{Deserialize, Serialize};

use crate::race::Frame;

/// Maps are keyed by name and version: records do not carry over when a map's blocks change.
pub fn map_key(map: &track::Map) -> String {
    format!("{}@{}", map.name, map.version)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Best {
    pub ticks: u32,
    pub splits: Vec<u32>,
    /// The parameters the record was driven with (JSON), to tell whether the ghost still applies.
    #[serde(rename = "params")]
    pub params_json: String,
    pub frames: Vec<Frame>,
}

pub struct Profile {
    pub params: CarParams,
    pub defaults: CarParams,
    /// Best run per map key.
    pub bests: BTreeMap<String, Best>,
    pub runs: u32,
    pub finishes: u32,
    pub eliminated: bool,
}

impl Profile {
    pub fn params_json(&self) -> String {
        serde_json::to_string(&self.params).unwrap_or_default()
    }

    pub fn best(&self, map: &str) -> Option<&Best> {
        self.bests.get(map)
    }

    /// The best run on `map`, if it was driven with the current parameters.
    pub fn current_best(&self, map: &str) -> Option<&Best> {
        let json = self.params_json();
        self.bests.get(map).filter(|b| b.params_json == json)
    }
}

pub struct Session {
    pub profiles: Vec<Profile>,
    pub current: usize,
    /// Key of the map last played.
    pub map: String,
    /// Off for self-test runs: they never write the player's session file.
    pub persist: bool,
    dirty_since: Option<Instant>,
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    current: usize,
    /// The current profile by name (indices move when profiles are added).
    #[serde(default)]
    current_name: String,
    #[serde(default)]
    map: String,
    profiles: Vec<SavedProfile>,
}

#[derive(Serialize, Deserialize)]
struct SavedProfile {
    name: String,
    params: serde_json::Value,
    /// The preset the tuned params started from; when the code's preset changes, the old
    /// tuning is dropped so a new physics version is never hidden behind stale values.
    #[serde(default)]
    defaults: String,
    runs: u32,
    finishes: u32,
    eliminated: bool,
    #[serde(default)]
    bests: BTreeMap<String, Best>,
    // Before maps existed, the one best run was on Jezero version 1.
    #[serde(default, skip_serializing)]
    best_ticks: Option<u32>,
    #[serde(default, skip_serializing)]
    best_splits: Vec<u32>,
    #[serde(default, skip_serializing)]
    best_params: String,
    #[serde(default, skip_serializing)]
    best_frames: Vec<Frame>,
}

pub fn path() -> PathBuf {
    if cfg!(target_os = "ios") {
        // HOME is the app's container.
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        return home.join("Documents/session.json");
    }
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tuning/session.json"))
}

impl Session {
    pub fn load() -> Self {
        let saved: Saved = std::fs::read_to_string(path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let profiles = physics::profiles()
            .into_iter()
            .map(|preset| {
                let s = saved.profiles.iter().find(|p| p.name == preset.name);
                let preset_json = serde_json::to_string(&preset).unwrap_or_default();
                let params = s
                    .filter(|s| s.defaults == preset_json)
                    .and_then(|s| serde_json::from_value::<CarParams>(s.params.clone()).ok())
                    .unwrap_or_else(|| preset.clone());
                let mut bests = s.map(|s| s.bests.clone()).unwrap_or_default();
                if let Some(s) = s {
                    if let Some(ticks) = s.best_ticks {
                        bests.entry("Jezero@1".into()).or_insert(Best {
                            ticks,
                            splits: s.best_splits.clone(),
                            params_json: s.best_params.clone(),
                            frames: s.best_frames.clone(),
                        });
                    }
                }
                Profile {
                    params,
                    defaults: preset,
                    bests,
                    runs: s.map_or(0, |s| s.runs),
                    finishes: s.map_or(0, |s| s.finishes),
                    eliminated: s.is_some_and(|s| s.eliminated),
                }
            })
            .collect::<Vec<_>>();
        // By name; a session saved before names were stored starts on the first profile.
        let current = profiles.iter().position(|p| p.defaults.name == saved.current_name).unwrap_or(0);
        Self { profiles, current, map: saved.map, persist: true, dirty_since: None }
    }

    /// Makes the car of `planet` the current profile (a map is always driven with its planet's car).
    pub fn use_car_for(&mut self, planet: track::Planet) {
        let name = physics::car_for(planet).name;
        if let Some(i) = self.profiles.iter().position(|p| p.defaults.name == name) {
            self.current = i;
        }
    }

    pub fn profile(&self) -> &Profile {
        &self.profiles[self.current]
    }

    pub fn profile_mut(&mut self) -> &mut Profile {
        &mut self.profiles[self.current]
    }

    pub fn mark_dirty(&mut self) {
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    /// Saves a moment after the last change (sliders fire every frame while dragged).
    pub fn autosave(&mut self) {
        if self.dirty_since.is_some_and(|t| t.elapsed() > Duration::from_millis(800)) {
            self.save();
        }
    }

    pub fn save(&mut self) {
        self.dirty_since = None;
        if !self.persist {
            return;
        }
        let saved = Saved {
            current: self.current,
            current_name: self.profiles[self.current].defaults.name.clone(),
            map: self.map.clone(),
            profiles: self
                .profiles
                .iter()
                .map(|p| SavedProfile {
                    name: p.defaults.name.clone(),
                    params: serde_json::to_value(&p.params).unwrap_or_default(),
                    defaults: serde_json::to_string(&p.defaults).unwrap_or_default(),
                    runs: p.runs,
                    finishes: p.finishes,
                    eliminated: p.eliminated,
                    bests: p.bests.clone(),
                    best_ticks: None,
                    best_splits: Vec::new(),
                    best_params: String::new(),
                    best_frames: Vec::new(),
                })
                .collect(),
        };
        let path = path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&saved) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_saved_with_the_textures_setting_still_loads() {
        // Sessions saved while the procedural look could be chosen carry `textures_off`.
        let json = r#"{"current": 1, "current_name": "B", "map": "Noctis@1", "textures_off": true, "profiles": []}"#;
        let saved: Saved = serde_json::from_str(json).expect("an old session loads");
        assert_eq!((saved.current, saved.current_name.as_str(), saved.map.as_str()), (1, "B", "Noctis@1"));
    }
}
